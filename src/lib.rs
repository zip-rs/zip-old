//! A basic ZipReader/Writer crate

#![warn(missing_docs)]

#[cfg(feature = "bzip2")]
extern crate bzip2;
#[cfg(feature = "chrono")]
extern crate chrono;
#[cfg(feature = "flate2")]
extern crate flate2;
#[cfg(not(feature = "chrono"))]
extern crate msdos_time;
extern crate podio;
#[cfg(not(feature = "chrono"))]
extern crate time;

pub use read::ZipArchive;
pub use write::ZipWriter;
pub use compression::CompressionMethod;

mod spec;
mod crc32;
mod types;
pub mod read;
mod compression;
pub mod write;
mod cp437;
pub mod result;
