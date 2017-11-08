//! Structs for reading a ZIP archive

use crc32::Crc32Reader;
use compression::CompressionMethod;
use spec;
use result::{ZipError, ZipResult};
use std::io::{Take, SeekFrom, Result};
use std::io::prelude::*;
use std::collections::HashMap;
use flate2;
use flate2::FlateReadExt;
use types::{System, ZipFileData};
use cp437::FromCp437;
use msdos_time::{MsDosDateTime, TmMsDosExt};

#[cfg(feature = "bzip2")]
use bzip2::read::BzDecoder;

mod ffi {
    pub const S_IFDIR: u32 = 0o0040000;
    pub const S_IFREG: u32 = 0o0100000;
}

/// Wrapper for reading the contents of a ZIP file.
///
/// ```
/// fn doit() -> zip::result::ZipResult<()>
/// {
///     use std::io::prelude::*;
///
///     // For demonstration purposes we read from an empty buffer.
///     // Normally a File object would be used.
///     let buf: &[u8] = &[0u8; 128];
///     let mut reader = std::io::Cursor::new(buf);
///
///     let mut zip = try!(zip::ZipArchive::new(reader));
///
///     for i in 0..zip.len()
///     {
///         let mut file = zip.by_index(i).unwrap();
///         println!("Filename: {}", file.name());
///         let first_byte = try!(file.bytes().next().unwrap());
///         println!("{}", first_byte);
///     }
///     Ok(())
/// }
///
/// println!("Result: {:?}", doit());
/// ```
#[derive(Debug)]
pub struct ZipArchive<R: Read + Seek> {
    reader: R,
    files: Vec<ZipFileData>,
    names_map: HashMap<String, usize>,
}

enum ZipFileReader<'a> {
    Stored(Crc32Reader<Take<&'a mut Read>>),
    Deflated(Crc32Reader<flate2::read::DeflateDecoder<Take<&'a mut Read>>>),
    #[cfg(feature = "bzip2")]
    Bzip2(Crc32Reader<BzDecoder<Take<&'a mut Read>>>),
}

/// A struct for reading a zip file
pub struct ZipFile<'a> {
    data: &'a ZipFileData,
    reader: ZipFileReader<'a>,
}

fn unsupported_zip_error<T>(detail: &'static str) -> ZipResult<T> {
    Err(ZipError::UnsupportedArchive(detail))
}
struct CentralDirectory {
    pub disk_number: u32,
    pub disk_with_central_directory: u32,
    pub number_of_files_on_this_disk: u64,
    pub number_of_files: u64,
    pub size: u64,
    pub offset: u64,
}
impl CentralDirectory {
    pub fn new() -> Self {
        CentralDirectory {
            disk_number: 0,
            disk_with_central_directory: 0,
            number_of_files_on_this_disk: 0,
            number_of_files: 0,
            size: 0,
            offset: 0,
        }
    }
}

impl<R: Read + Seek> ZipArchive<R> {
    /// Opens a Zip archive and parses the central directory
    pub fn new(mut reader: R) -> ZipResult<ZipArchive<R>> {
        let directory = Self::get_central_directory(&mut reader)?;
        if directory.disk_number != directory.disk_with_central_directory {
            return unsupported_zip_error("Support for multi-disk files is not implemented");
        }

        let mut files = Vec::with_capacity(directory.number_of_files_on_this_disk as usize);
        let mut names_map = HashMap::new();

        reader.seek(SeekFrom::Start(directory.offset))?;
        for idx in 0..directory.number_of_files_on_this_disk as usize {
            let file = central_header_to_zip_file(&mut reader)?;
            names_map.insert(file.file_name.clone(), idx);
            files.push(file);
        }

        Ok(ZipArchive {
            reader: reader,
            files: files,
            names_map: names_map,
        })
    }

    fn get_central_directory<T: Read + Seek>(reader: &mut T) -> ZipResult<CentralDirectory> {
        let mut directory = CentralDirectory::new();
        let (zip32, pos) = spec::END_OF_CENTRAL_DIRECTORY_RECORD::load(reader)?;
        if let Some(loc) = spec::ZIP64_END_OF_CENTRAL_DIRECTORY_LOCATOR::load(reader, &pos).ok() {
            let pos = loc.central_directory_offset;
            let zip64 = spec::ZIP64_END_OF_CENTRAL_DIRECTORY_RECORD::load(reader, &pos)?;
            directory.disk_number = zip64.disk_number;
            directory.disk_with_central_directory = zip64.disk_with_central_directory;
            directory.number_of_files_on_this_disk = zip64.number_of_files_on_this_disk;
            directory.number_of_files = zip64.number_of_files;
            directory.size = zip64.central_directory_size;
            directory.offset = zip64.central_directory_offset;
        } else {
            directory.disk_number = zip32.disk_number as u32;
            directory.disk_with_central_directory = zip32.disk_with_central_directory as u32;
            directory.number_of_files_on_this_disk = zip32.number_of_files_on_this_disk as u64;
            directory.number_of_files = zip32.number_of_files as u64;
            directory.size = zip32.central_directory_size as u64;
            directory.offset = zip32.central_directory_offset as u64;
        }
        Ok(directory)
    }

    /// Number of files contained in this zip.
    ///
    /// ```
    /// fn iter() {
    ///     let mut zip = zip::ZipArchive::new(std::io::Cursor::new(vec![])).unwrap();
    ///
    ///     for i in 0..zip.len() {
    ///         let mut file = zip.by_index(i).unwrap();
    ///         // Do something with file i
    ///     }
    /// }
    /// ```
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Search for a file entry by name
    pub fn by_name<'a>(&'a mut self, name: &str) -> ZipResult<ZipFile<'a>> {
        match self.names_map.get(name) {
            Some(&index) => self.by_index(index),
            None => Err(ZipError::FileNotFound),
        }
    }

    /// Get a contained file by index
    pub fn by_index<'a>(&'a mut self, file_index: usize) -> ZipResult<ZipFile<'a>> {
        if file_index >= self.files.len() {
            return Err(ZipError::FileNotFound);
        }

        let ref data = self.files[file_index];
        if data.encrypted {
            return unsupported_zip_error("Encrypted files are not supported");
        }

        self.reader.seek(SeekFrom::Start(data.header_start))?;
        spec::LOCAL_FILE_HEADER::load(&mut self.reader)?;
        let limit_reader = (self.reader.by_ref() as &mut Read).take(data.compressed_size);

        let reader = match data.compression_method {
            CompressionMethod::Stored => {
                ZipFileReader::Stored(Crc32Reader::new(limit_reader, data.crc32))
            }
            CompressionMethod::Deflated => {
                let deflate_reader = limit_reader.deflate_decode();
                ZipFileReader::Deflated(Crc32Reader::new(deflate_reader, data.crc32))
            }
            #[cfg(feature = "bzip2")]
            CompressionMethod::Bzip2 => {
                let bzip2_reader = BzDecoder::new(limit_reader);
                ZipFileReader::Bzip2(Crc32Reader::new(bzip2_reader, data.crc32))
            }
            _ => return unsupported_zip_error("Compression method not supported"),
        };
        Ok(ZipFile {
            reader: reader,
            data: data,
        })
    }

    /// Unwrap and return the inner reader object
    ///
    /// The position of the reader is undefined.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

