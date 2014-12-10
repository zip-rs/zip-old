//! Error types that can be emitted from this library

use std::io;
use std::error;

/// Generic result type with ZipError as its error variant
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
        ZipError::Io(err)
    }
}

impl error::Error for ZipError
{
    fn description(&self) -> &str
    {
        match *self
        {
            ZipError::Io(ref io_err) => io_err.description(),
            ZipError::InvalidZipFile(..) => "Invalid Zip File",
            ZipError::UnsupportedZipFile(..) => "Unsupported Zip File",
            ZipError::ReaderUnavailable => "No reader available",
        }
    }

    fn detail(&self) -> Option<String>
    {
        match *self
        {
            ZipError::Io(ref io_err) => io_err.detail(),
            ZipError::InvalidZipFile(detail) |
            ZipError::UnsupportedZipFile(detail) => Some(detail.to_string()),
            ZipError::ReaderUnavailable => None,
        }
    }

    fn cause(&self) -> Option<&error::Error>
    {
        match *self
        {
            ZipError::Io(ref io_err) => Some(io_err as &error::Error),
            _ => None,
        }
    }
}
