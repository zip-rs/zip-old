//! A basic ZipReader/Writer crate

#![warn(missing_docs)]

pub use crate::read::ZipArchive;
pub use crate::write::ZipWriter;
pub use crate::compression::CompressionMethod;
pub use crate::types::DateTime;

mod spec;
mod crc32;
mod types;
pub mod read;
mod compression;
pub mod write;
mod cp437;
pub mod result;
