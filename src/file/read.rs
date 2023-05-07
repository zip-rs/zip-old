use crate::error;
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
pub struct MaybeEncrypted(Option<u8>);
#[derive(Debug)]
pub struct ReadBuilder<D, E = MaybeEncrypted> {
    disk: std::io::Take<D>,
    storage: FileStorageKind,
    encryption: E,
}

enum ReadImpl<'a, D, Buffered> {
    Stored(std::io::Take<D>),
    #[cfg(feature = "read-deflate")]
    Deflate {
        disk: std::io::Take<Buffered>,
        decompressor: &'a mut (miniz_oxide::inflate::core::DecompressorOxide, InflateBuffer),
        out_pos: u16,
        read_cursor: u16,
    },
}
impl<D, E> ReadBuilder<D, E> {
    pub(super) fn map_disk<D2: std::io::Read>(self, f: impl FnOnce(D) -> D2) -> ReadBuilder<D2, E>
    {
        let limit = self.disk.limit();
        ReadBuilder {
            disk: f(self.disk.into_inner()).take(limit),
            storage: self.storage,
            encryption: self.encryption,
        }
    }
}
impl<D: io::Read + io::Seek> ReadBuilder<D> {
    pub(super) fn new(
        mut disk: D,
        locator: super::FileLocator,
    ) -> io::Result<Self>
    {
        let mut buf = [0; std::mem::size_of::<zip_format::Header>() + 4];
        disk.seek(std::io::SeekFrom::Start(locator.header_start))?;
        disk.read_exact(&mut buf)?;
        let header = zip_format::Header::as_prefix(&buf).ok_or(error::NotAnArchive(()))?;
        let storage = match header.method {
            zip_format::CompressionMethod::STORED if locator.content_len.is_some()=> {
                crate::file::FileStorageKind::Stored
            },
            #[cfg(feature = "read-deflate")]
            zip_format::CompressionMethod::DEFLATE => {
                crate::file::FileStorageKind::Deflated
            }
            _ => return Err(error::MethodNotSupported(()).into()),
        };
        disk.seek(std::io::SeekFrom::Current(header.name_len.get() as i64 + header.metadata_len.get() as i64))?;
        Ok(Self {
            disk: disk.take(locator.content_len.unwrap_or(u64::MAX)),
            storage,
            encryption: MaybeEncrypted(locator.encrypted),
        })
    }
}

impl<D> ReadBuilder<D, MaybeEncrypted> {
    pub fn assert_no_password(self) -> Result<ReadBuilder<D, Decrypted>, error::FileLocked> {
        Ok(ReadBuilder {
            disk: self.disk,
            storage: self.storage,
            encryption: self.encryption.0.is_none().then(|| Decrypted(())).ok_or(error::FileLocked(()))?,
        })
    }
    #[cfg(feature = "std")]
    pub fn remove_encryption_io(self) -> io::Result<Result<ReadBuilder<Decrypt<D>, Decrypted>, DecryptBuilder<D>>>
    where D: io::Read {
        Ok(Ok(ReadBuilder {
            encryption: match self.encryption.0 {
                Some(expected_byte) => {
                    return DecryptBuilder::from_io(ReadBuilder {
                        disk: self.disk,
                        storage: self.storage,
                        encryption: expected_byte,
                    }).map(Err);
                }
                None => Decrypted(())
            },
            disk: self.disk,
            storage: self.storage,
        }.map_disk(Decrypt::from_unlocked)))
    }
}
impl<D> ReadBuilder<D, Decrypted> {
    // FIXME: recommend self-reference for owning the store?
    #[cfg(feature = "std")]
    pub fn build_with_buffering<Buffered: std::io::Read>(
        self,
        store: &mut Store,
        f: impl FnOnce(D) -> Buffered,
    ) -> Read<'_, D, Buffered> {
        let limit = self.disk.limit();
        Read(match self.storage {
            FileStorageKind::Stored => ReadImpl::Stored(self.disk),
            #[cfg(feature = "read-deflate")]
            FileStorageKind::Deflated => ReadImpl::Deflate {
                disk: f(self.disk.into_inner()).take(limit),
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
            ReadImpl::Stored(disk) => disk.read(buf),
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

/// List of available [`zip_format::CompressionMethod`] implementations.
#[derive(Debug, Clone)]
pub(crate) enum FileStorageKind {
    Stored,
    #[cfg(feature = "read-deflate")]
    Deflated,
}
