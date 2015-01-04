use crc32::Crc32Reader;
use types::ZipFile;
use compression::CompressionMethod;
use spec;
use reader_spec;
use result::{ZipResult, ZipError};
use std::io;
use std::cell::RefCell;
use std::collections::HashMap;
use flate2::FlateReader;
use bzip2::reader::BzDecompressor;

/// Wrapper for reading the contents of a ZIP file.
///
/// ```
/// fn doit() -> zip::result::ZipResult<()>
/// {
///     // For demonstration purposes we read from an empty buffer.
///     // Normally a File object would be used.
///     let buf = [0u8; 128];
///     let mut reader = std::io::BufReader::new(&buf);
///
///     let zip = try!(zip::ZipReader::new(reader));
///
///     for file in zip.files()
///     {
///         println!("Filename: {}", file.file_name);
///         let mut file_reader = try!(zip.read_file(file));
///         let first_byte = try!(file_reader.read_byte());
///         println!("{}", first_byte);
///     }
///     Ok(())
/// }
///
/// println!("Result: {}", doit());
/// ```
pub struct ZipReader<T>
{
    inner: RefCell<T>,
    files: Vec<ZipFile>,
    names_map: HashMap<String, uint>,
}

fn unsupported_zip_error<T>(detail: &'static str) -> ZipResult<T>
{
    Err(ZipError::UnsupportedZipFile(detail))
}

impl<T: Reader+Seek> ZipReader<T>
{
    /// Opens a ZIP file and parses the content headers.
    pub fn new(mut reader: T) -> ZipResult<ZipReader<T>>
    {
        let footer = try!(spec::CentralDirectoryEnd::find_and_parse(&mut reader));

        if footer.disk_number != footer.disk_with_central_directory { return unsupported_zip_error("Support for multi-disk files is not implemented") }

        let directory_start = footer.central_directory_offset as i64;
        let number_of_files = footer.number_of_files_on_this_disk as uint;

        let mut files = Vec::with_capacity(number_of_files);
        let mut names_map = HashMap::new();

        try!(reader.seek(directory_start, io::SeekSet));
        for _ in range(0, number_of_files)
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
        self.files.as_slice().iter()
    }

    /// Search for a file entry by name
    pub fn get(&self, name: &str) -> Option<&ZipFile>
    {
        self.names_map.get(name).map(|index| &self.files[*index])
    }

    /// Gets a reader for a contained zipfile.
    ///
    /// May return `ReaderUnavailable` if there is another reader borrowed.
    pub fn read_file(&self, file: &ZipFile) -> ZipResult<Box<Reader>>
    {
        let mut inner_reader = match self.inner.try_borrow_mut()
        {
            Some(reader) => reader,
            None => return Err(ZipError::ReaderUnavailable),
        };
        let pos = file.data_start as i64;

        if file.encrypted
        {
            return unsupported_zip_error("Encrypted files are not supported")
        }

        try!(inner_reader.seek(pos, io::SeekSet));
        let refmut_reader = ::util::RefMutReader::new(inner_reader);
        let limit_reader = io::util::LimitReader::new(refmut_reader, file.compressed_size as uint);

        let reader = match file.compression_method
        {
            CompressionMethod::Stored =>
            {
                box
                    Crc32Reader::new(
                        limit_reader,
                        file.crc32)
                    as Box<Reader>
            },
            CompressionMethod::Deflated =>
            {
                let deflate_reader = limit_reader.deflate_decode();
                box
                    Crc32Reader::new(
                        deflate_reader,
                        file.crc32)
                    as Box<Reader>
            },
            CompressionMethod::Bzip2 =>
            {
                let bzip2_reader = BzDecompressor::new(limit_reader);
                box
                    Crc32Reader::new(
                        bzip2_reader,
                        file.crc32)
                    as Box<Reader>
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
