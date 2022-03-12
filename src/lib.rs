mod archive;
mod persisted;

pub mod error;
pub mod file;
pub mod metadata;

pub use archive::{Directory, Footer};
pub use persisted::Persisted;

use std::io::*;

pub fn files(disk: impl Read + Seek) -> Result<impl Iterator<Item = Result<file::File>>> {
    Footer::from_io(disk)?.into_directory()?.seek_to_files()
}
