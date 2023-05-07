#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]
#![doc = include_str!("../README.md")]

mod archive;
pub mod error;
pub mod file;
pub mod metadata;

pub use archive::{Directory, Footer};

#[cfg(feature = "std")]
use std::io::*;
/// Finds all the files in a single-disk archive.
///
/// This is equivalent to
///
/// ```
/// # use zip::*; fn f(disk: std::fs::File) -> std::io::Result<()> {
/// Footer::from_io(disk)?.into_directory()?.seek_to_files::<metadata::Full>()
/// # ;Ok(())}
/// ```
#[cfg(feature = "std")]
pub fn files(disk: impl Read + Seek) -> Result<impl Iterator<Item = Result<file::File>>> {
    Footer::from_io(disk)?.into_directory()?.seek_to_files()
}
