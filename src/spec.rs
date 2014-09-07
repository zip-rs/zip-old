use std::io;
use std::io::{IoResult, IoError};
use time::Tm;
use util;

/*
   4.3.7  Local file header:

      local file header signature     4 bytes  (0x04034b50)
      version needed to extract       2 bytes
      general purpose bit flag        2 bytes
      compression method              2 bytes
      last mod file time              2 bytes
      last mod file date              2 bytes
      crc-32                          4 bytes
      compressed size                 4 bytes
      uncompressed size               4 bytes
      file name length                2 bytes
      extra field length              2 bytes

      file name (variable size)
      extra field (variable size)
*/

#[deriving(FromPrimitive, Show)]
pub enum CompressionMethod
{
    Stored = 0,
    Shrunk = 1,
    Reduced1 = 2,
    Reduced2 = 3,
    Reduced3 = 4,
    Reduced4 = 5,
    Imploded = 6,
    Deflated = 8,
    Deflate64 = 9,
    PkwareImploding = 10,
    Bzip2 = 12,
    LZMA = 14,
    IBMTerse = 18,
    LZ77 = 19,
    WavPack = 97,
    PPMdI1 = 98,
    Unknown = 100000,
}

#[deriving(Show)]
pub struct LocalFileHeader
{
    extract_version: u16,

    // general purpose flags
    encrypted: bool, // bit 0
    // bit 1 & 2 unused
    has_descriptor: bool, // bit 3
    // bit 4 unused
    is_compressed_patch: bool, // bit 5
    strong_encryption: bool, // bit 6
    // bit 7 - 10 unused
    is_utf8: bool, // bit 11
    // bit 12 unused
    is_masked: bool, // bit 13
    // bit 14 & 15 unused
    
    compression_method: CompressionMethod,
    last_modified: Tm,
    crc32: u32,
    compressed_size: u32,
    uncompressed_size: u32,
    pub file_name: Vec<u8>,
    extra_field: Vec<u8>,
}


impl LocalFileHeader
{
    pub fn parse<T: Reader>(reader: &mut T) -> IoResult<LocalFileHeader>
    {
        let magic = try!(reader.read_le_u32());
        if magic != 0x04034b50 { return Err(IoError { kind: io::MismatchedFileTypeForOperation, desc: "Invalid local file header", detail: None }) }
        let version = try!(reader.read_le_u16());
        let flags = try!(reader.read_le_u16());
        let compression_method = try!(reader.read_le_u16());
        let last_mod_time = try!(reader.read_le_u16());
        let last_mod_date = try!(reader.read_le_u16());
        let crc = try!(reader.read_le_u32());
        let compressed_size = try!(reader.read_le_u32());
        let uncompressed_size = try!(reader.read_le_u32());
        let file_name_length = try!(reader.read_le_u16());
        let extra_field_length = try!(reader.read_le_u16());
        let file_name = try!(reader.read_exact(file_name_length as uint));
        let extra_field = try!(reader.read_exact(extra_field_length as uint));

        Ok(LocalFileHeader
           {
               extract_version: version,
               encrypted: (flags & (1 << 0)) != 0,
               has_descriptor: (flags & (1 << 3)) != 0,
               is_compressed_patch: (flags & (1 << 5)) != 0,
               strong_encryption: (flags & (1 << 6)) != 0,
               is_utf8: (flags & (1 << 11)) != 0,
               is_masked: (flags & (1 << 13)) != 0,
               compression_method: FromPrimitive::from_u16(compression_method).unwrap_or(Unknown),
               last_modified: util::msdos_datetime_to_tm(last_mod_time, last_mod_date),
               crc32: crc,
               compressed_size: compressed_size,
               uncompressed_size: uncompressed_size,
               file_name: file_name,
               extra_field: extra_field,
           })
    }
}
