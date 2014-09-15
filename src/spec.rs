use std::io;
use std::io::{IoResult, IoError};
use std::iter::range_step_inclusive;
use compression;
use types::ZipFile;
use util;

static LOCAL_FILE_HEADER_SIGNATURE : u32 = 0x04034b50;
static CENTRAL_DIRECTORY_HEADER_SIGNATURE : u32 = 0x02014b50;
static CENTRAL_DIRECTORY_END_SIGNATURE : u32 = 0x06054b50;

pub fn central_header_to_zip_file<R: Reader+Seek>(reader: &mut R) -> IoResult<ZipFile>
{
    // Parse central header
    let signature = try!(reader.read_le_u32());
    if signature != CENTRAL_DIRECTORY_HEADER_SIGNATURE
    {
        return Err(IoError {
            kind: io::MismatchedFileTypeForOperation,
            desc: "Invalid central directory header",
            detail: None })
    }

    try!(reader.read_le_u16());
    try!(reader.read_le_u16());
    let flags = try!(reader.read_le_u16());
    let encrypted = flags & 1 == 1;
    let is_utf8 = flags & (1 << 11) != 0;
    let compression_method = try!(reader.read_le_u16());
    let last_mod_time = try!(reader.read_le_u16());
    let last_mod_date = try!(reader.read_le_u16());
    let crc32 = try!(reader.read_le_u32());
    let compressed_size = try!(reader.read_le_u32());
    let uncompressed_size = try!(reader.read_le_u32());
    let file_name_length = try!(reader.read_le_u16()) as uint;
    let extra_field_length = try!(reader.read_le_u16()) as uint;
    let file_comment_length = try!(reader.read_le_u16()) as uint;
    try!(reader.read_le_u16());
    try!(reader.read_le_u16());
    try!(reader.read_le_u32());
    let offset = try!(reader.read_le_u32()) as i64;
    let file_name_raw = try!(reader.read_exact(file_name_length));
    let extra_field = try!(reader.read_exact(extra_field_length));
    let file_comment_raw  = try!(reader.read_exact(file_comment_length));

    let file_name = match is_utf8
    {
        true => String::from_utf8_lossy(file_name_raw.as_slice()).into_string(),
        false => ::cp437::to_string(file_name_raw.as_slice()),
    };
    let file_comment = match is_utf8
    {
        true => String::from_utf8_lossy(file_comment_raw.as_slice()).into_string(),
        false => ::cp437::to_string(file_comment_raw.as_slice()),
    };

    // Remember end of central header
    let return_position = try!(reader.tell()) as i64;

    // Parse local header
    try!(reader.seek(offset, io::SeekSet));
    let signature = try!(reader.read_le_u32());
    if signature != LOCAL_FILE_HEADER_SIGNATURE
    {
        return Err(IoError {
            kind: io::MismatchedFileTypeForOperation,
            desc: "Invalid local file header",
            detail: None })
    }

    try!(reader.seek(22, io::SeekCur));
    let file_name_length = try!(reader.read_le_u16()) as u64;
    let extra_field_length = try!(reader.read_le_u16()) as u64;
    let magic_and_header = 4 + 22 + 2 + 2;
    let data_start = offset as u64 + magic_and_header + file_name_length + extra_field_length;

    // Construct the result
    let mut result = ZipFile
    {
        encrypted: encrypted,
        compression_method: FromPrimitive::from_u16(compression_method).unwrap_or(compression::Unknown),
        last_modified_time: util::msdos_datetime_to_tm(last_mod_time, last_mod_date),
        crc32: crc32,
        compressed_size: compressed_size as u64,
        uncompressed_size: uncompressed_size as u64,
        file_name: file_name,
        file_comment: file_comment,
        header_start: offset as u64,
        data_start: data_start,
    };

    try!(parse_extra_field(&mut result, extra_field.as_slice()));

    // Go back after the central header
    try!(reader.seek(return_position, io::SeekSet));

    Ok(result)
}

fn parse_extra_field(_file: &mut ZipFile, data: &[u8]) -> IoResult<()>
{
    let mut reader = io::BufReader::new(data);
    while !reader.eof()
    {
        let kind = try!(reader.read_le_u16());
        let len = try!(reader.read_le_u16());
        debug!("Parsing extra block {:04x}", kind);
        match kind
        {
            _ => try!(reader.seek(len as i64, io::SeekCur)),
        }
    }
    Ok(())
}

pub fn write_local_file_header<T: Writer>(writer: &mut T, file: &ZipFile) -> IoResult<()>
{
    try!(writer.write_le_u32(LOCAL_FILE_HEADER_SIGNATURE));
    try!(writer.write_le_u16(20));
    let flag = if !file.file_name.is_ascii() { 1u16 << 11 } else { 0 };
    try!(writer.write_le_u16(flag));
    try!(writer.write_le_u16(file.compression_method as u16));
    try!(writer.write_le_u16(util::tm_to_msdos_time(file.last_modified_time)));
    try!(writer.write_le_u16(util::tm_to_msdos_date(file.last_modified_time)));
    try!(writer.write_le_u32(file.crc32));
    try!(writer.write_le_u32(file.compressed_size as u32));
    try!(writer.write_le_u32(file.uncompressed_size as u32));
    try!(writer.write_le_u16(file.file_name.as_bytes().len() as u16));
    let extra_field = try!(build_extra_field(file));
    try!(writer.write_le_u16(extra_field.len() as u16));
    try!(writer.write(file.file_name.as_bytes()));
    try!(writer.write(extra_field.as_slice()));

    Ok(())
}

