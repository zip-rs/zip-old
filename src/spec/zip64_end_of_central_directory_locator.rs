use std::io;
use std::io::prelude::*;
use result::{ZipError, ZipResult};
use podio::{LittleEndian, ReadPodExt};

/*
Zip64 end of central directory locator
      zip64 end of central dir locator
      signature                       4 bytes  (0x07064b50)
      number of the disk with the
      start of the zip64 end of
      central directory               4 bytes
      relative offset of the zip64
      end of central directory record 8 bytes
      total number of disks           4 bytes
*/

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct ZIP64_END_OF_CENTRAL_DIRECTORY_LOCATOR {
    /// number of the disk with the start of the zip64 end of central directory
    pub disk_with_central_directory: u32,
    /// relative offset of the zip64 end of central directory record
    pub central_directory_offset: u64,
    /// total number of disks
    pub number_of_disks: u32,
}
impl ZIP64_END_OF_CENTRAL_DIRECTORY_LOCATOR {
    pub fn size() -> u64 {
        20 // 4 + 4 + 8 + 4
    }
    pub fn signature() -> u32 {
        0x07064b50
    }

    pub fn name() -> &'static str {
        "[Zip64 end of central directory locator]"
    }

    pub fn load<T>(reader: &mut T, pos: &u64) -> ZipResult<Self>
    where
        T: Read + Seek,
    {
        let pos = *pos - Self::size();
        reader.seek(io::SeekFrom::Start(pos))?;
        if Self::signature() != reader.read_u32::<LittleEndian>()? {
            return Err(ZipError::SectionNotFound(Self::name()));
        }
        Ok(ZIP64_END_OF_CENTRAL_DIRECTORY_LOCATOR {
            disk_with_central_directory: reader.read_u32::<LittleEndian>()?,
            central_directory_offset: reader.read_u64::<LittleEndian>()?,
            number_of_disks: reader.read_u32::<LittleEndian>()?,
        })
    }
}
