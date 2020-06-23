//! Possible ZIP compression methods.

use std::fmt;

#[allow(deprecated)]
/// Compression methods for the contents of a ZIP file.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum CompressionMethod {
    /// The file is stored (no compression)
    Stored,
    /// Deflate in pure rust
    #[cfg(feature = "deflate")]
    Deflated,
    /// File is compressed using BZIP2 algorithm
    #[cfg(feature = "bzip2")]
    Bzip2,
    /// Unsupported compression method
    #[deprecated(
        since = "0.5.7",
        note = "implementation details are being removed from the public API"
    )]
    Unsupported(u16),
}

impl CompressionMethod {
    /// Converts an u16 to its corresponding CompressionMethod
    #[deprecated(
        since = "0.5.7",
        note = "implementation details are being removed from the public API"
    )]
    pub fn from_u16(val: u16) -> CompressionMethod {
        #[allow(deprecated)]
        match val {
            0 => CompressionMethod::Stored,
            #[cfg(feature = "deflate")]
            8 => CompressionMethod::Deflated,
            #[cfg(feature = "bzip2")]
            12 => CompressionMethod::Bzip2,

            v => CompressionMethod::Unsupported(v),
        }
    }

    /// Converts a CompressionMethod to a u16
    #[deprecated(
        since = "0.5.7",
        note = "implementation details are being removed from the public API"
    )]
    pub fn to_u16(self) -> u16 {
        #[allow(deprecated)]
        match self {
            CompressionMethod::Stored => 0,
            #[cfg(feature = "deflate")]
            CompressionMethod::Deflated => 8,
            #[cfg(feature = "bzip2")]
            CompressionMethod::Bzip2 => 12,
            CompressionMethod::Unsupported(v) => v,
        }
    }
}

impl fmt::Display for CompressionMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Just duplicate what the Debug format looks like, i.e, the enum key:
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod test {
    use super::CompressionMethod;

    #[test]
    fn from_eq_to() {
        for v in 0..(::std::u16::MAX as u32 + 1) {
            #[allow(deprecated)]
            let from = CompressionMethod::from_u16(v as u16);
            #[allow(deprecated)]
            let to = from.to_u16() as u32;
            assert_eq!(v, to);
        }
    }

    fn methods() -> Vec<CompressionMethod> {
        let mut methods = Vec::new();
        methods.push(CompressionMethod::Stored);
        #[cfg(feature = "deflate")]
        methods.push(CompressionMethod::Deflated);
        #[cfg(feature = "bzip2")]
        methods.push(CompressionMethod::Bzip2);
        methods
    }

    #[test]
    fn to_eq_from() {
        fn check_match(method: CompressionMethod) {
            #[allow(deprecated)]
            let to = method.to_u16();
            #[allow(deprecated)]
            let from = CompressionMethod::from_u16(to);
            #[allow(deprecated)]
            let back = from.to_u16();
            assert_eq!(to, back);
        }

        for method in methods() {
            check_match(method);
        }
    }

    #[test]
    fn to_display_fmt() {
        fn check_match(method: CompressionMethod) {
            let debug_str = format!("{:?}", method);
            let display_str = format!("{}", method);
            assert_eq!(debug_str, display_str);
        }

        for method in methods() {
            check_match(method);
        }
    }
}
