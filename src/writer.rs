use types;
use types::ZipFile;
use spec;
use std::default::Default;
use std::io;
use std::io::{IoResult, IoError};
use std::mem;
use time;
use flate2;
use flate2::FlateWriter;
use flate2::writer::DeflateEncoder;

enum GenericZipWriter<W>
{
    Closed,
    Storer(W),
    Deflater(DeflateEncoder<W>),
}

pub struct ZipWriter<W>
{
    inner: GenericZipWriter<W>,
    files: Vec<ZipFile>,
    stats: ZipWriterStats,
}

#[deriving(Default)]
struct ZipWriterStats
{
    crc32: u32,
    start: u64,
    bytes_written: u64,
}

fn writer_closed_error<T>() -> IoResult<T>
{
    Err(IoError { kind: io::Closed, desc: "This writer has been closed", detail: None })
}

impl<W: Writer+Seek> Writer for ZipWriter<W>
{
    fn write(&mut self, buf: &[u8]) -> IoResult<()>
    {
        if self.files.len() == 0 { return Err(IoError { kind: io::OtherIoError, desc: "No file has been started", detail: None, }) }
        self.stats.update(buf);
        match self.inner
        {
            Storer(ref mut w) => w.write(buf),
            Deflater(ref mut w) => w.write(buf),
            Closed => writer_closed_error(),
        }
    }
}

impl ZipWriterStats
{
    fn update(&mut self, buf: &[u8])
    {
        self.crc32 = ::crc32::crc32(self.crc32, buf);
        self.bytes_written += buf.len() as u64;
    }
}

impl<W: Writer+Seek> ZipWriter<W>
{
    pub fn new(inner: W) -> ZipWriter<W>
    {
        ZipWriter
        {
            inner: Storer(inner),
            files: Vec::new(),
            stats: Default::default(),
        }
    }

    pub fn start_file(&mut self, name: &[u8], compression: types::CompressionMethod) -> IoResult<()>
    {
        try!(self.finish_file());

        {
            let writer = self.inner.get_plain();
            let header_start = try!(writer.tell());
            try!(writer.seek(30 + name.len() as i64, io::SeekCur));

            self.stats.start = header_start + 30 + name.len() as u64;
            self.stats.bytes_written = 0;
            self.stats.crc32 = 0;

            self.files.push(ZipFile
            {
                encrypted: false,
                compression_method: compression,
                last_modified_time: time::now(),
                crc32: 0,
                compressed_size: 0,
                uncompressed_size: 0,
                file_name: Vec::from_slice(name),
                file_comment: Vec::new(),
                header_start: header_start,
                data_start: self.stats.start,
            });
        }

        try!(self.inner.switch_to(compression));

        Ok(())
    }

    fn finish_file(&mut self) -> IoResult<()>
    {
        try!(self.inner.switch_to(types::Stored));
        let writer = self.inner.get_plain();

        let file = match self.files.mut_last()
        {
            None => return Ok(()),
            Some(f) => f,
        };
        file.crc32 = self.stats.crc32;
        file.uncompressed_size = self.stats.bytes_written;
        file.compressed_size = try!(writer.tell()) - self.stats.start;

        try!(writer.seek(file.header_start as i64, io::SeekSet));
        try!(spec::write_local_file_header(writer, file));
        try!(writer.seek(0, io::SeekEnd));
        Ok(())
    }

    pub fn finalize(&mut self) -> IoResult<()>
    {
        try!(self.finish_file());

        {
            let writer = self.inner.get_plain();

            let central_start = try!(writer.tell());
            for file in self.files.iter()
            {
                try!(spec::write_central_directory_header(writer, file));
            }
            let central_size = try!(writer.tell()) - central_start;

            let footer = spec::CentralDirectoryEnd
            {
                disk_number: 0,
                disk_with_central_directory: 0,
                number_of_files_on_this_disk: self.files.len() as u16,
                number_of_files: self.files.len() as u16,
                central_directory_size: central_size as u32,
                central_directory_offset: central_start as u32,
                zip_file_comment: Vec::from_slice(b"zip-rs"),
            };

            try!(footer.write(writer));
        }

        self.inner = Closed;
        Ok(())
    }
}

#[unsafe_destructor]
impl<W: Writer+Seek> Drop for ZipWriter<W>
{
    fn drop(&mut self)
    {
        if !self.inner.is_closed()
        {
            match self.finalize()
            {
                Ok(_) => {},
                Err(e) => warn!("ZipWriter drop failed: {}", e),
            }
        }
    }
}

impl<W: Writer+Seek> GenericZipWriter<W>
{
    fn switch_to(&mut self, compression: types::CompressionMethod) -> IoResult<()>
    {
        let bare = match mem::replace(self, Closed)
        {
            Storer(w) => w,
            Deflater(w) => try!(w.finish()),
            Closed => return writer_closed_error(),
        };

        *self = match compression
        {
            types::Stored => Storer(bare),
            types::Deflated => Deflater(bare.deflate_encode(flate2::Default)),
            _ => return Err(IoError { kind: io::OtherIoError, desc: "Unsupported compression requested", detail: None }),
        };

        Ok(())
    }

    fn is_closed(&self) -> bool
    {
        match *self
        {
            Closed => true,
            _ => false,
        }
    }

    fn get_plain(&mut self) -> &mut W
    {
        match *self
        {
            Storer(ref mut w) => w,
            _ => fail!("Should have switched to stored beforehand"),
        }
    }
}
