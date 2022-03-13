use crate::error;
use std::io;

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
    deflate: Option<Box<(miniz_oxide::inflate::core::DecompressorOxide, InflateBuffer)>>,
}

pub struct ReadBuilder<'a, D> {
    disk: D,
    imp: ReadImpl<'a, (), ()>,
    start: u64,
}

enum ReadImpl<'a, D, Buffered> {
    Never {
        never: core::convert::Infallible,
        b: Buffered,
        stored: &'a mut (),
    },
    Stored {
        disk: D,
        remaining: u64,
    },
    #[cfg(feature = "read-deflate")]
    Deflate {
        disk: Buffered,
        remaining: u64,
        decompressor: &'a mut (miniz_oxide::inflate::core::DecompressorOxide, InflateBuffer),
        out_pos: u16,
        read_cursor: u16,
    },
}

impl<'a, D> ReadBuilder<'a, D> {
    pub(super) fn new(
        disk: D,
        header: super::FileLocator,
        st: &'a mut Store,
    ) -> Result<Self, error::MethodNotSupported> {
        let make_deflate = move |remaining, st: &'a mut Store| ReadImpl::Deflate {
            disk: (),
            remaining,
            decompressor: {
                let deflate = st.deflate.get_or_insert_with(Default::default);
                deflate.0.init();
                deflate
            },
            out_pos: 0,
            read_cursor: 0,
        };
        Ok(Self {
            disk,
            start: header.start,
            imp: match header.storage {
                super::FileStorage::Stored(len) => ReadImpl::Stored {
                    disk: (),
                    remaining: len,
                },
                #[cfg(feature = "read-deflate")]
                super::FileStorage::Deflated => make_deflate(u64::MAX, st),
                #[cfg(feature = "read-deflate")]
                super::FileStorage::LimitDeflated(n) => make_deflate(n, st),
                _ => return Err(error::MethodNotSupported(())),
            },
        })
    }
}

impl<'a, D: io::Seek + io::Read> ReadBuilder<'a, D> {
    pub fn seek_to_data<Buffered>(
        mut self,
        into_buffered: impl FnOnce(D) -> Buffered,
    ) -> io::Result<Read<'a, D, Buffered>> {
        // TODO: avoid seeking if we can, since this will often be done in a loop
        self.disk.seek(std::io::SeekFrom::Start(self.start))?;
        let mut buf = [0; std::mem::size_of::<zip_format::Header>() + 4];
        self.disk.read_exact(&mut buf)?;
        let header = zip_format::Header::as_prefix(&buf).ok_or(error::NotAnArchive(()))?;
        self.disk.seek(std::io::SeekFrom::Current(
            header.name_len.get() as i64 + header.metadata_len.get() as i64,
        ))?;
        Ok(Read(match self.imp {
            ReadImpl::Never { never, .. } => match never {},
            ReadImpl::Stored {
                remaining,
                disk: (),
            } => ReadImpl::Stored {
                remaining,
                disk: self.disk,
            },
            #[cfg(feature = "read-deflate")]
            ReadImpl::Deflate {
                remaining,
                decompressor,
                out_pos,
                read_cursor,
                disk: (),
            } => ReadImpl::Deflate {
                disk: into_buffered(self.disk),
                remaining,
                decompressor,
                out_pos,
                read_cursor,
            },
        }))
    }
}

impl<D: io::Read, Buffered: io::BufRead> io::Read for Read<'_, D, Buffered> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.0 {
            ReadImpl::Never { never, .. } => match *never {},
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
