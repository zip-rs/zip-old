#![allow(missing_docs, dead_code)]
use crate::result::{ZipError, ZipResult};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io;
use std::io::prelude::*;

pub const LOCAL_FILE_HEADER_SIGNATURE: u32 = 0x04034b50;
pub const CENTRAL_DIRECTORY_HEADER_SIGNATURE: u32 = 0x02014b50;
const CENTRAL_DIRECTORY_END_SIGNATURE: u32 = 0x06054b50;
pub const ZIP64_CENTRAL_DIRECTORY_END_SIGNATURE: u32 = 0x06064b50;
const ZIP64_CENTRAL_DIRECTORY_END_LOCATOR_SIGNATURE: u32 = 0x07064b50;
pub const DATA_DESCRIPTOR_SIGNATURE: u32 = 0x08074b50;

pub const ZIP64_BYTES_THR: u64 = u32::MAX as u64;
pub const ZIP64_ENTRY_THR: usize = u16::MAX as usize;

#[derive(Clone, Debug)]
pub struct CentralDirectoryEnd {
    pub disk_number: u16,
    pub disk_with_central_directory: u16,
    pub number_of_files_on_this_disk: u16,
    pub number_of_files: u16,
    pub central_directory_size: u32,
    pub central_directory_offset: u32,
    pub zip_file_comment: Vec<u8>,
}

impl CentralDirectoryEnd {
    pub fn len(&self) -> usize {
        22 + self.zip_file_comment.len()
    }

    pub fn parse<T: Read>(reader: &mut T) -> ZipResult<CentralDirectoryEnd> {
        let magic = reader.read_u32::<LittleEndian>()?;
        if magic != CENTRAL_DIRECTORY_END_SIGNATURE {
            return Err(ZipError::InvalidArchive("Invalid digital signature header"));
        }
        let disk_number = reader.read_u16::<LittleEndian>()?;
        let disk_with_central_directory = reader.read_u16::<LittleEndian>()?;
        let number_of_files_on_this_disk = reader.read_u16::<LittleEndian>()?;
        let number_of_files = reader.read_u16::<LittleEndian>()?;
        let central_directory_size = reader.read_u32::<LittleEndian>()?;
        let central_directory_offset = reader.read_u32::<LittleEndian>()?;
        let zip_file_comment_length = reader.read_u16::<LittleEndian>()? as usize;
        let mut zip_file_comment = vec![0; zip_file_comment_length];
        reader.read_exact(&mut zip_file_comment)?;

        Ok(CentralDirectoryEnd {
            disk_number,
            disk_with_central_directory,
            number_of_files_on_this_disk,
            number_of_files,
            central_directory_size,
            central_directory_offset,
            zip_file_comment,
        })
    }

    pub fn find_and_parse<T: Read + io::Seek>(
        reader: &mut T,
    ) -> ZipResult<(CentralDirectoryEnd, u64)> {
        const HEADER_SIZE: u64 = 22;
        const BYTES_BETWEEN_MAGIC_AND_COMMENT_SIZE: u64 = HEADER_SIZE - 6;
        let file_length = reader.seek(io::SeekFrom::End(0))?;

        let search_upper_bound = file_length.saturating_sub(HEADER_SIZE + ::std::u16::MAX as u64);

        if file_length < HEADER_SIZE {
            return Err(ZipError::InvalidArchive("Invalid zip header"));
        }

        let mut pos = file_length - HEADER_SIZE;
        while pos >= search_upper_bound {
            reader.seek(io::SeekFrom::Start(pos as u64))?;
            if reader.read_u32::<LittleEndian>()? == CENTRAL_DIRECTORY_END_SIGNATURE {
                reader.seek(io::SeekFrom::Current(
                    BYTES_BETWEEN_MAGIC_AND_COMMENT_SIZE as i64,
                ))?;
                let cde_start_pos = reader.seek(io::SeekFrom::Start(pos as u64))?;
                return CentralDirectoryEnd::parse(reader).map(|cde| (cde, cde_start_pos));
            }
            pos = match pos.checked_sub(1) {
                Some(p) => p,
                None => break,
            };
        }
        Err(ZipError::InvalidArchive(
            "Could not find central directory end",
        ))
    }

    pub fn write<T: Write>(&self, writer: &mut T) -> ZipResult<()> {
        writer.write_u32::<LittleEndian>(CENTRAL_DIRECTORY_END_SIGNATURE)?;
        writer.write_u16::<LittleEndian>(self.disk_number)?;
        writer.write_u16::<LittleEndian>(self.disk_with_central_directory)?;
        writer.write_u16::<LittleEndian>(self.number_of_files_on_this_disk)?;
        writer.write_u16::<LittleEndian>(self.number_of_files)?;
        writer.write_u32::<LittleEndian>(self.central_directory_size)?;
        writer.write_u32::<LittleEndian>(self.central_directory_offset)?;
        writer.write_u16::<LittleEndian>(self.zip_file_comment.len() as u16)?;
        writer.write_all(&self.zip_file_comment)?;
        Ok(())
    }
}

