//! A basic ZipReader/Writer crate

#![feature(phase)]
#![feature(unsafe_destructor)]
#![warn(missing_doc)]

#[phase(plugin, link)] extern crate log;
extern crate time;
extern crate flate2;

pub use reader::ZipReader;
pub use writer::ZipWriter;

mod util;
mod spec;
pub mod crc32;
mod reader;
pub mod types;
mod writer;
