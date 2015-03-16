//! Possible ZIP compression methods.

/// Compression methods for the contents of a ZIP file.
#[derive(Clone, Copy)]
pub enum CompressionMethod
{
    /// The file is stored (no compression)
    Stored = 0,
    /// The file is Deflated
    Deflated = 8,
    /// File is compressed using BZIP2 algorithm
    Bzip2 = 12,
    /// Unsupported compression method
    Unsupported = ::std::u16::MAX as isize,
}

impl CompressionMethod {
    /// Converts an u16 to its corresponding CompressionMethod
    pub fn from_u16(val: u16) -> CompressionMethod {
        match val {
            0 => CompressionMethod::Stored,
            8 => CompressionMethod::Deflated,
            12 => CompressionMethod::Bzip2,
            _ => CompressionMethod::Unsupported,
        }
    }
}

#[cfg(test)]
mod test {
    use super::CompressionMethod;

    #[test]
    fn from_u16() {
        for v in (0..::std::u16::MAX as u32 + 1)
        {
            let method = CompressionMethod::from_u16(v as u16);
            match method {
                CompressionMethod::Unsupported => {},
                supported => assert_eq!(v, supported as u32),
            }
        }
    }

    #[test]
    fn to_u16() {
        fn check_match(method: CompressionMethod) {
            assert!(method as u32 == CompressionMethod::from_u16(method as u16) as u32);
        }

        check_match(CompressionMethod::Stored);
        check_match(CompressionMethod::Deflated);
        check_match(CompressionMethod::Bzip2);
        check_match(CompressionMethod::Unsupported);
    }
}
