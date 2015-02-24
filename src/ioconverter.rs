use std::io;
use std::io::prelude::*;
use std::old_io;
use std::cell::RefCell;

pub struct IoConverter<T> {
    inner: RefCell<T>,
    eofs: usize,
}

impl<T> IoConverter<T> {
    pub fn new(inner: T) -> IoConverter<T> {
        IoConverter { inner: RefCell::new(inner), eofs: 0, }
    }
    pub fn into_inner(self) -> T {
        self.inner.into_inner()
    }
}

impl<W: Write> Writer for IoConverter<W> {
    fn write_all(&mut self, buf: &[u8]) -> old_io::IoResult<()> {
        match self.inner.borrow_mut().write_all(buf) {
            Ok(()) => Ok(()),
            Err(..) => Err(old_io::standard_error(old_io::OtherIoError)),
        }
    }
}

impl<W: Writer> Write for IoConverter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.inner.borrow_mut().write_all(buf) {
            Ok(()) => Ok(buf.len()),
            Err(..) => Err(io::Error::new(io::ErrorKind::Other, "Some writing error", None)),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self.inner.borrow_mut().flush() {
            Ok(()) => Ok(()),
            Err(..) => Err(io::Error::new(io::ErrorKind::Other, "Some flushing error", None)),
        }
    }
}

impl<W: io::Seek> old_io::Seek for IoConverter<W> {
    fn tell(&self) -> old_io::IoResult<u64> {
        match self.inner.borrow_mut().seek(io::SeekFrom::Current(0)) {
            Ok(v) => Ok(v),
            Err(..) => Err(old_io::standard_error(old_io::OtherIoError)),
        }
    }
    fn seek(&mut self, pos: i64, style: old_io::SeekStyle) -> old_io::IoResult<()> {
        let new_pos = match style {
            old_io::SeekSet => io::SeekFrom::Start(pos as u64),
            old_io::SeekEnd => io::SeekFrom::End(pos),
            old_io::SeekCur => io::SeekFrom::Current(pos),
        };
        match self.inner.borrow_mut().seek(new_pos) {
            Ok(..) => Ok(()),
            Err(..) => Err(old_io::standard_error(old_io::OtherIoError)),
        }
    }
}

impl<R: old_io::Reader> Read for IoConverter<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self.inner.borrow_mut().read(buf) {
            Ok(v) => Ok(v),
            Err(ref e) if e.kind == old_io::EndOfFile => Ok(0),
            Err(..) => Err(io::Error::new(io::ErrorKind::Other, "Some reading error", None)),
        }
    }
}

impl<R: Read> Reader for IoConverter<R> {
    fn read(&mut self, buf: &mut [u8]) -> old_io::IoResult<usize> {
        match self.inner.borrow_mut().read(buf) {
            Ok(0) => {
                if self.eofs >= 2 {
                    Err(old_io::standard_error(old_io::EndOfFile))
                } else {
                    self.eofs += 1;
                    Ok(0)
                }
            },
            Ok(v) => Ok(v),
            Err(..) => Err(old_io::standard_error(old_io::OtherIoError)),
        }
    }
}
