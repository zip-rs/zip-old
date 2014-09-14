//! Types that specify what is contained in a ZIP.

use time;

/// Compression methods for the contents of a ZIP file.
#[deriving(FromPrimitive, Clone)]
pub enum CompressionMethod
{
    /// The file is stored (no compression)
    Stored = 0,
    /// The file is Shrunk
    Shrunk = 1,
    /// The file is Reduced with compression factor 1
    Reduced1 = 2,
    /// The file is Reduced with compression factor 2
    Reduced2 = 3,
    /// The file is Reduced with compression factor 3
    Reduced3 = 4,
    /// The file is Reduced with compression factor 4
    Reduced4 = 5,
    /// The file is Imploded
    Imploded = 6,
    /// The file is Deflated
    Deflated = 8,
    /// Enhanced Deflating using Deflate64(tm)
    Deflate64 = 9,
    /// PKWARE Data Compression Library Imploding (old IBM TERSE)
    PkwareImploding = 10,
    /// File is compressed using BZIP2 algorithm
    Bzip2 = 12,
    /// LZMA (EFS)
    LZMA = 14,
    /// File is compressed using IBM TERSE (new)
    IBMTerse = 18,
    /// IBM LZ77 z Architecture (PFS)
    LZ77 = 19,
    /// WavPack compressed data
    WavPack = 97,
    /// PPMd version I, Rev 1
    PPMdI1 = 98,
    /// Unknown (invalid) compression
    Unknown = 100000,
}

/// Structure representing a ZIP file.
pub struct ZipFile
{
    /// True if the file is encrypted.
    pub encrypted: bool,
    /// Compression method used to store the file
    pub compression_method: CompressionMethod,
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
}
