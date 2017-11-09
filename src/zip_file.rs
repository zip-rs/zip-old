//! Structs for reading a ZIP archive

use compression::CompressionMethod;
use std::io::{Read, Result};

use system::System;
use zip_file_data::ZipFileData;
use zip_file_reader::ZipFileReader;

mod ffi {
    pub const S_IFDIR: u32 = 0o0040000;
    pub const S_IFREG: u32 = 0o0100000;
}

/// A struct for reading a zip file
pub struct ZipFile<'a> {
    data: &'a ZipFileData,
    reader: ZipFileReader<'a>,
}


/// Methods for retreiving information on zip files
impl<'a> ZipFile<'a> {
    /// Create ZipFile from ZipFileData and ZiptFileReader
    pub fn new(data: &'a ZipFileData, reader: ZipFileReader<'a>) -> Self {
        ZipFile {
            reader: reader,
            data: data,
        }
    }

    fn get_reader(&mut self) -> &mut Read {
        match self.reader {
            ZipFileReader::Stored(ref mut r) => r as &mut Read,
            ZipFileReader::Deflated(ref mut r) => r as &mut Read,
            #[cfg(feature = "bzip2")]
            ZipFileReader::Bzip2(ref mut r) => r as &mut Read,
        }
    }
    /// Get the version of the file
    pub fn version_made_by(&self) -> (u8, u8) {
        (
            self.data.version_made_by / 10,
            self.data.version_made_by % 10,
        )
    }
    /// Get the name of the file
    pub fn name(&self) -> &str {
        &*self.data.file_name
    }
    /// Get the name of the file, in the raw (internal) byte representation.
    pub fn name_raw(&self) -> &[u8] {
        &*self.data.file_name_raw
    }
    /// Get the comment of the file
    pub fn comment(&self) -> &str {
        &*self.data.file_comment
    }
    /// Get the compression method used to store the file
    pub fn compression(&self) -> CompressionMethod {
        self.data.compression_method
    }
    /// Get the size of the file in the archive
    pub fn compressed_size(&self) -> u64 {
        self.data.compressed_size
    }
    /// Get the size of the file when uncompressed
    pub fn size(&self) -> u64 {
        self.data.uncompressed_size
    }
    /// Get the time the file was last modified
    pub fn last_modified(&self) -> ::time::Tm {
        self.data.last_modified_time
    }
    /// Get unix mode for the file
    pub fn unix_mode(&self) -> Option<u32> {
        match self.data.system {
            System::Unix => Some(self.data.external_attributes >> 16),
            System::Dos => {
                // Interpret MSDOS directory bit
                let mut mode = if 0x10 == (self.data.external_attributes & 0x10) {
                    ffi::S_IFDIR | 0o0775
                } else {
                    ffi::S_IFREG | 0o0664
                };
                if 0x01 == (self.data.external_attributes & 0x01) {
                    // Read-only bit; strip write permissions
                    mode &= 0o0555;
                }
                Some(mode)
            }
            _ => None,
        }
    }
    /// Get the CRC32 hash of the original file
    pub fn crc32(&self) -> u32 {
        self.data.crc32
    }
    /// Get the offset of the payload of the original file
    pub fn offset(&self) -> u64 {
        self.data.header_start
    }
}

impl<'a> Read for ZipFile<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        self.get_reader().read(buf)
    }
}
