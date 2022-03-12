use crate::error::{self, MethodNotSupported};

mod data;
pub use data::{Read, ReadBuilder, Store};

pub struct File<M = crate::metadata::Full, D = ()> {
    pub disk: D,
    pub meta: M,
    pub locator: FileLocator,
}

pub struct FileLocator {
    start: u64,
    storage: FileStorage,
    disk_id: u32,
}
pub(crate) enum FileStorage {
    Stored(u64),
    Deflated,
    LimitDeflated(u64),
    Unknown
}
impl FileLocator {
    pub(crate) fn new(
        start: u64,
        storage: FileStorage,
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
