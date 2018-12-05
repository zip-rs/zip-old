//! A basic ZipReader/Writer crate

#![warn(missing_docs)]

extern crate bytes;
#[cfg(feature = "bzip2")]
extern crate bzip2;
extern crate crc32fast;
#[cfg(feature = "deflate")]
extern crate libflate;
extern crate podio;
extern crate string;
#[cfg(feature = "time")]
extern crate time;

pub use read::ZipArchive;
pub use write::ZipWriter;
pub use compression::CompressionMethod;
pub use types::DateTime;

mod buffer;
mod spec;
mod crc32;
mod types;
pub mod read;
mod compression;
pub mod write;
mod cp437;
pub mod result;
