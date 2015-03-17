//! Possible ZIP compression methods.

/// Compression methods for the contents of a ZIP file.
#[derive(Copy, PartialEq)]
pub enum CompressionMethod
{
    /// The file is stored (no compression)
    Stored,
    /// The file is Deflated
    Deflated,
    /// File is compressed using BZIP2 algorithm
    Bzip2,
    /// Unsupported compression method
    Unsupported(u16),
}

impl CompressionMethod {
    /// Converts an u16 to its corresponding CompressionMethod
    pub fn from_u16(val: u16) -> CompressionMethod {
        match val {
            0 => CompressionMethod::Stored,
            8 => CompressionMethod::Deflated,
            12 => CompressionMethod::Bzip2,
            v => CompressionMethod::Unsupported(v),
        }
    }

    /// Converts a CompressionMethod to a u16
    pub fn to_u16(self) -> u16 {
        match self {
            CompressionMethod::Stored => 0,
            CompressionMethod::Deflated => 8,
            CompressionMethod::Bzip2 => 12,
            CompressionMethod::Unsupported(v) => v,
        }
    }
}

#[cfg(test)]
mod test {
    use super::CompressionMethod;

    #[test]
    fn from_eq_to() {
        for v in (0..::std::u16::MAX as u32 + 1)
        {
            let from = CompressionMethod::from_u16(v as u16);
            let to = from.to_u16() as u32;
            assert_eq!(v, to);
        }
    }

    #[test]
    fn to_eq_from() {
        fn check_match(method: CompressionMethod) {
            let to = method.to_u16();
            let from = CompressionMethod::from_u16(to);
            let back = from.to_u16();
            assert_eq!(to, back);
        }

        check_match(CompressionMethod::Stored);
        check_match(CompressionMethod::Deflated);
        check_match(CompressionMethod::Bzip2);
    }
}
