use compression::CompressionMethod;
use types::ZipFile;
use spec;
use writer_spec;
use crc32;
use result::{ZipResult, ZipError};
use std::default::Default;
use std::io;
use std::io::prelude::*;
use std::mem;
use time;
use flate2;
use flate2::FlateWriteExt;
use flate2::write::DeflateEncoder;
use bzip2;
use bzip2::writer::BzCompressor;

enum GenericZipWriter<W>
{
    Closed,
    Storer(W),
    Deflater(DeflateEncoder<W>),
    Bzip2(BzCompressor<W>),
}

/// Generator for ZIP files.
///
/// ```
/// fn doit() -> zip::result::ZipResult<()>
/// {
///     use std::io::Write;
///
///     // For this example we write to a buffer, but normally you should use a File
///     let mut buf: &mut [u8] = &mut [0u8; 65536];
///     let mut w = std::io::Cursor::new(buf);
///     let mut zip = zip::ZipWriter::new(w);
///
///     try!(zip.start_file("hello_world.txt", zip::CompressionMethod::Stored));
///     try!(zip.write(b"Hello, World!"));
///
///     // Optionally finish the zip. (this is also done on drop)
///     try!(zip.finish());
///
///     Ok(())
/// }
///
/// println!("Result: {:?}", doit());
/// ```
pub struct ZipWriter<W>
{
    inner: GenericZipWriter<W>,
    files: Vec<ZipFile>,
    stats: ZipWriterStats,
}

#[derive(Default)]
struct ZipWriterStats
{
    crc32: u32,
    start: u64,
    bytes_written: u64,
}

impl<W: Write+io::Seek> Write for ZipWriter<W>
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize>
    {
        if self.files.len() == 0 { return Err(io::Error::new(io::ErrorKind::Other, "No file has been started", None)) }
        self.stats.update(buf);
        match self.inner
        {
            GenericZipWriter::Storer(ref mut w) => w.write(buf),
            GenericZipWriter::Deflater(ref mut w) => w.write(buf),
            GenericZipWriter::Bzip2(ref mut w) => w.write(buf),
            GenericZipWriter::Closed => Err(io::Error::new(io::ErrorKind::BrokenPipe, "ZipWriter was already closed", None)),
        }
    }

    fn flush(&mut self) -> io::Result<()>
    {
        let result = self.finalize();
        self.inner = GenericZipWriter::Closed;
        match result {
            Ok(..) => Ok(()),
            Err(ZipError::Io(io_err)) => Err(io_err),
            Err(..) => Err(io::Error::new(io::ErrorKind::Other, "Error occured during finalization", None)),
        }
    }
}

impl ZipWriterStats
{
    fn update(&mut self, buf: &[u8])
    {
        self.crc32 = crc32::update(self.crc32, buf);
        self.bytes_written += buf.len() as u64;
    }
}

impl<W: Write+io::Seek> ZipWriter<W>
{
    /// Initializes the ZipWriter.
    ///
    /// Before writing to this object, the start_file command should be called.
    pub fn new(inner: W) -> ZipWriter<W>
    {
        ZipWriter
        {
            inner: GenericZipWriter::Storer(inner),
            files: Vec::new(),
            stats: Default::default(),
        }
    }

    /// Start a new file for with the requested compression method.
    pub fn start_file(&mut self, name: &str, compression: CompressionMethod) -> ZipResult<()>
    {
        try!(self.finish_file());

        {
            let writer = self.inner.get_plain();
            let header_start = try!(writer.seek(io::SeekFrom::Current(0)));

            let mut file = ZipFile
            {
                encrypted: false,
                compression_method: compression,
                last_modified_time: time::now(),
                crc32: 0,
                compressed_size: 0,
                uncompressed_size: 0,
                file_name: name.to_string(),
                file_comment: String::new(),
                header_start: header_start,
                data_start: 0,
            };
            try!(writer_spec::write_local_file_header(writer, &file));

            let header_end = try!(writer.seek(io::SeekFrom::Current(0)));
            self.stats.start = header_end;
            file.data_start = header_end;

            self.stats.bytes_written = 0;
            self.stats.crc32 = 0;

            self.files.push(file);
        }

        try!(self.inner.switch_to(compression));

        Ok(())
    }

