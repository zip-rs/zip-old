use crc32::Crc32Reader;
use types::ZipFile;
use compression::CompressionMethod;
use spec;
use reader_spec;
use result::{ZipResult, ZipError};
use std::io;
use std::io::prelude::*;
use std::cell::{RefCell, BorrowState};
use std::collections::HashMap;
use flate2::FlateReadExt;
use bzip2::reader::BzDecompressor;

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
///     let zip = try!(zip::ZipReader::new(reader));
///
///     for file in zip.files()
///     {
///         println!("Filename: {}", file.file_name);
///         let mut file_reader = try!(zip.read_file(file));
///         let first_byte = try!(file_reader.bytes().next().unwrap());
///         println!("{}", first_byte);
///     }
///     Ok(())
/// }
///
/// println!("Result: {:?}", doit());
/// ```
pub struct ZipReader<T>
{
    inner: RefCell<T>,
    files: Vec<ZipFile>,
    names_map: HashMap<String, usize>,
}

fn unsupported_zip_error<T>(detail: &'static str) -> ZipResult<T>
{
    Err(ZipError::UnsupportedZipFile(detail))
}

impl<T: Read+io::Seek> ZipReader<T>
{
    /// Opens a ZIP file and parses the content headers.
    pub fn new(mut reader: T) -> ZipResult<ZipReader<T>>
    {
        let footer = try!(spec::CentralDirectoryEnd::find_and_parse(&mut reader));

        if footer.disk_number != footer.disk_with_central_directory { return unsupported_zip_error("Support for multi-disk files is not implemented") }

        let directory_start = footer.central_directory_offset as u64;
        let number_of_files = footer.number_of_files_on_this_disk as usize;

        let mut files = Vec::with_capacity(number_of_files);
        let mut names_map = HashMap::new();

        try!(reader.seek(io::SeekFrom::Start(directory_start)));
        for _ in (0 .. number_of_files)
        {
            let file = try!(reader_spec::central_header_to_zip_file(&mut reader));
            names_map.insert(file.file_name.clone(), files.len());
            files.push(file);
        }

        Ok(ZipReader { inner: RefCell::new(reader), files: files, names_map: names_map })
    }

    /// An iterator over the information of all contained files.
    pub fn files(&self) -> ::std::slice::Iter<ZipFile>
    {
        (&*self.files).iter()
    }

    /// Search for a file entry by name
    pub fn get(&self, name: &str) -> Option<&ZipFile>
    {
        self.names_map.get(name).map(|index| &self.files[*index])
    }

    /// Gets a reader for a contained zipfile.
    ///
    /// May return `ReaderUnavailable` if there is another reader borrowed.
    pub fn read_file<'a>(&'a self, file: &ZipFile) -> ZipResult<Box<Read+'a>>
    {
        let mut inner_reader = match self.inner.borrow_state()
        {
            BorrowState::Unused => self.inner.borrow_mut(),
            _ => return Err(ZipError::ReaderUnavailable),
        };
        let pos = file.data_start as u64;

        if file.encrypted
        {
            return unsupported_zip_error("Encrypted files are not supported")
        }

        try!(inner_reader.seek(io::SeekFrom::Start(pos)));
        let refmut_reader = ::util::RefMutReader::new(inner_reader);
        let limit_reader = refmut_reader.take(file.compressed_size as u64);

        let reader = match file.compression_method
        {
            CompressionMethod::Stored =>
            {
                Box::new(
                    Crc32Reader::new(
                        limit_reader,
                        file.crc32))
                    as Box<Read>
            },
            CompressionMethod::Deflated =>
            {
                let deflate_reader = limit_reader.deflate_decode();
                Box::new(
                    Crc32Reader::new(
                        deflate_reader,
                        file.crc32))
                    as Box<Read>
            },
            CompressionMethod::Bzip2 =>
            {
                let bzip2_reader = BzDecompressor::new(limit_reader);
                Box::new(
                    Crc32Reader::new(
                        bzip2_reader,
                        file.crc32))
                    as Box<Read>
            },
            _ => return unsupported_zip_error("Compression method not supported"),
        };
        Ok(reader)
    }

    /// Unwrap and return the inner reader object
    ///
    /// The position of the reader is undefined.
    pub fn into_inner(self) -> T
    {
        self.inner.into_inner()
    }

    /// Deprecated method equal to `into_inner()`
    #[deprecated="renamed to into_inner()"]
    pub fn unwrap(self) -> T
    {
        self.into_inner()
    }
}
