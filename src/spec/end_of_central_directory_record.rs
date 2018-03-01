use std::io;
use std::io::prelude::*;
use result::{ZipError, ZipResult};
use podio::{LittleEndian, ReadPodExt};

/*
End of central directory record:

      end of central dir signature    4 bytes  (0x06054b50)
      number of this disk             2 bytes
      number of the disk with the
      start of the central directory  2 bytes
      total number of entries in the
      central directory on this disk  2 bytes
      total number of entries in
      the central directory           2 bytes
      size of the central directory   4 bytes
      offset of start of central
      directory with respect to
      the starting disk number        4 bytes
      .ZIP file comment length        2 bytes
      .ZIP file comment       (variable size)
*/

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct END_OF_CENTRAL_DIRECTORY_RECORD {
    pub disk_number: u16,
    pub disk_with_central_directory: u16,
    pub number_of_files_on_this_disk: u16,
    pub number_of_files: u16,
    pub central_directory_size: u32,
    pub central_directory_offset: u32,
    pub zip_file_comment: Vec<u8>,
}
impl END_OF_CENTRAL_DIRECTORY_RECORD {
    pub fn size() -> u64 {
        22 // 4 + 2 + 2 + 2 + 2 + 4 + 4 + 2 + .?.
    }
    pub fn signature() -> u32 {
        0x06054b50
    }

    pub fn name() -> &'static str {
        "[End of central directory record]"
    }

    fn find<T>(reader: &mut T) -> ZipResult<u64>
    where
        T: Read + Seek,
    {
        let header_size = Self::size() as i64;
        let bytes_between_magic_and_comment_size = header_size - (4 + 2);
        let file_length = reader.seek(io::SeekFrom::End(0))? as i64;
        let upper_bound = ::std::cmp::max(0, file_length - header_size - ::std::u16::MAX as i64);

        let mut pos = file_length - header_size;
        while pos >= upper_bound {
            reader.seek(io::SeekFrom::Start(pos as u64))?;
            if Self::signature() == reader.read_u32::<LittleEndian>()? {
                reader.seek(io::SeekFrom::Current(bytes_between_magic_and_comment_size))?;
                let comment_length = reader.read_u16::<LittleEndian>()? as i64;
                if file_length - pos - header_size == comment_length {
                    return Ok(pos as u64);
                }
            }
            pos -= 1;
        }
        Err(ZipError::SectionNotFound(Self::name()))
    }

    pub fn load<T>(reader: &mut T) -> ZipResult<(Self, u64)>
    where
        T: Read + Seek,
    {
        let pos = Self::find(reader)?;
        reader.seek(io::SeekFrom::Start(pos))?;
        if Self::signature() != reader.read_u32::<LittleEndian>()? {
            return Err(ZipError::InvalidArchive("Invalid digital signature header"));
        }
        let mut record = END_OF_CENTRAL_DIRECTORY_RECORD {
            disk_number: reader.read_u16::<LittleEndian>()?,
            disk_with_central_directory: reader.read_u16::<LittleEndian>()?,
            number_of_files_on_this_disk: reader.read_u16::<LittleEndian>()?,
            number_of_files: reader.read_u16::<LittleEndian>()?,
            central_directory_size: reader.read_u32::<LittleEndian>()?,
            central_directory_offset: reader.read_u32::<LittleEndian>()?,
            zip_file_comment: Vec::new(),
        };
        let length = reader.read_u16::<LittleEndian>()?;
        record.zip_file_comment = ReadPodExt::read_exact(reader, length as usize)?;
        return Ok((record, pos));
    }
}