fn central_header_to_zip_file<R: Read + Seek>(reader: &mut R) -> ZipResult<ZipFileData> {
    let header = spec::CENTRAL_DIRECTORY_HEADER::load(reader)?;

    let is_utf8 = header.general_purpose_flag & (1 << 11) != 0;
    let file_name = match is_utf8 {
        true => String::from_utf8_lossy(&*header.file_name).into_owned(),
        false => header.file_name.clone().from_cp437(),
    };
    let file_comment = match is_utf8 {
        true => String::from_utf8_lossy(&*header.file_comment).into_owned(),
        false => header.file_comment.clone().from_cp437(),
    };

    let time = ::time::Tm::from_msdos(MsDosDateTime::new(
        header.last_mod_time,
        header.last_mod_date,
    ))?;
    // Construct the result
    let result = ZipFileData {
        system: System::from_u8((header.version_made_by >> 8) as u8),
        version_made_by: header.version_made_by as u8,
        encrypted: header.general_purpose_flag & 1 == 1,
        compression_method: CompressionMethod::from_u16(header.compression_method),
        last_modified_time: time,
        crc32: header.crc32,
        compressed_size: header.compressed_size as u64,
        uncompressed_size: header.uncompressed_size as u64,
        file_name: file_name,
        file_name_raw: header.file_name,
        file_comment: file_comment,
        header_start: header.local_header_offset,
        external_attributes: header.external_file_attrs,
    };

    Ok(result)
}

/// Methods for retreiving information on zip files
impl<'a> ZipFile<'a> {
    fn get_reader(&mut self) -> &mut Read {
        match self.reader {
            ZipFileReader::Stored(ref mut r) => r as &mut Read,
            ZipFileReader::Deflated(ref mut r) => r as &mut Read,
            #[cfg(feature = "bzip2")]
            ZipFileReader::Bzip2(ref mut r) => r as &mut Read,
        }
    }
    /// Get the version of the file
    pub fn version_made_by(&self) -> (u8, u8) {
        (
            self.data.version_made_by / 10,
            self.data.version_made_by % 10,
        )
    }
    /// Get the name of the file
    pub fn name(&self) -> &str {
        &*self.data.file_name
    }
    /// Get the name of the file, in the raw (internal) byte representation.
    pub fn name_raw(&self) -> &[u8] {
        &*self.data.file_name_raw
    }
    /// Get the comment of the file
    pub fn comment(&self) -> &str {
        &*self.data.file_comment
    }
    /// Get the compression method used to store the file
    pub fn compression(&self) -> CompressionMethod {
        self.data.compression_method
    }
    /// Get the size of the file in the archive
    pub fn compressed_size(&self) -> u64 {
        self.data.compressed_size
    }
    /// Get the size of the file when uncompressed
    pub fn size(&self) -> u64 {
        self.data.uncompressed_size
    }
    /// Get the time the file was last modified
    pub fn last_modified(&self) -> ::time::Tm {
        self.data.last_modified_time
    }
    /// Get unix mode for the file
    pub fn unix_mode(&self) -> Option<u32> {
        match self.data.system {
            System::Unix => Some(self.data.external_attributes >> 16),
            System::Dos => {
                // Interpret MSDOS directory bit
                let mut mode = if 0x10 == (self.data.external_attributes & 0x10) {
                    ffi::S_IFDIR | 0o0775
                } else {
                    ffi::S_IFREG | 0o0664
                };
                if 0x01 == (self.data.external_attributes & 0x01) {
                    // Read-only bit; strip write permissions
                    mode &= 0o0555;
                }
                Some(mode)
            }
            _ => None,
        }
    }
    /// Get the CRC32 hash of the original file
    pub fn crc32(&self) -> u32 {
        self.data.crc32
    }
    /// Get the offset of the payload of the original file
    pub fn offset(&self) -> u64 {
        self.data.header_start
    }
}

impl<'a> Read for ZipFile<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.get_reader().read(buf)
    }
}
