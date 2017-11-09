use flate2;
use std::io::{Take, Read};

#[cfg(feature = "bzip2")]
pub use bzip2::read::BzDecoder;
pub use crc32::Crc32Reader;

pub enum ZipFileReader<'a> {
    Stored(Crc32Reader<Take<&'a mut Read>>),
    Deflated(Crc32Reader<flate2::read::DeflateDecoder<Take<&'a mut Read>>>),
    #[cfg(feature = "bzip2")]
    Bzip2(Crc32Reader<BzDecoder<Take<&'a mut Read>>>),
}