use core::fmt;

macro_rules! gen {
    {
        $($n:ident $p:ident)*;
        $(
            $name:ident : $magic:literal{
                $($field:ident:$t:ident,)*
            }
        )*
    } => {
        $(
            #[derive(Copy, Clone, PartialEq, Eq)]
            pub struct $n([u8; core::mem::size_of::<$p>()]);
            impl $n {
                pub fn get(&self) -> $p {
                    $p::from_le_bytes(self.0)
                }
                #[allow(unused)]
                const fn new(n: $p) -> Self {
                    Self(n.to_le_bytes())
                }
            }
            impl fmt::Debug for $n {
                fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    self.get().fmt(f)
                }
            }
        )*
        mod types {
            pub use super::{CompressionMethod, $($n),*};
        }
        $(
            #[repr(C)]
            #[derive(Debug)]
            pub struct $name {
                $(pub $field: types::$t),*
            }
            impl $name {
                pub fn as_prefix(bytes: &[u8]) -> Option<&Self> {
                    (bytes.len() >= core::mem::size_of::<Self>() + 4)
                        .then(|| unsafe { &*(bytes.as_ptr() as *const (U32, Self)) })
                        .filter(|p| p.0.get() == $magic)
                        .map(|p| &p.1)
                }
                pub fn as_suffix(bytes: &[u8]) -> Option<&Self> {
                    bytes.get(bytes.len() - core::mem::size_of::<Self>() - 4..).and_then(Self::as_prefix)
                }
            }
        )*
    }
}
gen! {
    U16 u16 U32 u32 U64 u64;
    
    Footer: 0x06054b50 {
        disk_number: U16,
        directory_start_disk: U16,
        entries_on_this_disk: U16,
        entries: U16,
        directory_size: U32,
        offset_from_start: U32,
        comment_length: U16,
    }
    FooterLocator: 0x06054b50 {
        directory_start_disk: U32,
        footer_offset: U64,
        disk_count: U32,
    }
    FooterV2: 0x06054b50 {
        footer_size: U64,
        made_by: U16,
        required_version: U16,
        disk_number: U32,
        directory_start_disk: U64,
        entries_on_this_disk: U64,
        entries: U64,
        directory_size: U64,
        offset_from_start: U64,
    }
    DirectoryEntry: 0x02014b50 {
        made_by: U16,
        required_version: U16,
        flags: U16,
        method: CompressionMethod,
        modified_time: U16,
        modified_date: U16,
        crc32: U32,
        compressed_size: U32,
        uncompressed_size: U32,
        name_len: U16,
        metadata_len: U16,
        comment_len: U16,
    
        disk_number: U16,
        internal_attrs: U16,
        external_attrs: U32,
        offset_from_start: U32,
    }
    Header: 0x04034b50 {
        required_version: U16,
        flags: U16,
        method: CompressionMethod,
        modified_time: U16,
        modified_date: U16,
        crc32: U32,
        compressed_size: U32,
        uncompressed_size: U32,
        name_len: U16,
        metadata_len: U16,
    }
}

#[repr(transparent)]
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct CompressionMethod(U16);
impl CompressionMethod {
    pub const STORED: Self = Self(U16::new(0));
    pub const DEFLATE: Self = Self(U16::new(8));
}

impl fmt::Debug for CompressionMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CompressionMethod({})", self.0.get())
    }
}
/// Each method will be converted to a description that fits in the sentence:
/// `"this zip file was stored using {method}"`
/// 
/// I *think* you'd call it a noun phrase, though I've never been great at
/// linguistics.
impl fmt::Display for CompressionMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.0.get() {
            0 => "no compression",
            
            1 => "a deprecated method (shrunk)",
            2 => "a deprecated method (reduced with factor 1)",
            3 => "a deprecated method (reduced with factor 2)",
            4 => "a deprecated method (reduced with factor 3)",
            5 => "a deprecated method (reduced with factor 4)",
            6 => "a deprecated method (imploded)",

            7  => "a reserved method (PKWARE tokenizing algorithm)",
            11 => "a reserved method (11)",
            13 => "a reserved method (13)",
            15 => "a reserved method (15)",
            17 => "a reserved method (17)",

            8  => "deflate compression",
            9  => "deflate64 compression",
            10 => "PKWARE TERSE",
            12 => "BZIP2 compression",
            14 => "LZMA compression",
            16 => "IBM z/OS CMPSC compression",
            18 => "IBM TERSE",
            19 => "LZ77 compression",

            20 => "a deprecated method (zstd)",

            93 => "zstd compression",
            94 => "MP3 encoding",
            95 => "XZ compression",
            96 => "JPEG encoding",
            97 => "WavPack compression",
            98 => "PPMd compression",

            99 => "AE-x encryption",

            v => return write!(f, "an unknown method ({v})"),
        })
    }
}