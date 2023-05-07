use crate::error;

#[path = "file/read.rs"]
mod read_imp;
pub mod read {
    pub use super::read_imp::{DecryptBuilder, Decrypted, MaybeEncrypted, NotEncrypted, ReadBuilder, Store};
}
use read_imp::*;
pub use read_imp::{Decrypt, Read};

#[cfg(not(feature = "std"))]
type Default = ();
#[cfg(feature = "std")]
type Default = crate::metadata::std::Full;
pub struct File<M = Default, D = ()> {
    pub disk: D,
    pub meta: M,
    pub locator: FileLocator,
}
// metadata needed for this crate to read the contents of the file
#[derive(Clone, Debug)]
pub struct FileLocator {
    pub(crate) disk_id: u32,
    pub(crate) header_start: u64,
    pub(crate) content_len: Option<u64>,
    pub(crate) encrypted: Option<u8>,
}
impl FileLocator {
    pub(crate) fn from_entry(entry: &zip_format::DirectoryEntry) -> Self {
        let flags = entry.flags.get();
        let size = entry.compressed_size.get() as u64;
        let has_data_descriptor = flags & 0b1000 != 0;
        Self {
            encrypted: (flags & 0b1 != 0).then(|| (entry.crc32.get() >> 24) as u8),
            header_start: entry.offset_from_start.get() as u64,
            content_len: (!has_data_descriptor || size != 0).then_some(size),
            disk_id: entry.disk_number.get() as u32,
        }
    }
}

impl<M> File<M, ()> {
    pub fn in_disk<D>(
        self,
        disk: crate::DirectoryLocator<D>,
    ) -> Result<File<M, D>, error::DiskMismatch> {
        (self.locator.disk_id == disk.descriptor.disk_id())
            .then(move || self.assume_in_disk(disk.disk))
            .ok_or(error::DiskMismatch(()))
    }
    pub fn assume_in_disk<D>(self, disk: D) -> File<M, D> {
        File {
            disk,
            meta: self.meta,
            locator: self.locator,
        }
    }
}
#[cfg(feature = "std")]
impl<D: std::io::Read + std::io::Seek, M> File<M, D> {
    /// Build an extractor for the data stored in this file.
    ///
    /// ## Errors
    ///
    /// If the file uses a [`zip_format::CompressionMethod`] that isn't enabled in the
    /// crate features, a [`MethodNotSupported`] is returned instead.
    pub fn reader(self) -> std::io::Result<ReadBuilder<D>> {
        ReadBuilder::new(self.disk, self.locator)
    }

    /// ```no_run
    /// # fn main() -> std::io::Result<()> {
    /// # let archive = zip::Archive::open_at("").unwrap();
    /// # let file = archive.into_iter().next().unwrap();
    /// # let mut decompressor = Default::default();
    /// let reader = file
    ///     .reader_with_decryption()?
    ///     .or_else(|d| d.try_password(b"password"))?
    ///     .build_with_buffering(&mut decompressor, std::io::BufReader::new);
    /// # Ok(())
    /// # }
    /// ```
    pub fn reader_with_decryption(
        self,
    ) -> std::io::Result<Result<NotEncrypted<D>, DecryptBuilder<D>>> {
        self.reader()?.remove_encryption_io()
    }
}

impl<M, D> core::ops::Deref for File<M, D> {
    type Target = M;
    fn deref(&self) -> &Self::Target {
        &self.meta
    }
}
