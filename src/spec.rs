use std::old_io;
use result::{ZipResult, ZipError};
use std::iter::range_step_inclusive;

pub static LOCAL_FILE_HEADER_SIGNATURE : u32 = 0x04034b50;
pub static CENTRAL_DIRECTORY_HEADER_SIGNATURE : u32 = 0x02014b50;
static CENTRAL_DIRECTORY_END_SIGNATURE : u32 = 0x06054b50;

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
    pub fn parse<T: Reader>(reader: &mut T) -> ZipResult<CentralDirectoryEnd>
    {
        let magic = try!(reader.read_le_u32());
        if magic != CENTRAL_DIRECTORY_END_SIGNATURE
        {
            return Err(ZipError::UnsupportedZipFile("Invalid digital signature header"))
        }
        let disk_number = try!(reader.read_le_u16());
        let disk_with_central_directory = try!(reader.read_le_u16());
        let number_of_files_on_this_disk = try!(reader.read_le_u16());
        let number_of_files = try!(reader.read_le_u16());
        let central_directory_size = try!(reader.read_le_u32());
        let central_directory_offset = try!(reader.read_le_u32());
        let zip_file_comment_length = try!(reader.read_le_u16()) as usize;
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

    pub fn find_and_parse<T: Reader+Seek>(reader: &mut T) -> ZipResult<CentralDirectoryEnd>
    {
        let header_size = 22;
        let bytes_between_magic_and_comment_size = header_size - 6;
        try!(reader.seek(0, old_io::SeekEnd));
        let file_length = try!(reader.tell()) as i64;

        let search_upper_bound = ::std::cmp::max(0, file_length - header_size - ::std::u16::MAX as i64);
        for pos in range_step_inclusive(file_length - header_size, search_upper_bound, -1)
        {
            try!(reader.seek(pos, old_io::SeekSet));
            if try!(reader.read_le_u32()) == CENTRAL_DIRECTORY_END_SIGNATURE
            {
                try!(reader.seek(bytes_between_magic_and_comment_size, old_io::SeekCur));
                let comment_length = try!(reader.read_le_u16()) as i64;
                if file_length - pos - header_size == comment_length
                {
                    try!(reader.seek(pos, old_io::SeekSet));
                    return CentralDirectoryEnd::parse(reader);
                }
            }
        }
        Err(ZipError::UnsupportedZipFile("Could not find central directory end"))
    }

    pub fn write<T: Writer>(&self, writer: &mut T) -> ZipResult<()>
    {
        try!(writer.write_le_u32(CENTRAL_DIRECTORY_END_SIGNATURE));
        try!(writer.write_le_u16(self.disk_number));
        try!(writer.write_le_u16(self.disk_with_central_directory));
        try!(writer.write_le_u16(self.number_of_files_on_this_disk));
        try!(writer.write_le_u16(self.number_of_files));
        try!(writer.write_le_u32(self.central_directory_size));
        try!(writer.write_le_u32(self.central_directory_offset));
        try!(writer.write_le_u16(self.zip_file_comment.len() as u16));
        try!(writer.write_all(self.zip_file_comment.as_slice()));
        Ok(())
    }
}
