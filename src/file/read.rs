use crate::error;
use core::marker::PhantomData;
#[cfg(feature = "std")]
use std::io;

// we're imported by `file.rs` using a path attribute, so our imports are wonky
#[path = "read/decrypt.rs"]
mod decrypt;
pub use decrypt::{Decrypt, DecryptBuilder};

/// An [`io::Read`] implementation for files stored in a ZIP archive.
///
/// Can be created using [`super::File::reader`]
pub struct Read<'a, D, Buffered = D>(ReadImpl<'a, D, Buffered>);

/// A cache for the state of [`Read`]s
///
/// When a [`super::File`] is being extracted, it uses large buffers to keep
/// track of the decompression dictionary.
///
/// To avoid the cost of initializing these buffers, you can preallocate them
/// with [`Store::default`] and feed it into [`super::File::reader`] when
/// extracting your files.
#[derive(Default)]
pub struct Store {
    #[cfg(feature = "read-deflate")]
    deflate: Option<(miniz_oxide::inflate::core::DecompressorOxide, InflateBuffer)>,
}
#[derive(Debug)]
pub struct Decrypted(());
#[derive(Debug)]
pub struct Found(());
#[derive(Debug)]
pub struct Not<T>(T);
#[derive(Debug)]
pub struct ReadBuilder<D, F = Not<Found>, E = Not<Decrypted>> {
    disk: D,
    storage: FileStorage,
    state: PhantomData<(F, E)>,
}

enum ReadImpl<'a, D, Buffered> {
    Stored {
        disk: D,
        remaining: u64,
    },
    #[cfg(feature = "read-deflate")]
    Deflate {
        disk: std::io::Take<Buffered>,
        decompressor: &'a mut (miniz_oxide::inflate::core::DecompressorOxide, InflateBuffer),
        out_pos: u16,
        read_cursor: u16,
    },
}
impl<D, F, E> ReadBuilder<D, F, E> {
    pub(super) fn map_disk<D2>(self, f: impl FnOnce(D) -> D2) -> ReadBuilder<D2, F, E>
    {
        ReadBuilder {
            disk: f(self.disk),
            storage: self.storage,
            state: self.state,
        }
    }
}
impl<D> ReadBuilder<D> {
    pub(super) fn new(
        disk: D,
        header: super::FileLocator,
    ) -> Result<Self, error::MethodNotSupported> {
        Ok(Self {
            disk,
            storage: header.storage.ok_or(error::MethodNotSupported(()))?,
            state: PhantomData,
        })
    }
}

impl<D, F> ReadBuilder<D, F, Not<Decrypted>> {
    fn assert_no_password(self) -> Result<ReadBuilder<D, F, Decrypted>, error::FileLocked> {
        (!self.storage.encrypted)
            .then(|| ReadBuilder {
                disk: self.disk,
                storage: self.storage,
                state: PhantomData,
            })
            .ok_or(error::FileLocked(()))
    }
}

#[cfg(feature = "std")]
impl<D: io::Seek + io::Read, E> ReadBuilder<D, Not<Found>, E> {
    pub fn seek_to_data(mut self) -> io::Result<ReadBuilder<D, Found, E>> {
        // TODO: avoid seeking if we can, since this will often be done in a loop
        // FIXME: should we be using the local header? This will disagree with most other tools
        //        really, we want a side channel for "nonfatal errors".
        self.disk
            .seek(std::io::SeekFrom::Start(self.storage.start))?;
        let mut buf = [0; std::mem::size_of::<zip_format::Header>() + 4];
        self.disk.read_exact(&mut buf)?;
        let header = zip_format::Header::as_prefix(&buf).ok_or(error::NotAnArchive(()))?;
        self.disk.seek(std::io::SeekFrom::Current(
            header.name_len.get() as i64 + header.metadata_len.get() as i64,
        ))?;
        Ok(ReadBuilder {
            disk: self.disk,
            storage: self.storage,
            state: PhantomData,
        })
    }
}
#[cfg(feature = "std")]
impl<D: io::Read> ReadBuilder<D, Found> {
    pub fn remove_encryption_io(self) -> io::Result<Result<ReadBuilder<D, Found, Decrypted>, DecryptBuilder<D>>> {
        if !self.storage.encrypted {
            Ok(Ok(ReadBuilder {
                disk: self.disk,
                storage: self.storage,
                state: PhantomData,
            }))
        } else {
            DecryptBuilder::from_io(self).map(Err)
        }
    }
}
impl<D> ReadBuilder<D, Found, Decrypted> {
    // FIXME: recommend self-reference for owning the store?
    #[cfg(feature = "std")]
    pub fn build_with_buffering<Buffered: std::io::Read>(
        self,
        store: &mut Store,
        f: impl FnOnce(D) -> Buffered,
    ) -> Read<'_, D, Buffered> {
        Read(match self.storage.kind {
            FileStorageKind::Stored => ReadImpl::Stored {
                remaining: self.storage.len.expect("todo: error handling when file is STORED and len is not present"),
                disk: self.disk,
            },
            #[cfg(feature = "read-deflate")]
            FileStorageKind::Deflated => ReadImpl::Deflate {
                disk: f(self.disk).take(self.storage.len.unwrap_or(u64::MAX)),
                decompressor: {
                    let deflate = store.deflate.get_or_insert_with(Default::default);
                    deflate.0.init();
                    deflate
                },
                out_pos: 0,
                read_cursor: 0,
            },
        })
    }
}

#[cfg(feature = "std")]
impl<D: io::Read, Buffered: io::BufRead> io::Read for Read<'_, D, Buffered> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.0 {
            ReadImpl::Stored { disk, remaining } => {
                let n = disk.take(*remaining).read(buf)?;
                *remaining -= n as u64;
                Ok(n)
            }
            #[cfg(feature = "read-deflate")]
            ReadImpl::Deflate {
                disk,
                decompressor,
                out_pos,
                read_cursor,
                ..
            } => {
                use std::io::BufRead;
                // TODO: track `remaining`
                // TODO: Check the CRC of the decompressed data
                let (decompressor, InflateBuffer(backbuf)) = decompressor;
                let mut out = *out_pos as usize;
                let mut cursor = *read_cursor as usize;
                if read_cursor == out_pos {
                    let data = disk.fill_buf()?;
                    if out == backbuf.len() {
                        out = 0;
                    }
                    let (_status, consumed, written) =
                        miniz_oxide::inflate::core::decompress(decompressor, data, backbuf, out, 0);
                    out += written;
                    disk.consume(consumed);
                }
                if cursor == backbuf.len() {
                    cursor = 0;
                }
                let len = out.checked_sub(cursor).unwrap().min(buf.len());
                buf[..len].copy_from_slice(&backbuf[cursor..][..len]);
                *read_cursor = (cursor + len) as u16;
                *out_pos = out as u16;
                Ok(len)
            }
        }
    }
}

struct InflateBuffer([u8; 32 * 1024]);
impl Default for InflateBuffer {
    fn default() -> Self {
        Self([0; 32 * 1024])
    }
}

/// We use an `Option<FileStorage>` in [`super::FileLocator`] to represent files that we might not be able to read.
#[derive(Debug, Clone)]
pub(crate) struct FileStorage {
    pub(crate) start: u64,
    pub(crate) len: Option<u64>,
    pub(crate) crc32: u32,
    pub(crate) encrypted: bool,
    pub(crate) kind: FileStorageKind,
}

/// List of available [`zip_format::CompressionMethod`] implementations.
#[derive(Debug, Clone)]
pub(crate) enum FileStorageKind {
    Stored,
    #[cfg(feature = "read-deflate")]
    Deflated,
}
