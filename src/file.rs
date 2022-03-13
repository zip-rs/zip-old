use crate::error::{self, MethodNotSupported};

mod read;
pub use read::{Read, ReadBuilder, Store};

pub struct File<M = crate::metadata::Full, D = ()> {
    pub disk: D,
    pub meta: M,
    pub locator: FileLocator,
}

pub struct FileLocator {
    start: u64,
    storage: Option<FileStorage>,
    disk_id: u32,
}
/// List of available [`zip_format::CompressionMethod`] implementations.
/// We use an `Option<FileStorage>` in [`FileLocator`] to represent files that we might not be able to read.
pub(crate) enum FileStorage {
    Stored(u64),
    #[cfg(feature = "read-deflate")]
    Deflated,
    #[cfg(feature = "read-deflate")]
    LimitDeflated(u64),
}
impl FileLocator {
    pub(crate) fn new(
        start: u64,
        storage: Option<FileStorage>,
        disk_id: u32,
    ) -> Self {
        Self {
            start,
            storage,
            disk_id,
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
    pub fn reader(self, st: &mut Store) -> Result<ReadBuilder<'_, D>, MethodNotSupported> {
        Ok(ReadBuilder::new(self.disk, self.locator, st)?)
    }
}

impl<M, D> core::ops::Deref for File<M, D> {
    type Target = M;
    fn deref(&self) -> &Self::Target {
        &self.meta
    }
}
