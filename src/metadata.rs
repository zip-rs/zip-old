pub struct RawDirectoryEntry<'a> {
    entry: &'a zip_format::DirectoryEntry,
    metadata: &'a [u8],
}
impl<'a> RawDirectoryEntry<'a> {
    pub fn name(&self) -> &'a [u8] {
        &self.metadata[..self.entry.name_len.get() as usize]
    }
}
impl<'a> RawDirectoryEntry<'a> {
    pub(crate) fn new(entry: &'a zip_format::DirectoryEntry, metadata: &'a [u8]) -> Self {
        Self { entry, metadata }
    }
}
pub trait Metadata<D>: Sized {
    type Error;
    fn from_header(header: &zip_format::Header, disk: &mut D) -> Result<Self, Self::Error>;
    fn from_entry(entry: &zip_format::DirectoryEntry, disk: &mut D) -> Result<Self, Self::Error>;
}
// TODO: Provide useful alternatives:
//       - a `Name` that only saves the name of the file
//       - something that decodes some well-known extra fields - maybe in another crate?
#[cfg(feature = "std")]
pub mod std {
    use io::Read;
    use std::io;

    pub struct Full {
        buf: Vec<u8>,
        name_len: u16,
    }
    impl Full {
        pub fn name(&self) -> &[u8] {
            &self.buf[..self.name_len as usize]
        }
    }
    impl<D: Read> super::Metadata<D> for Full {
        type Error = io::Error;
        fn from_entry(entry: &zip_format::DirectoryEntry, disk: &mut D) -> io::Result<Self> {
            let mut buf = vec![];
            let name_len = entry.name_len.get();
            disk.take(
                name_len as u64 + entry.metadata_len.get() as u64 + entry.comment_len.get() as u64,
            )
            .read_to_end(&mut buf)?;
            Ok(Self { buf, name_len })
        }
        fn from_header(header: &zip_format::Header, disk: &mut D) -> io::Result<Self> {
            let mut buf = vec![];
            let name_len = header.name_len.get();
            disk.take(name_len as u64 + header.metadata_len.get() as u64)
                .read_to_end(&mut buf)?;
            Ok(Self { buf, name_len })
        }
    }
}
