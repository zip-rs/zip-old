use std::io;
use types::ZipFile;
use result::ZipResult;
use spec;
use util;

pub fn write_local_file_header<T: Writer>(writer: &mut T, file: &ZipFile) -> ZipResult<()>
{
    try!(writer.write_le_u32(spec::LOCAL_FILE_HEADER_SIGNATURE));
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

pub fn update_local_file_header<T: Writer+Seek>(writer: &mut T, file: &ZipFile) -> ZipResult<()>
{
    static CRC32_OFFSET : i64 = 14;
    try!(writer.seek(file.header_start as i64 + CRC32_OFFSET, io::SeekSet));
    try!(writer.write_le_u32(file.crc32));
    try!(writer.write_le_u32(file.compressed_size as u32));
    try!(writer.write_le_u32(file.uncompressed_size as u32));
    Ok(())
}

pub fn write_central_directory_header<T: Writer>(writer: &mut T, file: &ZipFile) -> ZipResult<()>
{
    try!(writer.write_le_u32(spec::CENTRAL_DIRECTORY_HEADER_SIGNATURE));
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

fn build_extra_field(_file: &ZipFile) -> ZipResult<Vec<u8>>
{
    let writer = io::MemWriter::new();
    // Future work
    Ok(writer.into_inner())
}
