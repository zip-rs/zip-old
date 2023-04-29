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

pub struct FileLocator {
    storage: Option<read::FileStorage>,
    disk_id: u32,
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
            unknown_size: flags & 0b100 != 0,
            crc32: entry.crc32.get(),
            len: entry.compressed_size.get() as u64,
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
        Ok(ReadBuilder::new(self.disk, self.locator)?)
    }
}

impl<M, D> core::ops::Deref for File<M, D> {
    type Target = M;
    fn deref(&self) -> &Self::Target {
        &self.meta
    }
}
