use crate::error::{self, MethodNotSupported};

mod read;
pub use read::*;

#[cfg(not(feature = "std"))]
type Default = ();
#[cfg(feature = "std")]
type Default = crate::metadata::std::Full;
pub struct File<M = Default, D = ()> {
    pub disk: D,
    pub meta: M,
    pub locator: FileLocator,
}
#[derive(Clone)]
pub struct FileLocator {
    storage: Option<read::FileStorage>,
    pub(crate) disk_id: u32,
}
impl FileLocator {
    pub(crate) fn from_entry(entry: &zip_format::DirectoryEntry) -> Self {
        let storage_kind = match entry.method {
            zip_format::CompressionMethod::STORED => Some(crate::file::FileStorageKind::Stored),
            #[cfg(feature = "read-deflate")]
            zip_format::CompressionMethod::DEFLATE => {
                Some(crate::file::FileStorageKind::Deflated)
            }
            _ => None,
        };
        let flags = entry.flags.get();
        let storage = storage_kind.map(|kind| crate::file::FileStorage {
            kind,
            encrypted: flags & 0b1 != 0,
            crc32: entry.crc32.get(),
            len: (flags & 0b1000 != 0).then(|| entry.compressed_size.get() as u64),
            start: entry.offset_from_start.get() as u64,
        });
        Self {
            storage,
            disk_id: entry.disk_number.get() as u32,
        }
    }
}

impl<M> File<M, ()> {
    pub fn in_disk<D>(self, disk: crate::Footer<D>) -> Result<File<M, D>, error::DiskMismatch> {
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
impl<D, M> File<M, D> {
    /// Build an extractor for the data stored in this file.
    ///
    /// ## Errors
    ///
    /// If the file uses a [`zip_format::CompressionMethod`] that isn't enabled in the
    /// crate features, a [`MethodNotSupported`] is returned instead.
    pub fn reader(self) -> Result<ReadBuilder<D>, MethodNotSupported> {
        ReadBuilder::new(self.disk, self.locator)
    }
}
#[cfg(feature = "std")]
impl<D: std::io::Read + std::io::Seek, M> File<M, D> {
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
    pub fn reader_with_decryption(self) -> std::io::Result<Result<ReadBuilder<Decrypt<D>, read::Found, read::Decrypted>, read::DecryptBuilder<D>>> {
        Ok(self
            .reader()?
            .seek_to_data()?
            .remove_encryption_io()?
            .map(|read| read.map_disk(read::Decrypt::from_unlocked)))
    }
}

impl<M, D> core::ops::Deref for File<M, D> {
    type Target = M;
    fn deref(&self) -> &Self::Target {
        &self.meta
    }
}