    fn finish_file(&mut self) -> ZipResult<()>
    {
        try!(self.inner.switch_to(CompressionMethod::Stored));
        let writer = self.inner.get_plain();

        let file = match self.files.last_mut()
        {
            None => return Ok(()),
            Some(f) => f,
        };
        file.crc32 = self.stats.crc32;
        file.uncompressed_size = self.stats.bytes_written;
        file.compressed_size = try!(writer.seek(io::SeekFrom::Current(0))) - self.stats.start;

        try!(writer_spec::update_local_file_header(writer, file));
        try!(writer.seek(io::SeekFrom::End(0)));
        Ok(())
    }

    /// Finish the last file and write all other zip-structures
    ///
    /// This will return the writer, but one should normally not append any data to the end of the file.  
    /// Note that the zipfile will also be finished on drop.
    pub fn finish(mut self) -> ZipResult<W>
    {
        try!(self.finalize());
        let inner = mem::replace(&mut self.inner, GenericZipWriter::Closed);
        Ok(inner.unwrap())
    }

    fn finalize(&mut self) -> ZipResult<()>
    {
        try!(self.finish_file());

        {
            let writer = self.inner.get_plain();

            let central_start = try!(writer.seek(io::SeekFrom::Current(0)));
            for file in self.files.iter()
            {
                try!(writer_spec::write_central_directory_header(writer, file));
            }
            let central_size = try!(writer.seek(io::SeekFrom::Current(0))) - central_start;

            let footer = spec::CentralDirectoryEnd
            {
                disk_number: 0,
                disk_with_central_directory: 0,
                number_of_files_on_this_disk: self.files.len() as u16,
                number_of_files: self.files.len() as u16,
                central_directory_size: central_size as u32,
                central_directory_offset: central_start as u32,
                zip_file_comment: b"zip-rs".to_vec(),
            };

            try!(footer.write(writer));
        }

        Ok(())
    }
}

#[unsafe_destructor]
impl<W: Write+io::Seek> Drop for ZipWriter<W>
{
    fn drop(&mut self)
    {
        if !self.inner.is_closed()
        {
            if let Err(e) = self.finalize() {
                let _ = write!(&mut ::std::old_io::stdio::stderr(), "ZipWriter drop failed: {:?}", e);
            }
        }
    }
}

impl<W: Write+io::Seek> GenericZipWriter<W>
{
    fn switch_to(&mut self, compression: CompressionMethod) -> ZipResult<()>
    {
        let bare = match mem::replace(self, GenericZipWriter::Closed)
        {
            GenericZipWriter::Storer(w) => w,
            GenericZipWriter::Deflater(w) => try!(w.finish()),
            GenericZipWriter::Bzip2(w) => match w.into_inner() { Ok(r) => r, Err((_, err)) => try!(Err(err)) },
            GenericZipWriter::Closed => try!(Err(io::Error::new(io::ErrorKind::BrokenPipe, "ZipWriter was already closed", None))),
        };

        *self = match compression
        {
            CompressionMethod::Stored => GenericZipWriter::Storer(bare),
            CompressionMethod::Deflated => GenericZipWriter::Deflater(bare.deflate_encode(flate2::Compression::Default)),
            CompressionMethod::Bzip2 => GenericZipWriter::Bzip2(BzCompressor::new(bare, bzip2::CompressionLevel::Default)),
            _ => return Err(ZipError::UnsupportedZipFile("Unsupported compression")),
        };

        Ok(())
    }

    fn is_closed(&self) -> bool
    {
        match *self
        {
            GenericZipWriter::Closed => true,
            _ => false,
        }
    }

    fn get_plain(&mut self) -> &mut W
    {
        match *self
        {
            GenericZipWriter::Storer(ref mut w) => w,
            _ => panic!("Should have switched to stored beforehand"),
        }
    }

    fn unwrap(self) -> W
    {
        match self
        {
            GenericZipWriter::Storer(w) => w,
            _ => panic!("Should have switched to stored beforehand"),
        }
    }
}
