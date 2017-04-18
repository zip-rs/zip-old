//! Types that specify what is contained in a ZIP.

use time;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum System {
    Dos = 0,
    Unix = 3,
    Unknown,
    #[doc(hidden)]
    __Nonexhaustive,
}

impl System {
    pub fn from_u8(system: u8) -> System {
        use self::System::*;

        match system {
            0 => Dos,
            3 => Unix,
            _ => Unknown,
        }
    }
}

pub const DEFAULT_VERSION: u8 = 20;

mod ffi {
    pub const S_IFDIR: u32 = 0o0040000;
    pub const S_IFREG: u32 = 0o0100000;
}

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
    /// Specifies where the compressed data of the file starts
    pub data_start: u64,
    /// External file attributes
    pub external_attributes: u32,
}

impl ZipFileData {
    pub fn unix_mode(&self) -> Option<u32> {
        match self.system {
            System::Unix => Some(self.external_attributes >> 16),
            System::Dos => {
                // Interpret MSDOS directory bit
                let mut mode = if 0x10 == (self.external_attributes & 0x10) {
                    ffi::S_IFDIR | 0o0775
                } else {
                    ffi::S_IFREG | 0o0664
                };
                if 0x01 == (self.external_attributes & 0x01) {
                    // Read-only bit; strip write permissions
                    mode &= 0o0555;
                }
                Some(mode)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn system() {
        use super::System;
        assert_eq!(System::Dos as u16, 0u16);
        assert_eq!(System::Unix as u16, 3u16);
        assert_eq!(System::from_u8(0), System::Dos);
        assert_eq!(System::from_u8(3), System::Unix);
    }
}
