use spec;
use crc32::Crc32Reader;
use types::ZipFile;
use types;
use std::io;
use std::io::{IoResult, IoError};
use std::cell::RefCell;
use flate2::FlateReader;

pub struct ZipContainer<T>
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

impl<T: Reader+Seek> ZipContainer<T>
{
    pub fn new(inner: T) -> IoResult<ZipContainer<T>>
    {
        let mut result = ZipContainer { inner: RefCell::new(inner), files: Vec::new() };

        {
            let reader = &mut *result.inner.borrow_mut();
            let footer = try!(spec::CentralDirectoryEnd::find_and_parse(reader));

            if footer.number_of_disks > 1 { return unsupported_zip_error("Support for multi-disk files is not implemented") }

            let directory_start = footer.central_directory_offset as i64;
            let number_of_files = footer.number_of_files_on_this_disk as uint;

            let mut files = Vec::with_capacity(number_of_files);

            try!(reader.seek(directory_start, io::SeekSet));
            for i in range(0, number_of_files)
            {
                files.push(try!(ZipContainer::parse_directory(reader)));
            }

            result.files = files;
        }

        Ok(result)
    }

    fn parse_directory(reader: &mut T) -> IoResult<ZipFile>
    {
        let cdh = try!(spec::CentralDirectoryHeader::parse(reader));
        // Remember position
        let pos = try!(reader.tell()) as i64;
        let result = ZipContainer::parse_file(reader, cdh);
        // Jump back for next directory
        try!(reader.seek(pos, io::SeekSet));
        result
    }

    fn parse_file(reader: &mut T, central: spec::CentralDirectoryHeader) -> IoResult<ZipFile>
    {
        try!(reader.seek(central.file_offset as i64, io::SeekSet));
        let local = try!(spec::LocalFileHeader::parse(reader));

        Ok(ZipFile
        {
            encrypted: central.encrypted,
            compression_method: central.compression_method,
            last_modified_time: central.last_modified_time,
            crc32: central.crc32,
            compressed_size: central.compressed_size as u64,
            uncompressed_size: central.uncompressed_size as u64,
            file_name: central.file_name.clone(),
            file_comment: central.file_comment.clone(),
            data_start: local.header_end,
        })
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
                    Crc32Reader::new_with_check(
                        limit_reader,
                        file.crc32)
                    as Box<Reader>
            },
            types::Deflated =>
            {
                let deflate_reader = limit_reader.deflate_decode();
                box
                    Crc32Reader::new_with_check(
                        deflate_reader,
                        file.crc32)
                    as Box<Reader>
            },
            _ => return unsupported_zip_error("Compression method not supported"),
        };
        Ok(reader)
    }
}
