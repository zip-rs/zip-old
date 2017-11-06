use std::io;
use std::io::prelude::*;
use result::{ZipError, ZipResult};
use podio::{LittleEndian, ReadPodExt};

/*
Central directory header
        central file header signature   4 bytes  (0x02014b50)
        version made by                 2 bytes
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
        file comment length             2 bytes
        disk number start               2 bytes
        internal file attributes        2 bytes
        external file attributes        4 bytes
        relative offset of local header 4 bytes
        file name (variable size)
        extra field (variable size)
        file comment (variable size)
*/

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct CENTRAL_DIRECTORY_HEADER {
    pub version_made_by: u16,
    pub version_to_extract: u16,
    pub general_purpose_flag: u16,
    pub compression_method: u16,
    pub last_mod_time: u16,
    pub last_mod_date: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub file_name_length: u16,
    pub extra_field_length: u16,
    pub file_comment_length: u16,
    pub disk_number_start: u16,
    pub internal_file_attrs: u16,
    pub external_file_attrs: u32,
    pub local_header_offset: u32,
    pub file_name: Vec<u8>,
    pub extra_field: Vec<u8>,
    pub file_comment: Vec<u8>,
}
impl CENTRAL_DIRECTORY_HEADER {
    pub fn signature() -> u32 {
        0x02014b50
    }

    pub fn name() -> &'static str {
        "[Central directory header]"
    }

    pub fn load<T>(reader: &mut T) -> ZipResult<Self>
    where
        T: Read + Seek,
    {
        if Self::signature() != reader.read_u32::<LittleEndian>()? {
            return Err(ZipError::SectionNotFound(Self::name()));
        }
        let mut header = CENTRAL_DIRECTORY_HEADER {
            version_made_by: reader.read_u16::<LittleEndian>()?,
            version_to_extract: reader.read_u16::<LittleEndian>()?,
            general_purpose_flag: reader.read_u16::<LittleEndian>()?,
            compression_method: reader.read_u16::<LittleEndian>()?,
            last_mod_time: reader.read_u16::<LittleEndian>()?,
            last_mod_date: reader.read_u16::<LittleEndian>()?,
            crc32: reader.read_u32::<LittleEndian>()?,
            compressed_size: reader.read_u32::<LittleEndian>()?,
            uncompressed_size: reader.read_u32::<LittleEndian>()?,
            file_name_length: reader.read_u16::<LittleEndian>()?,
            extra_field_length: reader.read_u16::<LittleEndian>()?,
            file_comment_length: reader.read_u16::<LittleEndian>()?,
            disk_number_start: reader.read_u16::<LittleEndian>()?,
            internal_file_attrs: reader.read_u16::<LittleEndian>()?,
            external_file_attrs: reader.read_u32::<LittleEndian>()?,
            local_header_offset: reader.read_u32::<LittleEndian>()?,
            file_name: Vec::new(),
            extra_field: Vec::new(),
            file_comment: Vec::new(),
        };

        let enable = header.local_header_offset == 0xFFFFFFFF;
        header.file_name = ReadPodExt::read_exact(reader, header.file_name_length as usize)?;
        if enable {
            println!("{:08x}", reader.seek(io::SeekFrom::Current(0))?);
        }

        header.extra_field = ReadPodExt::read_exact(reader, header.extra_field_length as usize)?;
        if enable {
            for byte in &header.extra_field {
                println!("{:x}", byte);
            }
        }
        header.file_comment = ReadPodExt::read_exact(reader, header.file_comment_length as usize)?;
        return Ok(header);
    }
}
