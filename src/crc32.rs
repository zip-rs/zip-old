//! Helper module to compute a CRC32 checksum

use std::io;
use std::io::prelude::*;

use crc32fast::Hasher;

/// Reader that validates the CRC32 when it reaches the EOF.
pub struct Crc32Reader<R>
{
    inner: R,
    hasher: Hasher,
    check: u32,
}

impl<R> Crc32Reader<R>
{
    /// Get a new Crc32Reader which check the inner reader against checksum.
    pub fn new(inner: R, checksum: u32) -> Crc32Reader<R>
    {
        Crc32Reader
        {
            inner: inner,
            hasher: Hasher::new(),
            check: checksum,
        }
    }

    fn check_matches(&self) -> bool
    {
        self.check == self.hasher.clone().finalize()
    }

    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: Read> Read for Crc32Reader<R>
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>
    {
        let count = match self.inner.read(buf)
        {
            Ok(0) if !self.check_matches() => { return Err(io::Error::new(io::ErrorKind::Other, "Invalid checksum")) },
            Ok(n) => n,
            Err(e) => return Err(e),
        };
        self.hasher.update(&buf[0..count]);
        Ok(count)
    }
}
