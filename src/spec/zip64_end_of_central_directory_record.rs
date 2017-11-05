use std::io;
use std::io::prelude::*;
use result::{ZipError, ZipResult};
use podio::{LittleEndian, ReadPodExt};

/*
Zip64 end of central directory record

        zip64 end of central dir
        signature                       4 bytes  (0x06064b50)
        size of zip64 end of central
        directory record                8 bytes
        version made by                 2 bytes
        version needed to extract       2 bytes
        number of this disk             4 bytes
        number of the disk with the
        start of the central directory  4 bytes
        total number of entries in the
        central directory on this disk  8 bytes
        total number of entries in the
        central directory               8 bytes
        size of the central directory   8 bytes
        offset of start of central
        directory with respect to
        the starting disk number        8 bytes
        zip64 extensible data sector    (variable size)
*/

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct ZIP64_END_OF_CENTRAL_DIRECTORY_RECORD {
    pub size_of_rest_structure: u64, // SizeOfFixedFields + SizeOfVariableData - 12
    pub version_made_by: u16,
    pub version_need_to_extract: u16,
    pub disk_number: u32,
    pub disk_with_central_directory: u32,
    pub number_of_files_on_this_disk: u64,
    pub number_of_files: u64,
    pub central_directory_size: u64,
    pub central_directory_offset: u64,
}
impl ZIP64_END_OF_CENTRAL_DIRECTORY_RECORD {
    pub fn signature() -> u32 {
        0x06064b50
    }

    pub fn name() -> &'static str {
        "[Zip64 end of central directory record]"
    }

    pub fn load<T>(reader: &mut T, pos: &u64) -> ZipResult<Self>
    where
        T: Read + Seek,
    {
        reader.seek(io::SeekFrom::Start(*pos))?;
        if Self::signature() != reader.read_u32::<LittleEndian>()? {
            return Err(ZipError::SectionNotFound(Self::name()));
        }
        Ok(
            ZIP64_END_OF_CENTRAL_DIRECTORY_RECORD {
                size_of_rest_structure: reader.read_u64::<LittleEndian>()?,
                version_made_by: reader.read_u16::<LittleEndian>()?,
                version_need_to_extract: reader.read_u16::<LittleEndian>()?,
                disk_number: reader.read_u32::<LittleEndian>()?,
                disk_with_central_directory: reader.read_u32::<LittleEndian>()?,
                number_of_files_on_this_disk: reader.read_u64::<LittleEndian>()?,
                number_of_files: reader.read_u64::<LittleEndian>()?,
                central_directory_size: reader.read_u64::<LittleEndian>()?,
                central_directory_offset: reader.read_u64::<LittleEndian>()?,
            }
        )
    }
}
