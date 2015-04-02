//! A basic ZipReader/Writer crate

#![feature(unsafe_destructor)]
#![warn(missing_docs)]

#![feature(step_by)]

extern crate time;
extern crate flate2;
extern crate bzip2;
extern crate podio;

pub use read::ZipArchive;
pub use write::ZipWriter;
pub use compression::CompressionMethod;

mod util;
mod spec;
mod crc32;
mod types;
pub mod read;
mod compression;
pub mod write;
mod cp437;
pub mod result;
