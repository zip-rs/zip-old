//! Error types that can be emitted from this library

use std::old_io::IoError;
use std::error;
use std::fmt;

/// Generic result type with ZipError as its error variant
pub type ZipResult<T> = Result<T, ZipError>;

/// Error type for Zip
#[derive(Show)]
pub enum ZipError
{
    /// An Error caused by I/O
    Io(IoError),

    /// This file is probably not a zipfile. The argument is enclosed.
    InvalidZipFile(&'static str),

    /// This file is unsupported. The reason is enclosed.
    UnsupportedZipFile(&'static str),

    /// The ZipReader is not available.
    ReaderUnavailable,
}

impl ZipError
{
    fn detail(&self) -> ::std::string::CowString
    {
        use ::std::error::Error;
        use ::std::borrow::IntoCow;

        match *self
        {
            ZipError::Io(ref io_err) => {
                ("Io Error: ".to_string() + io_err.description()).into_cow()
            },
            ZipError::InvalidZipFile(msg) | ZipError::UnsupportedZipFile(msg) => {
                (self.description().to_string() + ": " + msg).into_cow()
            },
            ZipError::ReaderUnavailable => {
                self.description().into_cow()
            },
        }
    }
}

impl error::FromError<IoError> for ZipError
{
    fn from_error(err: IoError) -> ZipError
    {
        ZipError::Io(err)
    }
}

impl fmt::Display for ZipError
{
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error>
    {
        fmt.write_str(&*self.detail())
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

    fn cause(&self) -> Option<&error::Error>
    {
        match *self
        {
            ZipError::Io(ref io_err) => Some(io_err as &error::Error),
            _ => None,
        }
    }
}