pub fn write_central_directory_header<T: Writer>(writer: &mut T, file: &ZipFile) -> IoResult<()>
{
    try!(writer.write_le_u32(CENTRAL_DIRECTORY_HEADER_SIGNATURE));
    try!(writer.write_le_u16(0x14FF));
    try!(writer.write_le_u16(20));
    let flag = if !file.file_name.is_ascii() { 1u16 << 11 } else { 0 };
    try!(writer.write_le_u16(flag));
    try!(writer.write_le_u16(file.compression_method as u16));
    try!(writer.write_le_u16(util::tm_to_msdos_time(file.last_modified_time)));
    try!(writer.write_le_u16(util::tm_to_msdos_date(file.last_modified_time)));
    try!(writer.write_le_u32(file.crc32));
    try!(writer.write_le_u32(file.compressed_size as u32));
    try!(writer.write_le_u32(file.uncompressed_size as u32));
    try!(writer.write_le_u16(file.file_name.as_bytes().len() as u16));
    let extra_field = try!(build_extra_field(file));
    try!(writer.write_le_u16(extra_field.len() as u16));
    try!(writer.write_le_u16(0));
    try!(writer.write_le_u16(0));
    try!(writer.write_le_u16(0));
    try!(writer.write_le_u32(0));
    try!(writer.write_le_u32(file.header_start as u32));
    try!(writer.write(file.file_name.as_bytes()));
    try!(writer.write(extra_field.as_slice()));

    Ok(())
}

fn build_extra_field(_file: &ZipFile) -> IoResult<Vec<u8>>
{
    let writer = io::MemWriter::new();
    // Future work
    Ok(writer.unwrap())
}

pub struct CentralDirectoryEnd
{
    pub disk_number: u16,
    pub disk_with_central_directory: u16,
    pub number_of_files_on_this_disk: u16,
    pub number_of_files: u16,
    pub central_directory_size: u32,
    pub central_directory_offset: u32,
    pub zip_file_comment: Vec<u8>,
}

impl CentralDirectoryEnd
{
    pub fn parse<T: Reader>(reader: &mut T) -> IoResult<CentralDirectoryEnd>
    {
        let magic = try!(reader.read_le_u32());
        if magic != CENTRAL_DIRECTORY_END_SIGNATURE
        {
            return Err(IoError {
                kind: io::MismatchedFileTypeForOperation,
                desc: "Invalid digital signature header",
                detail: None })
        }
        let disk_number = try!(reader.read_le_u16());
        let disk_with_central_directory = try!(reader.read_le_u16());
        let number_of_files_on_this_disk = try!(reader.read_le_u16());
        let number_of_files = try!(reader.read_le_u16());
        let central_directory_size = try!(reader.read_le_u32());
        let central_directory_offset = try!(reader.read_le_u32());
        let zip_file_comment_length = try!(reader.read_le_u16()) as uint;
        let zip_file_comment = try!(reader.read_exact(zip_file_comment_length));

        Ok(CentralDirectoryEnd
           {
               disk_number: disk_number,
               disk_with_central_directory: disk_with_central_directory,
               number_of_files_on_this_disk: number_of_files_on_this_disk,
               number_of_files: number_of_files,
               central_directory_size: central_directory_size,
               central_directory_offset: central_directory_offset,
               zip_file_comment: zip_file_comment,
           })
    }

    pub fn find_and_parse<T: Reader+Seek>(reader: &mut T) -> IoResult<CentralDirectoryEnd>
    {
        let header_size = 22;
        let bytes_between_magic_and_comment_size = header_size - 6;
        try!(reader.seek(0, io::SeekEnd));
        let file_length = try!(reader.tell()) as i64;

        let search_upper_bound = ::std::cmp::max(0, file_length - header_size - ::std::u16::MAX as i64);
        for pos in range_step_inclusive(file_length - header_size, search_upper_bound, -1)
        {
            try!(reader.seek(pos, io::SeekSet));
            if try!(reader.read_le_u32()) == CENTRAL_DIRECTORY_END_SIGNATURE
            {
                try!(reader.seek(bytes_between_magic_and_comment_size, io::SeekCur));
                let comment_length = try!(reader.read_le_u16()) as i64;
                if file_length - pos - header_size == comment_length
                {
                    try!(reader.seek(pos, io::SeekSet));
                    return CentralDirectoryEnd::parse(reader);
                }
            }
        }
        Err(IoError
            {
                kind: io::MismatchedFileTypeForOperation,
                desc: "Could not find central directory end",
                detail: None
            })
    }

    pub fn write<T: Writer>(&self, writer: &mut T) -> IoResult<()>
    {
        try!(writer.write_le_u32(CENTRAL_DIRECTORY_END_SIGNATURE));
        try!(writer.write_le_u16(self.disk_number));
        try!(writer.write_le_u16(self.disk_with_central_directory));
        try!(writer.write_le_u16(self.number_of_files_on_this_disk));
        try!(writer.write_le_u16(self.number_of_files));
        try!(writer.write_le_u32(self.central_directory_size));
        try!(writer.write_le_u32(self.central_directory_offset));
        try!(writer.write_le_u16(self.zip_file_comment.len() as u16));
        try!(writer.write(self.zip_file_comment.as_slice()));
        Ok(())
    }
}
