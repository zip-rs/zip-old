//! A basic ZipReader/Writer crate

#![warn(missing_docs)]

#[cfg(feature = "bzip2")]
extern crate bzip2;
extern crate crc32fast;
#[cfg(feature = "deflate")]
extern crate libflate;
extern crate podio;
#[cfg(feature = "time")]
extern crate time;

pub use compression::CompressionMethod;
pub use read::ZipArchive;
pub use types::{DateTime, ZipFileId};
pub use write::ZipWriter;

mod compression;
mod cp437;
mod crc32;
pub mod read;
pub mod result;
mod spec;
mod types;
pub mod write;
