use spec;
use crc32::Crc32Reader;
use types::ZipFile;
use types;
use std::io;
use std::io::{IoResult, IoError};
use std::cell::RefCell;
use flate2::FlateReader;

/// Wrapper for reading the contents of a ZIP file.
///
/// ```
/// fn doit() -> std::io::IoResult<()>
/// {
///     // For demonstration purposes we read from an empty buffer.
///     // Normally a File object would be used.
///     let buf = [0u8, 128];
///     let mut reader = std::io::BufReader::new(&buf);
///
///     let zip = try!(zip::ZipReader::new(reader));
///
///     for file in zip.files()
///     {
///         println!("Filename: {}", file.file_name_string());
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
}

fn unsupported_zip_error<T>(detail: &str) -> IoResult<T>
{
    Err(IoError
        {
            kind: io::OtherIoError,
            desc: "This ZIP file is not supported",
            detail: Some(detail.to_string()),
        })
}

impl<T: Reader+Seek> ZipReader<T>
{
    /// Opens a ZIP file and parses the content headers.
    pub fn new(mut reader: T) -> IoResult<ZipReader<T>>
    {
        let footer = try!(spec::CentralDirectoryEnd::find_and_parse(&mut reader));

        if footer.disk_number != footer.disk_with_central_directory { return unsupported_zip_error("Support for multi-disk files is not implemented") }

        let directory_start = footer.central_directory_offset as i64;
        let number_of_files = footer.number_of_files_on_this_disk as uint;

        let mut files = Vec::with_capacity(number_of_files);

        try!(reader.seek(directory_start, io::SeekSet));
        for i in range(0, number_of_files)
        {
            files.push(try!(spec::central_header_to_zip_file(&mut reader)));
        }

        Ok(ZipReader { inner: RefCell::new(reader), files: files })
    }

    /// An iterator over the information of all contained files.
    pub fn files(&self) -> ::std::slice::Items<ZipFile>
    {
        self.files.as_slice().iter()
    }

    /// Gets a reader for a contained zipfile.
    ///
    /// Possible errors:
    ///
    /// * `ResourceUnavailable`: when another reader returned from this function is still active
    /// * `OtherIoError`: if the file is encrypted or has an unsupported compression
    pub fn read_file(&self, file: &ZipFile) -> IoResult<Box<Reader>>
    {
        let mut inner_reader = match self.inner.try_borrow_mut()
        {
            Some(reader) => reader,
            None => return Err(IoError
                               {
                                   kind: io::ResourceUnavailable,
                                   desc: "There is already a ZIP reader active",
                                   detail: None
                               }),
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
            types::Stored =>
            {
                box
                    Crc32Reader::new(
                        limit_reader,
                        file.crc32)
                    as Box<Reader>
            },
            types::Deflated =>
            {
                let deflate_reader = limit_reader.deflate_decode();
                box
                    Crc32Reader::new(
                        deflate_reader,
                        file.crc32)
                    as Box<Reader>
            },
            _ => return unsupported_zip_error("Compression method not supported"),
        };
        Ok(reader)
    }
}
