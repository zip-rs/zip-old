use std::io;
use std::io::prelude::*;
use std::old_io;

pub struct IoConverter<T> {
    inner: T,
}

impl<T> IoConverter<T> {
    pub fn new(inner: T) -> IoConverter<T> {
        IoConverter { inner: inner, }
    }
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<W: Write> Writer for IoConverter<W> {
    fn write_all(&mut self, buf: &[u8]) -> old_io::IoResult<()> {
        match self.inner.write_all(buf) {
            Ok(()) => Ok(()),
            Err(..) => Err(old_io::standard_error(old_io::OtherIoError)),
        }
    }
}

impl<W: Writer> Write for IoConverter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.inner.write_all(buf) {
            Ok(()) => Ok(buf.len()),
            Err(..) => Err(io::Error::new(io::ErrorKind::Other, "Some writing error", None)),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self.inner.flush() {
            Ok(()) => Ok(()),
            Err(..) => Err(io::Error::new(io::ErrorKind::Other, "Some flushing error", None)),
        }
    }
}

impl<R: Reader> Read for IoConverter<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.inner.read(buf) {
            Ok(v) => Ok(v),
            Err(ref e) if e.kind == old_io::EndOfFile => Ok(0),
            Err(..) => Err(io::Error::new(io::ErrorKind::Other, "Some reading error", None)),
        }
    }
}

impl<R: Read> Reader for IoConverter<R> {
    fn read(&mut self, buf: &mut [u8]) -> old_io::IoResult<usize> {
        match self.inner.read(buf) {
            Ok(0) if buf.len() > 0 => Err(old_io::standard_error(old_io::EndOfFile)),
            Ok(v) => Ok(v),
            Err(..) => Err(old_io::standard_error(old_io::OtherIoError)),
        }
    }
}
