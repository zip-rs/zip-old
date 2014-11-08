//! Error types that can be emitted from this library

use std::io;
use std::error;

pub type ZipResult<T> = Result<T, ZipError>;

/// Error type for Zip
#[deriving(Show)]
pub enum ZipError
{
    /// An Error caused by I/O
    Io(io::IoError),

    /// This file is probably not a zipfile. The argument is enclosed.
    InvalidZipFile(&'static str),

    /// This file is unsupported. The reason is enclosed.
    UnsupportedZipFile(&'static str),

    /// The ZipReader is not available.
    ReaderUnavailable,
}

impl error::FromError<io::IoError> for ZipError
{
    fn from_error(err: io::IoError) -> ZipError
    {
        Io(err)
    }
}

impl error::Error for ZipError
{
    fn description(&self) -> &str
    {
        match *self
        {
            Io(ref io_err) => io_err.description(),
            InvalidZipFile(..) => "Invalid Zip File",
            UnsupportedZipFile(..) => "Unsupported Zip File",
            ReaderUnavailable => "No reader available",
        }
    }

    fn detail(&self) -> Option<String>
    {
        match *self
        {
            Io(ref io_err) => io_err.detail(),
            InvalidZipFile(detail) |
            UnsupportedZipFile(detail) => Some(detail.to_string()),
            ReaderUnavailable => None,
        }
    }

    fn cause(&self) -> Option<&error::Error>
    {
        match *self
        {
            Io(ref io_err) => Some(io_err as &error::Error),
            _ => None,
        }
    }
}
