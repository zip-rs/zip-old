//! A basic ZipReader/Writer crate

#![feature(unsafe_destructor)]
#![warn(missing_docs)]

#![feature(core, old_io, std_misc, io)]

extern crate time;
extern crate flate2;
extern crate bzip2;

pub use reader::ZipReader;
pub use writer::ZipWriter;
pub use compression::CompressionMethod;
pub use types::ZipFile;

mod util;
mod spec;
mod reader_spec;
mod writer_spec;
mod crc32;
pub mod reader;
mod types;
pub mod compression;
pub mod writer;
mod cp437;
pub mod result;
