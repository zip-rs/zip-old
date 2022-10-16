//! Truncating writable streams.
// Borrowed and modified from the `trunc` crate

use std::fs::File;
use std::io;

#[allow(missing_docs)]
pub trait Truncate {
    fn truncate(&mut self, size: u64) -> io::Result<()>;
}

impl Truncate for File {
    fn truncate(&mut self, size: u64) -> io::Result<()> {
        self.set_len(size)
    }
}

impl<'a> Truncate for &'a File {
    fn truncate(&mut self, size: u64) -> io::Result<()> {
        self.set_len(size)
    }
}

impl<R: Truncate + io::Read> Truncate for io::BufReader<R> {
    fn truncate(&mut self, size: u64) -> io::Result<()> {
        self.get_mut().truncate(size)
    }
}

impl<W: Truncate + io::Write> Truncate for io::BufWriter<W> {
    fn truncate(&mut self, size: u64) -> io::Result<()> {
        self.get_mut().truncate(size)
    }
}

impl Truncate for Vec<u8> {
    fn truncate(&mut self, size: u64) -> io::Result<()> {
        Ok(self.resize(size as usize, 0))
    }
}

impl Truncate for io::Cursor<Vec<u8>> {
    fn truncate(&mut self, size: u64) -> io::Result<()> {
        Ok(self.get_mut().resize(size as usize, 0))
    }
}

impl<'a, T: Truncate + ?Sized> Truncate for &'a mut T {
    fn truncate(&mut self, size: u64) -> io::Result<()> {
        (**self).truncate(size)
    }
}

impl<'a, T: Truncate + ?Sized> Truncate for Box<T> {
    fn truncate(&mut self, size: u64) -> io::Result<()> {
        (**self).truncate(size)
    }
}
