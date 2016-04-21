//! Types that specify what is contained in a ZIP.

use time;

pub const SYSTEM_MSDOS: u8 = 0;
pub const SYSTEM_UNIX: u8 = 3;

pub const DEFAULT_VERSION: u8 = 20;

/// Structure representing a ZIP file.
pub struct ZipFileData
{
    /// Compatibility of the file attribute information
    pub system: u8,
    /// Specification version
    pub version: u8,
    /// True if the file is encrypted.
    pub encrypted: bool,
    /// Compression method used to store the file
    pub compression_method: ::compression::CompressionMethod,
    /// Last modified time. This will only have a 2 second precision.
    pub last_modified_time: time::Tm,
    /// CRC32 checksum
    pub crc32: u32,
    /// Size of the file in the ZIP
    pub compressed_size: u64,
    /// Size of the file when extracted
    pub uncompressed_size: u64,
    /// Name of the file
    pub file_name: String,
    /// File comment
    pub file_comment: String,
    /// Specifies where the local header of the file starts
    pub header_start: u64,
    /// Specifies where the compressed data of the file starts
    pub data_start: u64,
    /// External file attributes
    pub external_attributes: u32,
}