pub struct Zip64CentralDirectoryEndLocator {
    pub disk_with_central_directory: u32,
    pub end_of_central_directory_offset: u64,
    pub number_of_disks: u32,
}

impl Zip64CentralDirectoryEndLocator {
    pub fn parse<T: Read>(reader: &mut T) -> ZipResult<Zip64CentralDirectoryEndLocator> {
        let magic = reader.read_u32::<LittleEndian>()?;
        if magic != ZIP64_CENTRAL_DIRECTORY_END_LOCATOR_SIGNATURE {
            return Err(ZipError::InvalidArchive(
                "Invalid zip64 locator digital signature header",
            ));
        }
        let disk_with_central_directory = reader.read_u32::<LittleEndian>()?;
        let end_of_central_directory_offset = reader.read_u64::<LittleEndian>()?;
        let number_of_disks = reader.read_u32::<LittleEndian>()?;

        Ok(Zip64CentralDirectoryEndLocator {
            disk_with_central_directory,
            end_of_central_directory_offset,
            number_of_disks,
        })
    }

    pub fn write<T: Write>(&self, writer: &mut T) -> ZipResult<()> {
        writer.write_u32::<LittleEndian>(ZIP64_CENTRAL_DIRECTORY_END_LOCATOR_SIGNATURE)?;
        writer.write_u32::<LittleEndian>(self.disk_with_central_directory)?;
        writer.write_u64::<LittleEndian>(self.end_of_central_directory_offset)?;
        writer.write_u32::<LittleEndian>(self.number_of_disks)?;
        Ok(())
    }
}

pub struct Zip64CentralDirectoryEnd {
    pub version_made_by: u16,
    pub version_needed_to_extract: u16,
    pub disk_number: u32,
    pub disk_with_central_directory: u32,
    pub number_of_files_on_this_disk: u64,
    pub number_of_files: u64,
    pub central_directory_size: u64,
    pub central_directory_offset: u64,
    //pub extensible_data_sector: Vec<u8>, <-- We don't do anything with this at the moment.
}

impl Zip64CentralDirectoryEnd {
    pub fn find_and_parse<T: Read + io::Seek>(
        reader: &mut T,
        nominal_offset: u64,
        search_upper_bound: u64,
    ) -> ZipResult<(Zip64CentralDirectoryEnd, u64)> {
        let mut pos = nominal_offset;

        while pos <= search_upper_bound {
            reader.seek(io::SeekFrom::Start(pos))?;

            if reader.read_u32::<LittleEndian>()? == ZIP64_CENTRAL_DIRECTORY_END_SIGNATURE {
                let archive_offset = pos - nominal_offset;

                let _record_size = reader.read_u64::<LittleEndian>()?;
                // We would use this value if we did anything with the "zip64 extensible data sector".

                let version_made_by = reader.read_u16::<LittleEndian>()?;
                let version_needed_to_extract = reader.read_u16::<LittleEndian>()?;
                let disk_number = reader.read_u32::<LittleEndian>()?;
                let disk_with_central_directory = reader.read_u32::<LittleEndian>()?;
                let number_of_files_on_this_disk = reader.read_u64::<LittleEndian>()?;
                let number_of_files = reader.read_u64::<LittleEndian>()?;
                let central_directory_size = reader.read_u64::<LittleEndian>()?;
                let central_directory_offset = reader.read_u64::<LittleEndian>()?;

                return Ok((
                    Zip64CentralDirectoryEnd {
                        version_made_by,
                        version_needed_to_extract,
                        disk_number,
                        disk_with_central_directory,
                        number_of_files_on_this_disk,
                        number_of_files,
                        central_directory_size,
                        central_directory_offset,
                    },
                    archive_offset,
                ));
            }

            pos += 1;
        }

        Err(ZipError::InvalidArchive(
            "Could not find ZIP64 central directory end",
        ))
    }

