use std::io::{Read, Seek, SeekFrom};
use std::collections::HashMap;
use compression::CompressionMethod;
use zip_file_reader::{ZipFileReader, Crc32Reader, BzDecoder};
use msdos_time::{MsDosDateTime, TmMsDosExt};
use zip_file_data::ZipFileData;
use flate2::FlateReadExt;
use cp437::FromCp437;

use result::{ZipError, ZipResult};
use system::System;
use spec::{LOCAL_FILE_HEADER, CENTRAL_DIRECTORY_HEADER};
use zip_file::ZipFile;
use central_directory::CentralDirectory;

fn unsupported_zip_error<T>(detail: &'static str) -> ZipResult<T> {
    Err(ZipError::UnsupportedArchive(detail))
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
impl<R: Read + Seek> ZipArchive<R> {
    /// Opens a Zip archive and parses the central directory
    pub fn new(mut reader: R) -> ZipResult<ZipArchive<R>> {
        let directory = CentralDirectory::load(&mut reader)?;
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
        LOCAL_FILE_HEADER::load(&mut self.reader)?;
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
        Ok(ZipFile::new(data, reader))
    }

    /// Unwrap and return the inner reader object
    ///
    /// The position of the reader is undefined.
    pub fn into_inner(self) -> R {
        self.reader
    }
}

fn central_header_to_zip_file<R: Read + Seek>(reader: &mut R) -> ZipResult<ZipFileData> {
    let header = CENTRAL_DIRECTORY_HEADER::load(reader)?;

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
        compressed_size: header.compressed_size,
        uncompressed_size: header.uncompressed_size,
        file_name: file_name,
        file_name_raw: header.file_name,
        file_comment: file_comment,
        header_start: header.local_header_offset,
        external_attributes: header.external_file_attrs,
    };

    Ok(result)
}