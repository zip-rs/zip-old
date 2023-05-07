use crate::{error, file, metadata};

#[cfg(feature = "std")]
use std::io;
#[cfg(feature = "std")]
pub struct Archive<M = metadata::std::Full, D = std::fs::File> {
    disks: Vec<D>,
    files: Vec<file::File<M, ()>>,
}
#[cfg(feature = "std")]
/// # Warning
///
/// This can be used to iterate through an [`Archive<std::fs::File>`](Archive),
/// but you must take care to only use the [`file::File::reader`] method on
/// one file at a time. This is because each file will be located in a shared [`std::fs::File`].
impl<'a, M, D: 'a> IntoIterator for &'a Archive<M, D> {
    type IntoIter = Iter<'a, M, D>;
    type Item = file::File<&'a M, &'a D>;
    fn into_iter(self) -> Self::IntoIter {
        Iter(&self.disks, self.files.iter())
    }
}
#[cfg(feature = "std")]
pub struct Iter<'a, M, D>(&'a [D], core::slice::Iter<'a, file::File<M, ()>>);
#[cfg(feature = "std")]
impl<'a, M, D> Iterator for Iter<'a, M, D> {
    type Item = file::File<&'a M, &'a D>;
    fn next(&mut self) -> Option<Self::Item> {
        self.1.next().map(|file| file::File {
            disk: &self.0[file.locator.disk_id as usize],
            meta: &file.meta,
            locator: file.locator.clone(),
        })
    }
}
#[cfg(feature = "std")]
impl Archive<metadata::std::Full> {
    pub fn open_at(path: impl AsRef<std::path::Path>) -> io::Result<Self> {
        let mut last_disk = DirectoryLocator::from_io(std::fs::File::open(path)?)?;
        assert_eq!(last_disk.descriptor.disk_id(), 0);
        let files = last_disk
            .as_mut()
            .into_directory()?
            .seek_to_files()?
            .collect::<Result<_, _>>()?;
        Ok(Self {
            files,
            disks: vec![last_disk.disk],
        })
    }
}
impl<M, D> Archive<M, D> {
    pub fn try_map_disks<D2, E>(self, f: impl FnMut(D) -> Result<D2, E>) -> Result<Archive<M, D2>, E> {
        Ok(Archive {
            disks: self.disks.into_iter().map(f).collect::<Result<_, _>>()?,
            files: self.files,
        })
    }
}
pub struct DirectoryLocator<D> {
    pub disk: D,
    pub descriptor: DiskDescriptor,
}
#[derive(Copy, Clone)]
pub struct DiskDescriptor {
    disk_id: u32,
    directory_location_disk: u32,
    directory_location_offset: u64,
    directory_entries: u64,
}
impl DiskDescriptor {
    pub fn disk_id(&self) -> u32 {
        self.disk_id
    }
}
#[cfg(feature = "std")]
impl<D: io::Read + io::Seek> DirectoryLocator<D> {
    pub fn from_io(mut disk: D) -> io::Result<Self> {
        // TODO: optimize this
        let mut buf = vec![];
        let n = disk.seek(std::io::SeekFrom::End(0))?;
        let offset = disk.seek(std::io::SeekFrom::Start(n.saturating_sub(
            64 * 1024 + core::mem::size_of::<zip_format::Footer>() as u64 + 4 - 2,
        )))?;
        disk.read_to_end(&mut buf)?;
        Ok(DirectoryLocator::from_buf_at_offset(&buf, offset)?.with_disk(disk))
    }
}
impl<'a> DirectoryLocator<&'a [u8]> {
    /// Load a zip central directory from a buffer
    pub fn from_buf(disk: &'a [u8]) -> Result<Self, error::NotAnArchive> {
        Self::from_buf_at_offset(disk, 0)
    }
    pub fn from_buf_at_offset(disk: &'a [u8], offset: u64) -> Result<Self, error::NotAnArchive> {
        disk.windows(2)
            .rev()
            .take(u16::MAX as _)
            .map(|w| u16::from_le_bytes([w[0], w[1]]))
            .enumerate()
            .filter(|(i, n)| *n as usize == *i)
            .find_map(|(i, _)| {
                zip_format::Footer::as_suffix(&disk[..disk.len() - i]).zip(Some(disk.len() - i))
            })
            .and_then(|(footer, i)| {
                Some(Self {
                    disk,
                    descriptor: i
                        .checked_sub(core::mem::size_of::<zip_format::Footer>() + 4)
                        .and_then(|i| zip_format::FooterLocator::as_suffix(&disk[..i]))
                        .map_or_else(
                            || {
                                Some(DiskDescriptor {
                                    disk_id: footer.disk_number.get() as _,
                                    directory_location_disk: footer.directory_start_disk.get() as _,
                                    directory_location_offset: footer.offset_from_start.get() as _,
                                    directory_entries: footer.entries.get() as _,
                                })
                            },
                            |locator| {
                                if locator.directory_start_disk.get()
                                    != footer.disk_number.get() as u32
                                {
                                    return None;
                                }

                                // FIXME: This will fail when `from_io` is called on an archive with a large comment
                                let offset =
                                    locator.footer_offset.get().checked_sub(offset)? as usize;
                                let footer = zip_format::FooterV2::as_prefix(&disk[offset..])?;
                                Some(DiskDescriptor {
                                    disk_id: footer.disk_number.get(),
                                    directory_location_disk: footer.directory_start_disk.get(),
                                    directory_location_offset: footer.offset_from_start.get(),
                                    directory_entries: footer.entries.get(),
                                })
                            },
                        )?,
                })
            })
            .ok_or(error::NotAnArchive(()))
    }
}
impl<D> DirectoryLocator<D> {
    pub fn into_directory(self) -> Result<Directory<D>, error::DiskMismatch> {
        (self.descriptor.directory_location_disk == self.descriptor.disk_id())
            .then_some(Directory {
                disk: self.disk,
                span: DirectorySpan {
                    offset: self.descriptor.directory_location_offset,
                    entries: self.descriptor.directory_entries,
                },
            })
            .ok_or(error::DiskMismatch(()))
    }
}
impl<D> DirectoryLocator<D> {
    pub fn with_disk<U>(&self, disk: U) -> DirectoryLocator<U> {
        DirectoryLocator {
            disk,
            descriptor: self.descriptor,
        }
    }
    pub fn as_mut(&mut self) -> DirectoryLocator<&mut D> {
        DirectoryLocator {
            disk: &mut self.disk,
            descriptor: self.descriptor,
        }
    }
}
pub struct Directory<D> {
    pub disk: D,
    pub span: DirectorySpan,
}
pub struct DirectorySpan {
    offset: u64,
    entries: u64,
}
impl<'a> Directory<&'a [u8]> {
    pub fn iter(
        self,
    ) -> impl Iterator<
        Item = Result<file::File<metadata::RawDirectoryEntry<'a>, &'a [u8]>, error::NotAnArchive>,
    > {
        let Directory { disk, span } = self;
        // FIXME: checked conversions
        let mut offset = span.offset as usize;
        let mut entries = span.entries;
        core::iter::from_fn(move || {
            (entries != 0).then(|| {
                let entry = zip_format::DirectoryEntry::as_prefix(&disk[offset..])
                    .ok_or(error::NotAnArchive(()))?;
                offset += core::mem::size_of::<zip_format::DirectoryEntry>() + 4;
                let size = entry.name_len.get() as usize
                    + entry.metadata_len.get() as usize
                    + entry.comment_len.get() as usize;
                let metadata = &disk[offset..offset + size];
                offset += size;
                entries -= 1;
                Ok(file::File {
                    disk,
                    meta: metadata::RawDirectoryEntry::new(entry, metadata),
                    locator: file::FileLocator::from_entry(entry),
                })
            })
        })
    }
}
#[cfg(feature = "std")]
impl<D: io::Seek + io::Read> Directory<D> {
    /// It is highly recommended to use a buffered disk for this operation
    pub fn seek_to_files<M: metadata::Metadata<D>>(
        mut self,
    ) -> io::Result<impl ExactSizeIterator<Item = io::Result<file::File<M>>>>
    where
        M::Error: Into<io::Error>,
    {
        self.disk.seek(std::io::SeekFrom::Start(self.span.offset))?;
        Ok(DirectoryIter {
            disk: self.disk,
            entries: self.span.entries,
            metadata_parser: core::marker::PhantomData,
        })
    }
}
struct DirectoryIter<M, D> {
    disk: D,
    entries: u64,
    metadata_parser: core::marker::PhantomData<fn() -> M>,
}
impl<M, D> ExactSizeIterator for DirectoryIter<M, D>
where
    Self: Iterator,
{
    fn len(&self) -> usize {
        self.entries as usize
    }
}
#[cfg(feature = "std")]
// TODO: Design an API for reading metadata from an entry
impl<D: io::Read + io::Seek, M: metadata::Metadata<D>> Iterator for DirectoryIter<M, D>
where
    M::Error: Into<std::io::Error>,
{
    type Item = io::Result<file::File<M>>;
    fn next(&mut self) -> Option<Self::Item> {
        self.entries.checked_sub(1).map(|rem| {
            self.entries = rem;
            let mut buf = [0; core::mem::size_of::<zip_format::DirectoryEntry>() + 4];
            self.disk.read_exact(&mut buf)?;
            let entry =
                zip_format::DirectoryEntry::as_prefix(&buf).ok_or(error::NotAnArchive(()))?;
            Ok(file::File {
                disk: (),
                meta: M::from_entry(entry, &mut self.disk).map_err(|e| {
                    let e: io::Error = e.into();
                    e
                })?,
                locator: file::FileLocator::from_entry(entry),
            })
        })
    }
}
