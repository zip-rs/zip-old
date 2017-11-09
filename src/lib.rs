//! A basic ZipReader/Writer crate

#![warn(missing_docs)]

#[cfg(feature = "bzip2")]
extern crate bzip2;
extern crate flate2;
extern crate msdos_time;
extern crate podio;
extern crate time;

pub use zip_file::ZipFile;
pub use zip_archive::ZipArchive;
pub use write::ZipWriter;
pub use compression::CompressionMethod;

mod spec;
mod crc32;
mod system;
mod zip_file_reader;
mod zip_file_data;
mod zip_archive;
mod zip_file;
mod central_directory;



mod compression;
pub mod write;
mod cp437;
pub mod result;
