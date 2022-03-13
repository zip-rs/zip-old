use crate::error::{self, MethodNotSupported};

mod read;
pub use read::*;

pub struct File<M = crate::metadata::Full, D = ()> {
    pub disk: D,
    pub meta: M,
    pub locator: FileLocator,
}

pub struct FileLocator {
    pub(crate) storage: Option<read::FileStorage>,
    pub(crate) disk_id: u32,
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
