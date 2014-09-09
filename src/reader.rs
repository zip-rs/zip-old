use spec;
use std::io;
use std::io::{IoResult, IoError};

pub struct ZipContainer<T>
{
    inner: T,
    files: Vec<ZipFile>,
}

struct ZipFile
{
    central_header: spec::CentralDirectoryHeader,
    local_header: spec::LocalFileHeader,
}

struct ZipFileNames<'a, T:'a>
{
    container: &'a mut ZipContainer<T>,
    pos: uint,
}

fn unsupported_zip_error<T>(detail: Option<String>) -> IoResult<T>
{
    Err(IoError
        {
            kind: io::OtherIoError,
            desc: "This ZIP file is not supported",
            detail: detail,
        })
}

impl<T: Reader+Seek> ZipContainer<T>
{
    pub fn new(inner: T) -> IoResult<ZipContainer<T>>
    {
        let mut result = ZipContainer { inner: inner, files: Vec::new() };
        let footer = try!(spec::CentralDirectoryEnd::find_and_parse(&mut result.inner));

        if footer.number_of_disks > 1 { return unsupported_zip_error(Some("Support for multi-disk files is not implemented".to_string())) }

        let directory_start = footer.central_directory_offset as i64;
        let number_of_files = footer.number_of_files_on_this_disk as uint;
        
        let mut files = Vec::with_capacity(number_of_files);
        
        try!(result.inner.seek(directory_start, io::SeekSet));
        for i in range(0, number_of_files)
        {
            files.push(try!(ZipContainer::parse_directory(&mut result.inner)));
        }

        result.files = files;
        Ok(result)
    }

    fn parse_directory(reader: &mut T) -> IoResult<ZipFile>
    {
        let cdh = try!(spec::CentralDirectoryHeader::parse(reader));
        let pos = try!(reader.tell()) as i64;
        try!(reader.seek(cdh.file_offset as i64, io::SeekSet));
        let lfh = try!(spec::LocalFileHeader::parse(reader));
        try!(reader.seek(pos, io::SeekSet));
        Ok(ZipFile { central_header: cdh, local_header: lfh } )
    }

    pub fn files<'a>(&'a mut self) -> ZipFileNames<'a, T>
    {
        ZipFileNames { container: self, pos: 0 }
    }
}

impl<'a, T> Iterator<String> for ZipFileNames<'a, T>
{
    fn next(&mut self) -> Option<String>
    {
        self.pos += 1;
        if self.pos - 1 >= self.container.files.len()
        {
            None
        }
        else
        {
            let fname = self.container.files.get(self.pos - 1).central_header.file_name.clone();
            String::from_utf8(fname).ok()
        }
    }
}
