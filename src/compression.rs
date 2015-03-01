//! Possible ZIP compression methods.

/// Compression methods for the contents of a ZIP file.
#[derive(FromPrimitive, Clone, Copy)]
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
    Unknown = 10000,
}
