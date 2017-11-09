use time;
use system::System;

/// Structure representing a ZIP file.
#[derive(Debug)]
pub struct ZipFileData {
    /// Compatibility of the file attribute information
    pub system: System,
    /// Specification version
    pub version_made_by: u8,
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
    /// Raw file name. To be used when file_name was incorrectly decoded.
    pub file_name_raw: Vec<u8>,
    /// File comment
    pub file_comment: String,
    /// Specifies where the local header of the file starts
    pub header_start: u64,
    /// External file attributes
    pub external_attributes: u32,
}