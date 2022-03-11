use crate::error;
use miniz_oxide::inflate::core as inflate;
use std::io;

pub struct File<M> {
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

impl<M> File<M> {
    pub fn in_disk<D>(
        self,
        disk: crate::Persisted<crate::Footer, D>,
    ) -> Result<crate::Persisted<File<M>, D>, error::DiskMismatch> {
        (self.header.disk_id == disk.structure.disk_id)
            .then(move || disk.map(move |_| self))
            .ok_or(error::DiskMismatch(()))
    }
}
impl<D, M> crate::Persisted<File<M>, D> {
    pub fn into_reader(
        self,
        decompressor: &mut Decompressor,
    ) -> Result<crate::Persisted<Reader<'_>, D>, error::MethodNotSupported> {
        Ok(crate::Persisted {
            disk: self.disk,
            structure: Reader {
                start: self.structure.header.start,
                imp: match self.structure.header.method {
                    zip_format::CompressionMethod::STORED => DataImpl::Stored {
                        disk: (),
                        remaining: self.structure.header.len,
                    },
                    zip_format::CompressionMethod::DEFLATE => DataImpl::Deflate {
                        disk: (),
                        remaining: self.structure.header.len,
                        decompressor: &mut decompressor.deflate,
                        out_pos: 0,
                        read_cursor: 0,
                    },
                    _ => return Err(error::MethodNotSupported(())),
                },
            },
        })
    }
}

impl<M> core::ops::Deref for File<M> {
    type Target = M;
    fn deref(&self) -> &Self::Target {
        &self.meta
    }
}

struct InflateBuffer([u8; 32 * 1024]);
impl Default for InflateBuffer {
    fn default() -> Self {
        Self([0; 32 * 1024])
    }
}
#[derive(Default)]
pub struct Decompressor {
    deflate: Box<(inflate::DecompressorOxide, InflateBuffer)>,
}

pub struct Reader<'a> {
    imp: DataImpl<'a, (), ()>,
    start: u64,
}
impl<'a, D: io::Seek + io::Read> crate::Persisted<Reader<'a>, D> {
    pub fn seek_to_data<Buffered>(
        mut self,
        into_buffered: impl FnOnce(D) -> Buffered,
    ) -> io::Result<Data<'a, D, Buffered>> {
        // TODO: avoid seeking if we can, since this will often be done in a loop
        self.disk
            .seek(std::io::SeekFrom::Start(self.structure.start))?;
        let mut buf = [0; std::mem::size_of::<zip_format::Header>() + 4];
        self.disk.read_exact(&mut buf)?;
        let header = zip_format::Header::as_prefix(&buf).ok_or(error::NotAnArchive(()))?;
        self.disk.seek(std::io::SeekFrom::Current(
            header.name_len.get() as i64 + header.metadata_len.get() as i64,
        ))?;
        Ok(Data(match self.structure.imp {
            DataImpl::Stored { remaining, .. } => DataImpl::Stored {
                remaining,
                disk: self.disk,
            },
            DataImpl::Deflate {
                remaining,
                decompressor,
                out_pos,
                read_cursor,
                ..
            } => DataImpl::Deflate {
                remaining,
                decompressor,
                disk: into_buffered(self.disk),
                out_pos,
                read_cursor,
            },
        }))
    }
}
enum DataImpl<'a, D, Buffered> {
    Stored {
        disk: D,
        remaining: u64,
    },
    Deflate {
        disk: Buffered,
        remaining: u64,
        decompressor: &'a mut (miniz_oxide::inflate::core::DecompressorOxide, InflateBuffer),
        out_pos: u16,
        read_cursor: u16,
    },
}
pub struct Data<'a, D, Buffered = D>(DataImpl<'a, D, Buffered>);
impl<D: io::Read, Buffered: io::BufRead> io::Read for Data<'_, D, Buffered> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.0 {
            DataImpl::Deflate {
                disk,
                decompressor,
                out_pos,
                read_cursor,
                ..
            } => {
                // TODO: track `remaining`
                // TODO: Check the CRC of the decompressed data
                let decompressed = &mut (decompressor.1).0;
                let mut out = *out_pos as usize;
                let mut cursor = *read_cursor as usize;
                if read_cursor == out_pos {
                    let data = disk.fill_buf()?;
                    if out == decompressed.len() {
                        out = 0;
                    }
                    let (_status, consumed, written) = miniz_oxide::inflate::core::decompress(
                        &mut decompressor.0,
                        data,
                        decompressed,
                        out,
                        0,
                    );
                    out += written;
                    disk.consume(consumed);
                }
                if cursor == decompressed.len() {
                    cursor = 0;
                }
                let len = out.checked_sub(cursor).unwrap().min(buf.len());
                buf[..len].copy_from_slice(&decompressed[cursor..len]);
                Ok(len)
            }
            DataImpl::Stored { disk, remaining } => {
                let n = disk.take(*remaining).read(buf)?;
                *remaining -= n as u64;
                Ok(n)
            }
        }
    }
}
