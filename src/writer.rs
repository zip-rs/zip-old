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
    Invalid,
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
            Invalid => Err(IoError { kind: io::OtherIoError, desc: "The writer has previously failed", detail: None }),
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

    pub fn start_new_file(&mut self, name: &[u8], compression: types::CompressionMethod) -> IoResult<()>
    {
        try!(self.finish_file());

        let writer = match self.inner
        {
            Storer(ref mut r) => r,
            _ => fail!("Should have switched to stored first"),
        };
        let header_start = try!(writer.tell());
        try!(writer.seek(30 + name.len() as i64, io::SeekCur));
        self.stats.start = header_start + 30 + name.len() as u64;

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
            data_start: self.stats.start,
        });

        Ok(())
    }

    fn finish_file(&mut self) -> IoResult<()>
    {
        try!(self.inner.switch_to(types::Stored));
        let writer = match self.inner
        {
            Storer(ref mut r) => r,
            _ => fail!("Should have switched to stored first"),
        };

        let file = match self.files.mut_last()
        {
            None => return Ok(()),
            Some(f) => f,
        };
        file.crc32 = self.stats.crc32;
        file.uncompressed_size = self.stats.bytes_written;
        file.compressed_size = try!(writer.tell()) - self.stats.start;

        // write header
        try!(spec::write_local_file_header(writer, file));
        Ok(())
    }
}

impl<W: Writer+Seek> GenericZipWriter<W>
{
    fn switch_to(&mut self, compression: types::CompressionMethod) -> IoResult<()>
    {
        let bare = match mem::replace(self, Invalid)
        {
            Storer(w) => w,
            Deflater(w) => try!(w.finish()),
            Invalid => return Err(IoError { kind: io::OtherIoError, desc: "The writer has previously failed", detail: None }),
        };

        *self = match compression
        {
            types::Stored => Storer(bare),
            types::Deflated => Deflater(bare.deflate_encode(flate2::Default)),
            _ => return Err(IoError { kind: io::OtherIoError, desc: "Unsupported compression requested", detail: None }),
        };

        Ok(())
    }
}