    pub fn write<T: Write>(&self, writer: &mut T) -> ZipResult<()> {
        writer.write_u32::<LittleEndian>(ZIP64_CENTRAL_DIRECTORY_END_SIGNATURE)?;
        writer.write_u64::<LittleEndian>(44)?; // record size
        writer.write_u16::<LittleEndian>(self.version_made_by)?;
        writer.write_u16::<LittleEndian>(self.version_needed_to_extract)?;
        writer.write_u32::<LittleEndian>(self.disk_number)?;
        writer.write_u32::<LittleEndian>(self.disk_with_central_directory)?;
        writer.write_u64::<LittleEndian>(self.number_of_files_on_this_disk)?;
        writer.write_u64::<LittleEndian>(self.number_of_files)?;
        writer.write_u64::<LittleEndian>(self.central_directory_size)?;
        writer.write_u64::<LittleEndian>(self.central_directory_offset)?;
        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
pub struct GeneralPurposeBitFlags(pub u16);

impl GeneralPurposeBitFlags {
    #[inline]
    pub fn encrypted(&self) -> bool {
        self.0 & 1 == 1
    }

    #[inline]
    pub fn is_utf8(&self) -> bool {
        self.0 & (1 << 11) != 0
    }

    #[inline]
    pub fn using_data_descriptor(&self) -> bool {
        self.0 & (1 << 3) != 0
    }

    #[inline]
    pub fn set_using_data_descriptor(&mut self, b: bool) {
        self.0 &= !(1 << 3);
        if b {
            self.0 |= 1 << 3;
        }
    }
}

#[derive(Clone, Debug)]
pub struct CentralDirectoryHeader {
    pub version_made_by: u16,
    pub version_to_extract: u16,
    pub flags: GeneralPurposeBitFlags,
    pub compression_method: u16,
    pub last_mod_time: u16,
    pub last_mod_date: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub disk_number: u16,
    pub internal_file_attributes: u16,
    pub external_file_attributes: u32,
    pub offset: u32,
    pub file_name_raw: Vec<u8>,
    pub extra_field: Vec<u8>,
    pub file_comment_raw: Vec<u8>,
}

impl CentralDirectoryHeader {
    pub fn len(&self) -> usize {
        46 + self.file_name_raw.len() + self.extra_field.len() + self.file_comment_raw.len()
    }
    pub fn parse<R: Read>(reader: &mut R) -> ZipResult<CentralDirectoryHeader> {
        let signature = reader.read_u32::<LittleEndian>()?;
        if signature != CENTRAL_DIRECTORY_HEADER_SIGNATURE {
            return Err(ZipError::InvalidArchive("Invalid Central Directory header"));
        }

        let version_made_by = reader.read_u16::<LittleEndian>()?;
        let version_to_extract = reader.read_u16::<LittleEndian>()?;
        let flags = reader.read_u16::<LittleEndian>()?;
        let compression_method = reader.read_u16::<LittleEndian>()?;
        let last_mod_time = reader.read_u16::<LittleEndian>()?;
        let last_mod_date = reader.read_u16::<LittleEndian>()?;
        let crc32 = reader.read_u32::<LittleEndian>()?;
        let compressed_size = reader.read_u32::<LittleEndian>()?;
        let uncompressed_size = reader.read_u32::<LittleEndian>()?;
        let file_name_length = reader.read_u16::<LittleEndian>()?;
        let extra_field_length = reader.read_u16::<LittleEndian>()?;
        let file_comment_length = reader.read_u16::<LittleEndian>()?;
        let disk_number = reader.read_u16::<LittleEndian>()?;
        let internal_file_attributes = reader.read_u16::<LittleEndian>()?;
        let external_file_attributes = reader.read_u32::<LittleEndian>()?;
        let offset = reader.read_u32::<LittleEndian>()?;
        let mut file_name_raw = vec![0; file_name_length as usize];
        reader.read_exact(&mut file_name_raw)?;
        let mut extra_field = vec![0; extra_field_length as usize];
        reader.read_exact(&mut extra_field)?;
        let mut file_comment_raw = vec![0; file_comment_length as usize];
        reader.read_exact(&mut file_comment_raw)?;

        Ok(CentralDirectoryHeader {
            version_made_by,
            version_to_extract,
            flags: GeneralPurposeBitFlags(flags),
            compression_method,
            last_mod_time,
            last_mod_date,
            crc32,
            compressed_size,
            uncompressed_size,
            disk_number,
            internal_file_attributes,
            external_file_attributes,
            offset,
            file_name_raw,
            extra_field,
            file_comment_raw,
        })
    }

    pub fn write<T: Write>(&self, writer: &mut T) -> ZipResult<()> {
        writer.write_u32::<LittleEndian>(CENTRAL_DIRECTORY_HEADER_SIGNATURE)?;
        writer.write_u16::<LittleEndian>(self.version_made_by)?;
        writer.write_u16::<LittleEndian>(self.version_to_extract)?;
        writer.write_u16::<LittleEndian>(self.flags.0)?;
        writer.write_u16::<LittleEndian>(self.compression_method)?;
        writer.write_u16::<LittleEndian>(self.last_mod_time)?;
        writer.write_u16::<LittleEndian>(self.last_mod_date)?;
        writer.write_u32::<LittleEndian>(self.crc32)?;
        writer.write_u32::<LittleEndian>(self.compressed_size)?;
        writer.write_u32::<LittleEndian>(self.uncompressed_size)?;
        writer.write_u16::<LittleEndian>(self.file_name_raw.len() as u16)?;
        writer.write_u16::<LittleEndian>(self.extra_field.len() as u16)?;
        writer.write_u16::<LittleEndian>(self.file_comment_raw.len() as u16)?;
        writer.write_u16::<LittleEndian>(self.disk_number)?;
        writer.write_u16::<LittleEndian>(self.internal_file_attributes)?;
        writer.write_u32::<LittleEndian>(self.external_file_attributes)?;
        writer.write_u32::<LittleEndian>(self.offset)?;
        writer.write_all(&self.file_name_raw)?;
        writer.write_all(&self.extra_field)?;
        writer.write_all(&self.file_comment_raw)?;
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct LocalFileHeader {
    pub version_to_extract: u16,
    pub flags: GeneralPurposeBitFlags,
    pub compression_method: u16,
    pub last_mod_time: u16,
    pub last_mod_date: u16,
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
    pub file_name_raw: Vec<u8>,
    pub extra_field: Vec<u8>,
}

impl LocalFileHeader {
    pub fn len(&self) -> usize {
        30 + self.file_name_raw.len() + self.extra_field.len()
    }

    pub fn parse<R: Read>(reader: &mut R) -> ZipResult<LocalFileHeader> {
        let signature = reader.read_u32::<LittleEndian>()?;
        if signature != LOCAL_FILE_HEADER_SIGNATURE {
            return Err(ZipError::InvalidArchive("Invalid local file header"));
        }

        let version_to_extract = reader.read_u16::<LittleEndian>()?;
        let flags = reader.read_u16::<LittleEndian>()?;
        let compression_method = reader.read_u16::<LittleEndian>()?;
        let last_mod_time = reader.read_u16::<LittleEndian>()?;
        let last_mod_date = reader.read_u16::<LittleEndian>()?;
        let crc32 = reader.read_u32::<LittleEndian>()?;
        let compressed_size = reader.read_u32::<LittleEndian>()?;
        let uncompressed_size = reader.read_u32::<LittleEndian>()?;
        let file_name_length = reader.read_u16::<LittleEndian>()?;
        let extra_field_length = reader.read_u16::<LittleEndian>()?;

        let mut file_name_raw = vec![0; file_name_length as usize];
        reader.read_exact(&mut file_name_raw)?;
        let mut extra_field = vec![0; extra_field_length as usize];
        reader.read_exact(&mut extra_field)?;

        Ok(LocalFileHeader {
            version_to_extract,
            flags: GeneralPurposeBitFlags(flags),
            compression_method,
            last_mod_time,
            last_mod_date,
            crc32,
            compressed_size,
            uncompressed_size,
            file_name_raw,
            extra_field,
        })
    }

    pub fn write<T: Write>(&self, writer: &mut T) -> ZipResult<()> {
        writer.write_u32::<LittleEndian>(LOCAL_FILE_HEADER_SIGNATURE)?;
        writer.write_u16::<LittleEndian>(self.version_to_extract)?;
        writer.write_u16::<LittleEndian>(self.flags.0)?;
        writer.write_u16::<LittleEndian>(self.compression_method)?;
        writer.write_u16::<LittleEndian>(self.last_mod_time)?;
        writer.write_u16::<LittleEndian>(self.last_mod_date)?;
        writer.write_u32::<LittleEndian>(self.crc32)?;
        writer.write_u32::<LittleEndian>(self.compressed_size)?;
        writer.write_u32::<LittleEndian>(self.uncompressed_size)?;
        writer.write_u16::<LittleEndian>(self.file_name_raw.len() as u16)?;
        writer.write_u16::<LittleEndian>(self.extra_field.len() as u16)?;
        writer.write_all(&self.file_name_raw)?;
        writer.write_all(&self.extra_field)?;
        Ok(())
    }
}

#[derive(Copy, Clone, Debug)]
pub struct DataDescriptor {
    pub crc32: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
}

impl DataDescriptor {
    pub fn read<T: Read>(reader: &mut T) -> ZipResult<DataDescriptor> {
        let first_word = reader.read_u32::<LittleEndian>()?;
        let crc32 = if first_word == DATA_DESCRIPTOR_SIGNATURE {
            reader.read_u32::<LittleEndian>()?
        } else {
            first_word
        };
        let compressed_size = reader.read_u32::<LittleEndian>()?;
        let uncompressed_size = reader.read_u32::<LittleEndian>()?;
        Ok(DataDescriptor {
            crc32,
            compressed_size,
            uncompressed_size,
        })
    }
}
