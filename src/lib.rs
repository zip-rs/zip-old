//! A basic ZipReader/Writer crate

#![feature(phase)]
#![feature(unsafe_destructor)]
#![warn(missing_doc)]

#[phase(plugin, link)] extern crate log;
extern crate time;
extern crate flate2;

pub use reader::ZipReader;
pub use writer::ZipWriter;
pub use types::ZipFile;

mod util;
mod spec;
mod crc32;
mod reader;
mod types;
pub mod compression;
mod writer;
mod cp437;
