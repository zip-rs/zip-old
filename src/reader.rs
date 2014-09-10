use spec;
use crc32::Crc32Reader;
use types::ZipFile;
use types;
use std::io;
use std::io::{IoResult, IoError};
use std::cell::RefCell;
use flate2::FlateReader;

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
    pub fn new(mut reader: T) -> IoResult<ZipReader<T>>
    {
        let footer = try!(spec::CentralDirectoryEnd::find_and_parse(&mut reader));

        if footer.number_of_disks > 1 { return unsupported_zip_error("Support for multi-disk files is not implemented") }

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

    pub fn files(&self) -> ::std::slice::Items<ZipFile>
    {
        self.files.as_slice().iter()
    }

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
