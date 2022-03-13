use crate::{error, file, metadata};

use std::io;

pub struct Footer<D> {
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
impl<D: io::Read + io::Seek> Footer<D> {
    pub fn from_io(mut disk: D) -> io::Result<Self> {
        // TODO: optimize this
        let mut buf = vec![];
        let n = disk.seek(std::io::SeekFrom::End(0))?;
        let offset = disk.seek(std::io::SeekFrom::Start(n.saturating_sub(
            64 * 1024 + core::mem::size_of::<zip_format::Footer>() as u64 + 4 - 2,
        )))?;
        disk.read_to_end(&mut buf)?;
        Ok(Footer::from_buf_at_offset(&buf, offset)?.with_disk(disk))
    }
}
impl<'a> Footer<&'a [u8]> {
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
impl<D> Footer<D> {
    pub fn into_directory(self) -> Result<Directory<D>, error::DiskMismatch> {
        (self.descriptor.directory_location_disk == self.descriptor.disk_id())
            .then(|| Directory {
                disk: self.disk,
                span: DirectorySpan {
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
pub struct Directory<D> {
    pub disk: D,
    pub span: DirectorySpan,
}
pub struct DirectorySpan {
    offset: u64,
    entries: u64,
}
impl<D: io::Seek + io::Read> Directory<D> {
    /// It is highly recommended to use a buffered disk for this operation
    pub fn seek_to_files<M: metadata::Metadata<D>>(
        mut self,
    ) -> io::Result<impl ExactSizeIterator<Item = io::Result<file::File<M>>>> {
        self.disk
            .seek(std::io::SeekFrom::Start(self.span.offset as u64))?;
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
// TODO: Design an API for reading metadata from an entry
impl<
        D: io::Read + io::Seek,
        M: for<'a> TryFrom<(&'a zip_format::DirectoryEntry, &'a mut D), Error = io::Error>,
    > Iterator for DirectoryIter<M, D>
{
    type Item = io::Result<file::File<M>>;
    fn next(&mut self) -> Option<Self::Item> {
        self.entries.checked_sub(1).map(|rem| {
            self.entries = rem;
            let mut buf = [0; core::mem::size_of::<zip_format::DirectoryEntry>() + 4];
            self.disk.read_exact(&mut buf)?;
            let entry =
                zip_format::DirectoryEntry::as_prefix(&buf).ok_or(error::NotAnArchive(()))?;
            let storage_kind = match entry.method {
                zip_format::CompressionMethod::STORED => Some(crate::file::FileStorageKind::Stored),
                #[cfg(feature = "read-deflate")]
                zip_format::CompressionMethod::DEFLATE => Some(if entry.flags.get() & 0b100 != 0 {
                    crate::file::FileStorageKind::Deflated
                } else {
                    crate::file::FileStorageKind::LimitDeflated
                }),
                _ => None,
            };
            let storage = storage_kind.map(|kind| crate::file::FileStorage {
                kind,
                len: entry.compressed_size.get() as u64,
                start: entry.offset_from_start.get() as u64,
            });
            Ok(file::File {
                disk: (),
                meta: M::try_from((entry, &mut self.disk))?,
                locator: file::FileLocator {
                    storage,
                    disk_id: entry.disk_number.get() as _,
                },
            })
        })
    }
}
