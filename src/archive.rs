use crate::{error, file};

use std::io;

pub struct Footer<D> {
    pub disk: D,
    pub descriptor: DiskDescriptor,
}
#[derive(Copy, Clone)]
pub struct DiskDescriptor {
    disk_id: u16,
    directory_location_disk: u16,
    directory_location_offset: u32,
    directory_entries: u16,
}
impl DiskDescriptor {
    pub fn disk_id(&self) -> u16 {
        self.disk_id
    }
}
impl<D: io::Read + io::Seek> Footer<D> {
    pub fn from_io(mut disk: D) -> io::Result<Self> {
        // TODO: optimize this
        let mut buf = vec![];
        let n = disk.seek(std::io::SeekFrom::End(0))?;
        disk.seek(std::io::SeekFrom::Start(n.saturating_sub(64 * 1024)))?;
        disk.read_to_end(&mut buf)?;
        Ok(Footer::from_buf(&buf).map(move |v| v.with_disk(disk))?)
    }
}
impl<'a> Footer<&'a [u8]> {
    /// Load a zip central directory from a buffer
    pub fn from_buf(disk: &'a [u8]) -> Result<Self, error::NotAnArchive> {
        disk.windows(2)
            .rev()
            .take(u16::MAX as _)
            .map(|w| u16::from_le_bytes([w[0], w[1]]))
            .enumerate()
            .filter(|(i, n)| *n as usize == *i)
            .find_map(|(i, _)| zip_format::Footer::as_suffix(&disk[..disk.len() - i]))
            .map(|footer| Self {
                disk,
                descriptor: DiskDescriptor {
                    disk_id: footer.disk_number.get(),
                    directory_location_disk: footer.directory_start_disk.get(),
                    directory_location_offset: footer.offset_from_start.get(),
                    directory_entries: footer.entries.get(),
                },
            })
            .ok_or(error::NotAnArchive(()))
    }
}
impl<D> Footer<D> {
    pub fn into_directory(self) -> Result<crate::Persisted<Directory, D>, error::DiskMismatch> {
        (self.descriptor.directory_location_disk == self.descriptor.disk_id())
            .then(|| crate::Persisted {
                disk: self.disk,
                structure: Directory {
                    offset: self.descriptor.directory_location_offset,
                    entries: self.descriptor.directory_entries,
                },
            })
            .ok_or(error::DiskMismatch(()))
    }
}
impl<D> Footer<D> {
    pub fn with_disk<U>(&self, disk: U) -> Footer<U> {
        Footer {
            disk,
            descriptor: self.descriptor.clone(),
        }
    }
    pub fn as_mut(&mut self) -> Footer<&mut D> {
        Footer {
            disk: &mut self.disk,
            descriptor: self.descriptor.clone(),
        }
    }
}
pub struct Directory {
    offset: u32,
    entries: u16,
}
impl<D: io::Seek + io::Read> crate::Persisted<Directory, D> {
    /// It is highly recommended to use a buffered disk for this operation
    pub fn seek_to_files<M>(mut self) -> io::Result<impl Iterator<Item = io::Result<file::File<M>>>>
    where
        M: for<'a> TryFrom<(&'a zip_format::DirectoryEntry, &'a mut D), Error = io::Error>,
    {
        self.disk
            .seek(std::io::SeekFrom::Start(self.structure.offset as u64))?;
        Ok(self.map(|dir| DirectoryIter {
            entries: dir.entries,
            metadata_parser: core::marker::PhantomData,
        }))
    }
}
pub struct DirectoryIter<M> {
    entries: u16,
    metadata_parser: core::marker::PhantomData<fn() -> M>,
}
// TODO: Design an API for reading metadata from an entry
impl<
        D: io::Read + io::Seek,
        M: for<'a> TryFrom<(&'a zip_format::DirectoryEntry, &'a mut D), Error = io::Error>,
    > Iterator for crate::Persisted<DirectoryIter<M>, D>
{
    type Item = io::Result<file::File<M>>;
    fn next(&mut self) -> Option<Self::Item> {
        self.structure.entries.checked_sub(1).map(|rem| {
            self.structure.entries = rem;
            let mut buf = [0; core::mem::size_of::<zip_format::DirectoryEntry>() + 4];
            self.disk.read_exact(&mut buf)?;
            let entry =
                zip_format::DirectoryEntry::as_prefix(&buf).ok_or(error::NotAnArchive(()))?;

            Ok(file::File {
                meta: M::try_from((entry, &mut self.disk))?,
                header: file::FileHeader::new(
                    entry.offset_from_start.get() as u64,
                    entry.compressed_size.get() as u64,
                    entry.method,
                    entry.disk_number.get(),
                ),
            })
        })
    }
}
