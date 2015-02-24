use std::io;
use std::io::prelude::*;
use std::num::FromPrimitive;
use result::{ZipResult, ZipError};
use types::ZipFile;
use compression::CompressionMethod;
use spec;
use util;
use util::ReadIntExt;

pub fn central_header_to_zip_file<R: Read+io::Seek>(reader: &mut R) -> ZipResult<ZipFile>
{
    // Parse central header
    let signature = try!(reader.read_le_u32());
    if signature != spec::CENTRAL_DIRECTORY_HEADER_SIGNATURE
    {
        return Err(ZipError::InvalidZipFile("Invalid Central Directory header"))
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
    let file_name_length = try!(reader.read_le_u16()) as usize;
    let extra_field_length = try!(reader.read_le_u16()) as usize;
    let file_comment_length = try!(reader.read_le_u16()) as usize;
    try!(reader.read_le_u16());
    try!(reader.read_le_u16());
    try!(reader.read_le_u32());
    let offset = try!(reader.read_le_u32()) as u64;
    let file_name_raw = try!(reader.read_exact(file_name_length));
    let extra_field = try!(reader.read_exact(extra_field_length));
    let file_comment_raw  = try!(reader.read_exact(file_comment_length));

    let file_name = match is_utf8
    {
        true => String::from_utf8_lossy(&*file_name_raw).into_owned(),
        false => ::cp437::to_string(&*file_name_raw),
    };
    let file_comment = match is_utf8
    {
        true => String::from_utf8_lossy(&*file_comment_raw).into_owned(),
        false => ::cp437::to_string(&*file_comment_raw),
    };

    // Remember end of central header
    let return_position = try!(reader.seek(io::SeekFrom::Current(0)));

    // Parse local header
    try!(reader.seek(io::SeekFrom::Start(offset)));
    let signature = try!(reader.read_le_u32());
    if signature != spec::LOCAL_FILE_HEADER_SIGNATURE
    {
        return Err(ZipError::InvalidZipFile("Invalid local file header"))
    }

    try!(reader.seek(io::SeekFrom::Current(22)));
    let file_name_length = try!(reader.read_le_u16()) as u64;
    let extra_field_length = try!(reader.read_le_u16()) as u64;
    let magic_and_header = 4 + 22 + 2 + 2;
    let data_start = offset as u64 + magic_and_header + file_name_length + extra_field_length;

    // Construct the result
    let mut result = ZipFile
    {
        encrypted: encrypted,
        compression_method: FromPrimitive::from_u16(compression_method).unwrap_or(CompressionMethod::Unknown),
        last_modified_time: util::msdos_datetime_to_tm(last_mod_time, last_mod_date),
        crc32: crc32,
        compressed_size: compressed_size as u64,
        uncompressed_size: uncompressed_size as u64,
        file_name: file_name,
        file_comment: file_comment,
        header_start: offset as u64,
        data_start: data_start,
    };

    try!(parse_extra_field(&mut result, &*extra_field));

    // Go back after the central header
    try!(reader.seek(io::SeekFrom::Start(return_position)));

    Ok(result)
}

fn parse_extra_field(_file: &mut ZipFile, data: &[u8]) -> ZipResult<()>
{
    let mut reader = io::Cursor::new(data);

    while (reader.position() as usize) < data.len()
    {
        let kind = try!(reader.read_le_u16());
        let len = try!(reader.read_le_u16());
        match kind
        {
            _ => try!(reader.seek(io::SeekFrom::Current(len as i64))),
        };
    }
    Ok(())
}
