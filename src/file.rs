use crate::error;

mod data;
pub use data::{Decompressor, Reader, ReaderBuilder};

pub struct File<M = crate::metadata::Full, D = ()> {
    pub disk: D,
    pub meta: M,
    pub header: FileHeader,
}

pub struct FileHeader {
    start: u64,
    len: u64,
    method: zip_format::CompressionMethod,
    disk_id: u16,
}
impl FileHeader {
    pub(crate) fn new(
        start: u64,
        len: u64,
        method: zip_format::CompressionMethod,
        disk_id: u16,
    ) -> Self {
        Self {
            start,
            len,
            method,
            disk_id,
        }
    }
}

impl<M> File<M, ()> {
    pub fn in_disk<D>(self, disk: crate::Footer<D>) -> Result<File<M, D>, error::DiskMismatch> {
        (self.header.disk_id == disk.descriptor.disk_id())
            .then(move || self.assume_disk(disk.disk))
            .ok_or(error::DiskMismatch(()))
    }
    pub fn assume_disk<D>(self, disk: D) -> File<M, D> {
        File {
            disk,
            meta: self.meta,
            header: self.header,
        }
    }
}
impl<D, M> File<M, D> {
    pub fn reader(
        self,
        decompressor: &mut Decompressor,
    ) -> Result<ReaderBuilder<'_, D>, error::MethodNotSupported> {
        Ok(ReaderBuilder::new(self.disk, self.header, decompressor)?)
    }
}

impl<M, D> core::ops::Deref for File<M, D> {
    type Target = M;
    fn deref(&self) -> &Self::Target {
        &self.meta
    }
}
