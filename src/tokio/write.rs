use crate::{
    compression::CompressionMethod,
    result::{ZipError, ZipResult},
    spec,
    tokio::{buf_writer::BufWriter, stream_impls::deflate, utils::map_swap_uninit, WrappedPin},
    types::{ZipFileData, DEFAULT_VERSION},
    write::{FileOptions, ZipRawValues, ZipWriterStats},
};

use tokio::io::{self, AsyncSeekExt, AsyncWriteExt};

use std::{
    cmp, mem, ops,
    pin::Pin,
    ptr,
    task::{ready, Context, Poll},
};

#[cfg(any(
    feature = "deflate",
    feature = "deflate-miniz",
    feature = "deflate-zlib"
))]
use flate2::{Compress, Compression};

pub struct StoredWriter<S>(Pin<Box<S>>);

impl<S> StoredWriter<S> {
    pub fn new(inner: Pin<Box<S>>) -> Self {
        Self(inner)
    }

    #[inline]
    fn pin_stream(self: Pin<&mut Self>) -> Pin<&mut S> {
        unsafe { self.get_unchecked_mut() }.0.as_mut()
    }
}

impl<S> WrappedPin<S> for StoredWriter<S> {
    fn unwrap_inner_pin(self) -> Pin<Box<S>> {
        self.0
    }
}

impl<S: io::AsyncWrite> io::AsyncWrite for StoredWriter<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.pin_stream().poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.pin_stream().poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.pin_stream().poll_shutdown(cx)
    }
}

pub struct DeflatedWriter<S>(deflate::Writer<Compress, BufWriter<S>>);

impl<S> DeflatedWriter<S> {
    #[inline]
    fn pin_stream(self: Pin<&mut Self>) -> Pin<&mut deflate::Writer<Compress, BufWriter<S>>> {
        unsafe { self.map_unchecked_mut(|Self(inner)| inner) }
    }
}

impl<S: io::AsyncWrite> DeflatedWriter<S> {
    pub fn new(compression: Compression, inner: Pin<Box<S>>) -> Self {
        let compress = Compress::new(compression, false);
        let buf_writer = BufWriter::new(inner);
        Self(deflate::Writer::with_state(compress, Box::pin(buf_writer)))
    }
}

impl<S> WrappedPin<S> for DeflatedWriter<S> {
    fn unwrap_inner_pin(self) -> Pin<Box<S>> {
        Pin::into_inner(self.0.unwrap_inner_pin()).unwrap_inner_pin()
    }
}

impl<S: io::AsyncWrite> io::AsyncWrite for DeflatedWriter<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.pin_stream().poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.pin_stream().poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.pin_stream().poll_shutdown(cx)
    }
}

pub enum ZipFileWrappedWriter<S> {
    Stored(StoredWriter<S>),
    Deflated(DeflatedWriter<S>),
}

enum WrappedProj<'a, S> {
    Stored(Pin<&'a mut StoredWriter<S>>),
    Deflated(Pin<&'a mut DeflatedWriter<S>>),
}

impl<S> ZipFileWrappedWriter<S> {
    #[inline]
    fn project(self: Pin<&mut Self>) -> WrappedProj<'_, S> {
        unsafe {
            let s = self.get_unchecked_mut();
            match s {
                Self::Stored(s) => WrappedProj::Stored(Pin::new_unchecked(s)),
                Self::Deflated(s) => WrappedProj::Deflated(Pin::new_unchecked(s)),
            }
        }
    }
}

impl<S: io::AsyncWrite> io::AsyncWrite for ZipFileWrappedWriter<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.project() {
            WrappedProj::Stored(w) => w.poll_write(cx, buf),
            WrappedProj::Deflated(w) => w.poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.project() {
            WrappedProj::Stored(w) => w.poll_flush(cx),
            WrappedProj::Deflated(w) => w.poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.project() {
            WrappedProj::Stored(w) => w.poll_shutdown(cx),
            WrappedProj::Deflated(w) => w.poll_shutdown(cx),
        }
    }
}

impl<S> WrappedPin<S> for ZipFileWrappedWriter<S> {
    fn unwrap_inner_pin(self) -> Pin<Box<S>> {
        match self {
            Self::Stored(s) => s.unwrap_inner_pin(),
            Self::Deflated(s) => s.unwrap_inner_pin(),
        }
    }
}

enum InnerWriter<S> {
    NoActiveFile(Pin<Box<S>>),
    FileWriter(ZipFileWrappedWriter<S>),
}

enum InnerProj<'a, S> {
    NoActiveFile(Pin<&'a mut S>),
    FileWriter(Pin<&'a mut ZipFileWrappedWriter<S>>),
}

impl<S> InnerWriter<S> {
    #[inline]
    pub(crate) fn project(self: Pin<&mut Self>) -> InnerProj<'_, S> {
        unsafe {
            let s = self.get_unchecked_mut();
            match s {
                Self::NoActiveFile(s) => InnerProj::NoActiveFile(s.as_mut()),
                Self::FileWriter(s) => InnerProj::FileWriter(Pin::new_unchecked(s)),
            }
        }
    }
}

enum WriterWrapResult {
    Stored,
    Deflated(Compression),
}

impl WriterWrapResult {
    pub(crate) fn wrap<S: io::AsyncWrite>(self, s: Pin<Box<S>>) -> ZipFileWrappedWriter<S> {
        match self {
            Self::Stored => ZipFileWrappedWriter::Stored(StoredWriter::new(s)),
            Self::Deflated(c) => ZipFileWrappedWriter::Deflated(DeflatedWriter::new(c, s)),
        }
    }
}

impl<S: io::AsyncWrite + io::AsyncSeek> InnerWriter<S> {
    pub(crate) async fn finalize(
        self: Pin<&mut Self>,
        files: &[ZipFileData],
        comment: &[u8],
    ) -> ZipResult<()> {
        match self.project() {
            InnerProj::FileWriter(_) => unreachable!("stream should be unwrapped!"),
            InnerProj::NoActiveFile(mut inner) => {
                let central_start = inner.stream_position().await?;
                for file in files.iter() {
                    write_spec::write_central_directory_header(inner.as_mut(), file).await?;
                }
                let central_size = inner.stream_position().await? - central_start;

                /* If we have to create a zip64 file, generate the appropriate additional footer. */
                if files.len() > spec::ZIP64_ENTRY_THR
                    || central_size.max(central_start) > spec::ZIP64_BYTES_THR
                {
                    let zip64_footer = spec::Zip64CentralDirectoryEnd {
                        version_made_by: DEFAULT_VERSION as u16,
                        version_needed_to_extract: DEFAULT_VERSION as u16,
                        disk_number: 0,
                        disk_with_central_directory: 0,
                        number_of_files_on_this_disk: files.len() as u64,
                        number_of_files: files.len() as u64,
                        central_directory_size: central_size,
                        central_directory_offset: central_start,
                    };
                    zip64_footer.write_async(inner.as_mut()).await?;

                    let zip64_footer = spec::Zip64CentralDirectoryEndLocator {
                        disk_with_central_directory: 0,
                        end_of_central_directory_offset: central_start + central_size,
                        number_of_disks: 1,
                    };
                    zip64_footer.write_async(inner.as_mut()).await?;
                }

                let number_of_files = files.len().min(spec::ZIP64_ENTRY_THR) as u16;
                let footer = spec::CentralDirectoryEnd {
                    disk_number: 0,
                    disk_with_central_directory: 0,
                    zip_file_comment: comment.to_vec(),
                    number_of_files_on_this_disk: number_of_files,
                    number_of_files,
                    central_directory_size: central_size.min(spec::ZIP64_BYTES_THR) as u32,
                    central_directory_offset: central_start.min(spec::ZIP64_BYTES_THR) as u32,
                };
                footer.write_async(inner.as_mut()).await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn initialize_entry(
        self: Pin<&mut Self>,
        raw: ZipRawValues,
        options: FileOptions,
        file_name: String,
        stats: &mut ZipWriterStats,
    ) -> ZipResult<ZipFileData> {
        match self.project() {
            InnerProj::FileWriter(_) => unreachable!("stream should be unwrapped!"),
            InnerProj::NoActiveFile(mut inner) => {
                let header_start = inner.stream_position().await?;

                let mut file = ZipFileData::initialize(raw, options, header_start, file_name);
                write_spec::write_local_file_header(inner.as_mut(), &file).await?;

                let header_end = inner.stream_position().await?;
                stats.start = header_end;
                *file.data_start.get_mut() = header_end;

                stats.bytes_written = 0;
                stats.hasher.reset();

                Ok(file)
            }
        }
    }

    pub(crate) async fn update_header_and_unwrap_stream(
        mut self: Pin<&mut Self>,
        stats: &mut ZipWriterStats,
        file: &mut ZipFileData,
    ) -> ZipResult<()> {
        match self.as_mut().project() {
            InnerProj::NoActiveFile(_) => unreachable!("expected this to be wrapped!"),
            InnerProj::FileWriter(mut inner) => {
                /* NB: we need to ensure the compression stream writes out everything it has left
                 * before reclaiming the stream handle! */
                inner.shutdown().await?;
            }
        }

        let s = self.get_mut();

        map_swap_uninit(s, |s| Self::NoActiveFile(s.unwrap_inner_pin()));

        match s {
            Self::NoActiveFile(ref mut inner) => {
                file.crc32 = mem::take(&mut stats.hasher).finalize();
                file.uncompressed_size = stats.bytes_written;

                let file_end = inner.stream_position().await?;
                file.compressed_size = file_end - stats.start;

                write_spec::update_local_file_header(inner.as_mut(), file).await?;
                inner.seek(io::SeekFrom::Start(file_end)).await?;
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    pub(crate) fn wrap_compressor_stream(
        self: Pin<&mut Self>,
        file: &ZipFileData,
    ) -> ZipResult<()> {
        let s = self.get_mut();
        let wrap_result = Self::try_wrap_writer(file.compression_method, file.compression_level)?;
        map_swap_uninit(s, move |s| match s {
            Self::FileWriter(_) => unreachable!("writer should be unwrapped!"),
            Self::NoActiveFile(s) => Self::FileWriter(wrap_result.wrap(s)),
        });
        Ok(())
    }

    pub(crate) async fn write_extra_data_and_wrap_compressor_stream(
        self: Pin<&mut Self>,
        stats: &mut ZipWriterStats,
        file: &mut ZipFileData,
    ) -> ZipResult<()> {
        let s = self.get_mut();
        match s {
            Self::FileWriter(_) => unreachable!("writer should be unwrapped!"),
            Self::NoActiveFile(inner) => {
                let data_start = file.data_start.get_mut();

                // Append extra data to local file header and keep it for central file header.
                inner.write_all(&file.extra_field).await?;

                // Update final `data_start`.
                let header_end = *data_start + file.extra_field.len() as u64;
                stats.start = header_end;
                *data_start = header_end;

                // Update extra field length in local file header.
                let extra_field_length =
                    if file.large_file { 20 } else { 0 } + file.extra_field.len() as u16;
                inner
                    .seek(io::SeekFrom::Start(file.header_start + 28))
                    .await?;
                inner.write_u16_le(extra_field_length).await?;
                inner.seek(io::SeekFrom::Start(header_end)).await?;
            }
        }

        Pin::new(s).wrap_compressor_stream(file)?;

        Ok(())
    }
}

impl<S: io::AsyncWrite> InnerWriter<S> {
    pub fn try_wrap_writer(
        compression: CompressionMethod,
        compression_level: Option<i32>,
    ) -> ZipResult<WriterWrapResult> {
        match compression {
            CompressionMethod::Stored => {
                if compression_level.is_some() {
                    return Err(ZipError::UnsupportedArchive(
                        "Unsupported compression level",
                    ));
                }
                Ok(WriterWrapResult::Stored)
            }
            CompressionMethod::Deflated => {
                let compression = Compression::new(
                    clamp_opt(
                        compression_level.unwrap_or(Compression::default().level() as i32),
                        deflate_compression_level_range(),
                    )
                    .ok_or(ZipError::UnsupportedArchive(
                        "Unsupported compression level",
                    ))? as u32,
                );
                Ok(WriterWrapResult::Deflated(compression))
            }
            _ => todo!("other compression methods not yet supported!"),
        }
    }
}

fn clamp_opt<T: cmp::Ord + Copy>(value: T, range: ops::RangeInclusive<T>) -> Option<T> {
    if range.contains(&value) {
        Some(value)
    } else {
        None
    }
}

fn deflate_compression_level_range() -> ops::RangeInclusive<i32> {
    let min = Compression::none().level() as i32;
    let max = Compression::best().level() as i32;
    min..=max
}

impl<S> WrappedPin<S> for InnerWriter<S> {
    fn unwrap_inner_pin(self) -> Pin<Box<S>> {
        match self {
            Self::NoActiveFile(s) => s,
            Self::FileWriter(s) => s.unwrap_inner_pin(),
        }
    }
}

/// To a [`Cursor`](std::io::Cursor):
///```
/// # fn main() -> zip::result::ZipResult<()> { tokio_test::block_on(async {
/// use zip::{write::FileOptions, tokio::{read::ZipArchive, write::ZipWriter}};
/// use std::{io::Cursor, pin::Pin};
/// use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
///
/// let buf = Cursor::new(Vec::new());
/// let mut f = ZipWriter::new(Box::pin(buf));
/// let mut fp = Pin::new(&mut f);
///
/// let opts = FileOptions::default();
/// fp.as_mut().start_file("asdf.txt", opts).await?;
/// fp.write_all(b"hello!").await?;
/// let buf = f.finish().await?;
///
/// let mut f = ZipArchive::new(buf).await?;
/// let mut f = Pin::new(&mut f);
/// let mut s = String::new();
/// {
///   let mut zf = f.by_name("asdf.txt").await?;
///   zf.read_to_string(&mut s).await?;
/// }
/// assert_eq!(&s, "hello!");
/// # Ok(())
/// # })}
///```
///
/// To a [`File`](tokio::fs::File):
///```
/// # fn main() -> zip::result::ZipResult<()> { tokio_test::block_on(async {
/// use zip::{write::FileOptions, tokio::{read::ZipArchive, write::ZipWriter}};
/// use std::pin::Pin;
/// use tokio::{fs, io::{self, AsyncReadExt, AsyncWriteExt}};
///
/// let file = fs::File::from_std(tempfile::tempfile()?);
/// let mut f = ZipWriter::new(Box::pin(file));
/// let mut fp = Pin::new(&mut f);
///
/// let opts = FileOptions::default();
/// fp.as_mut().start_file("asdf.txt", opts).await?;
/// fp.write_all(b"hello!").await?;
/// let file = f.finish().await?;
///
/// let mut f = ZipArchive::new(file).await?;
/// let mut f = Pin::new(&mut f);
/// let mut s = String::new();
/// {
///   let mut zf = f.by_name("asdf.txt").await?;
///   zf.read_to_string(&mut s).await?;
/// }
/// assert_eq!(&s, "hello!");
/// # Ok(())
/// # })}
///```
pub struct ZipWriter<S> {
    inner: InnerWriter<S>,
    files: Vec<ZipFileData>,
    stats: ZipWriterStats,
    writing_to_file: bool,
    writing_to_extra_field: bool,
    writing_to_central_extra_field_only: bool,
    writing_raw: bool,
    comment: Vec<u8>,
}

struct WriteProj<'a, S> {
    pub inner: Pin<&'a mut InnerWriter<S>>,
    pub files: &'a mut Vec<ZipFileData>,
    pub stats: &'a mut ZipWriterStats,
    pub writing_to_file: &'a mut bool,
    pub writing_to_extra_field: &'a mut bool,
    pub writing_to_central_extra_field_only: &'a mut bool,
    pub writing_raw: &'a mut bool,
    pub comment: &'a mut Vec<u8>,
}

impl<S> ZipWriter<S> {
    pub fn new(inner: Pin<Box<S>>) -> Self {
        Self {
            inner: InnerWriter::NoActiveFile(inner),
            files: Vec::new(),
            stats: Default::default(),
            writing_to_file: false,
            writing_to_extra_field: false,
            writing_to_central_extra_field_only: false,
            writing_raw: false,
            comment: Vec::new(),
        }
    }

    #[inline]
    fn project(self: Pin<&mut Self>) -> WriteProj<'_, S> {
        unsafe {
            let Self {
                inner,
                files,
                stats,
                writing_to_file,
                writing_to_extra_field,
                writing_to_central_extra_field_only,
                writing_raw,
                comment,
            } = self.get_unchecked_mut();
            WriteProj {
                inner: Pin::new_unchecked(inner),
                files,
                stats,
                writing_to_file,
                writing_to_extra_field,
                writing_to_central_extra_field_only,
                writing_raw,
                comment,
            }
        }
    }

    pub fn set_raw_comment(self: Pin<&mut Self>, comment: Vec<u8>) {
        *self.project().comment = comment;
    }

    pub fn set_comment(self: Pin<&mut Self>, comment: impl Into<String>) {
        self.set_raw_comment(comment.into().into());
    }
}

impl<S: io::AsyncWrite + io::AsyncSeek> ZipWriter<S> {
    pub async fn start_file(
        mut self: Pin<&mut Self>,
        name: impl Into<String>,
        mut options: FileOptions,
    ) -> ZipResult<()> {
        if options.permissions.is_none() {
            options.permissions = Some(0o644);
        }
        *options.permissions.as_mut().unwrap() |= 0o100000;

        self.as_mut().start_entry(name, options, None).await?;

        let me = self.project();

        me.inner.wrap_compressor_stream(me.files.last().unwrap())?;

        *me.writing_to_file = true;

        Ok(())
    }

    pub async fn add_directory(
        mut self: Pin<&mut Self>,
        name: impl Into<String>,
        mut options: FileOptions,
    ) -> ZipResult<()> {
        if options.permissions.is_none() {
            options.permissions = Some(0o755);
        }
        *options.permissions.as_mut().unwrap() |= 0o40000;
        options.compression_method = CompressionMethod::Stored;

        let mut name_as_string = name.into();
        // Append a slash to the filename if it does not end with it.
        if !name_as_string.ends_with('/') {
            /* TODO: ends_with('\\') as well? */
            name_as_string.push('/');
        }

        self.as_mut()
            .start_entry(name_as_string, options, None)
            .await?;
        self.writing_to_file = false;
        Ok(())
    }

    pub async fn add_symlink(
        mut self: Pin<&mut Self>,
        name: impl Into<String>,
        target: impl Into<String>,
        mut options: FileOptions,
    ) -> ZipResult<()> {
        if options.permissions.is_none() {
            options.permissions = Some(0o777);
        }
        *options.permissions.as_mut().unwrap() |= 0o120000;
        // The symlink target is stored as file content. And compressing the target path
        // likely wastes space. So always store.
        options.compression_method = CompressionMethod::Stored;

        self.as_mut().start_entry(name, options, None).await?;
        self.writing_to_file = true;
        self.write_all(target.into().as_bytes()).await?;
        self.writing_to_file = false;

        Ok(())
    }

    pub async fn start_file_with_extra_data(
        mut self: Pin<&mut Self>,
        name: impl Into<String>,
        mut options: FileOptions,
    ) -> ZipResult<u64> {
        if options.permissions.is_none() {
            options.permissions = Some(0o644);
        }
        *options.permissions.as_mut().unwrap() |= 0o100000;

        self.as_mut().start_entry(name, options, None).await?;

        self.writing_to_file = true;
        self.writing_to_extra_field = true;

        Ok(self.files.last().unwrap().data_start.load())
    }

    pub async fn start_file_aligned(
        mut self: Pin<&mut Self>,
        name: impl Into<String>,
        options: FileOptions,
        align: u16,
    ) -> ZipResult<u64> {
        let data_start = self
            .as_mut()
            .start_file_with_extra_data(name, options)
            .await?;
        let align = align as u64;

        if align > 1 && data_start % align != 0 {
            let pad_length = (align - (data_start + 4) % align) % align;
            let pad = vec![0; pad_length as usize];
            self.write_all(b"za").await?; // 0x617a
            self.write_u16_le(pad.len() as u16).await?;
            self.write_all(&pad).await?;
            assert_eq!(
                self.as_mut().end_local_start_central_extra_data().await? % align,
                0
            );
        }
        let extra_data_end = self.end_extra_data().await?;
        Ok(extra_data_end - data_start)
    }

    async fn start_entry(
        mut self: Pin<&mut Self>,
        name: impl Into<String>,
        options: FileOptions,
        raw_values: Option<ZipRawValues>,
    ) -> ZipResult<()> {
        self.as_mut().finish_file().await?;

        let raw_values = raw_values.unwrap_or_default();

        let mut me = self.project();

        let file = me
            .inner
            .as_mut()
            .initialize_entry(raw_values, options, name.into(), me.stats)
            .await?;
        me.files.push(file);
        Ok(())
    }

    async fn finish_file(mut self: Pin<&mut Self>) -> ZipResult<()> {
        if self.writing_to_extra_field {
            // Implicitly calling [`ZipWriter::end_extra_data`] for empty files.
            self.as_mut().end_extra_data().await?;
        }

        assert!(!self.writing_raw);

        let mut me = self.project();

        let file = match me.files.last_mut() {
            None => return Ok(()),
            Some(f) => f,
        };

        me.inner
            .as_mut()
            .update_header_and_unwrap_stream(me.stats, file)
            .await?;

        *me.writing_to_file = false;
        *me.writing_raw = false;
        Ok(())
    }

    pub async fn end_local_start_central_extra_data(mut self: Pin<&mut Self>) -> ZipResult<u64> {
        let data_start = self.as_mut().end_extra_data().await?;
        self.files.last_mut().unwrap().extra_field.clear();
        self.writing_to_extra_field = true;
        self.writing_to_central_extra_field_only = true;
        Ok(data_start)
    }

    pub async fn end_extra_data(self: Pin<&mut Self>) -> ZipResult<u64> {
        if !self.writing_to_extra_field {
            return Err(io::Error::new(io::ErrorKind::Other, "Not writing to extra field").into());
        }

        let mut me = self.project();

        let file = me.files.last_mut().unwrap();

        write_spec::validate_extra_data(file).await?;

        if !*me.writing_to_central_extra_field_only {
            me.inner
                .as_mut()
                .write_extra_data_and_wrap_compressor_stream(me.stats, file)
                .await?;
        }

        let data_start = file.data_start.get_mut();
        *me.writing_to_extra_field = false;
        *me.writing_to_central_extra_field_only = false;
        Ok(*data_start)
    }

    pub async fn finish(mut self) -> ZipResult<Pin<Box<S>>> {
        Pin::new(&mut self).finalize().await?;
        Pin::new(&mut self).shutdown().await?;
        Ok(self.unwrap_inner_pin())
    }

    async fn finalize(mut self: Pin<&mut Self>) -> ZipResult<()> {
        self.as_mut().finish_file().await?;

        let me = self.project();
        me.inner.finalize(me.files, me.comment).await?;

        Ok(())
    }
}

impl<S: io::AsyncWrite + io::AsyncSeek> io::AsyncWrite for ZipWriter<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if !self.writing_to_file {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::Other,
                "No file has been started",
            )));
        }
        let mut me = self.project();
        match me.inner.as_mut().project() {
            InnerProj::NoActiveFile(_) => {
                assert!(*me.writing_to_extra_field);
                let field = Pin::new(&mut me.files.last_mut().unwrap().extra_field);
                field.poll_write(cx, buf)
            }
            InnerProj::FileWriter(wrapped) => {
                let num_written = ready!(wrapped.poll_write(cx, buf))?;
                me.stats.update(&buf[..num_written]);
                if me.stats.bytes_written > spec::ZIP64_BYTES_THR
                    && !me.files.last_mut().unwrap().large_file
                {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::Other,
                        "Large file option has not been set",
                    )));
                }
                Poll::Ready(Ok(num_written))
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.project().inner.project() {
            InnerProj::NoActiveFile(inner) => inner.poll_flush(cx),
            InnerProj::FileWriter(wrapped) => wrapped.poll_flush(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().poll_flush(cx))?;
        match self.project().inner.project() {
            InnerProj::NoActiveFile(inner) => inner.poll_shutdown(cx),
            InnerProj::FileWriter(wrapped) => wrapped.poll_shutdown(cx),
        }
    }
}

impl<S> ops::Drop for ZipWriter<S> {
    fn drop(&mut self) {
        unreachable!("must call .finish()!");
    }
}

impl<S> WrappedPin<S> for ZipWriter<S> {
    fn unwrap_inner_pin(self) -> Pin<Box<S>> {
        let inner: InnerWriter<S> = unsafe { ptr::read(&self.inner) };
        mem::forget(self);
        inner.unwrap_inner_pin()
    }
}

mod write_spec {
    use crate::{result::ZipResult, spec, types::ZipFileData};

    use std::pin::Pin;

    use tokio::io::{self, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

    pub async fn validate_extra_data(file: &ZipFileData) -> ZipResult<()> {
        if file.extra_field.len() > spec::ZIP64_ENTRY_THR {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Extra data exceeds extra field",
            )
            .into());
        }

        let mut data = file.extra_field.as_slice();

        while !data.is_empty() {
            let left = data.len();
            if left < 4 {
                return Err(
                    io::Error::new(io::ErrorKind::Other, "Incomplete extra data header").into(),
                );
            }
            let kind = data.read_u16_le().await?;
            let size = data.read_u16_le().await? as usize;
            let left = left - 4;

            if kind == 0x0001 {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "No custom ZIP64 extra data allowed",
                )
                .into());
            }

            #[cfg(not(feature = "unreserved"))]
            {
                if kind <= 31
                    || crate::write::EXTRA_FIELD_MAPPING
                        .iter()
                        .any(|&mapped| mapped == kind)
                {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        format!(
                            "Extra data header ID {kind:#06} requires crate feature \"unreserved\"",
                        ),
                    )
                    .into());
                }
            }

            if size > left {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Extra data size exceeds extra field",
                )
                .into());
            }

            data = &data[size..];
        }

        Ok(())
    }

    pub async fn update_local_file_header<S: io::AsyncWrite + io::AsyncSeek>(
        mut writer: Pin<&mut S>,
        file: &ZipFileData,
    ) -> ZipResult<()> {
        const CRC32_OFFSET: u64 = 14;
        writer
            .seek(io::SeekFrom::Start(file.header_start + CRC32_OFFSET))
            .await?;
        writer.write_u32_le(file.crc32).await?;
        if file.large_file {
            update_local_zip64_extra_field(writer.as_mut(), file).await?;
        } else {
            // check compressed size as well as it can also be slightly larger than uncompressed size
            if file.compressed_size > spec::ZIP64_BYTES_THR {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    "Large file option has not been set",
                )
                .into());
            }
            writer.write_u32_le(file.compressed_size as u32).await?;
            // uncompressed size is already checked on write to catch it as soon as possible
            writer.write_u32_le(file.uncompressed_size as u32).await?;
        }
        Ok(())
    }

    async fn update_local_zip64_extra_field<S: io::AsyncWrite + io::AsyncSeek>(
        mut writer: Pin<&mut S>,
        file: &ZipFileData,
    ) -> ZipResult<()> {
        let zip64_extra_field = file.header_start + 30 + file.file_name.as_bytes().len() as u64;
        writer
            .seek(io::SeekFrom::Start(zip64_extra_field + 4))
            .await?;
        writer.write_u64_le(file.uncompressed_size).await?;
        writer.write_u64_le(file.compressed_size).await?;
        // Excluded fields:
        // u32: disk start number
        Ok(())
    }

    pub async fn write_local_file_header<S: io::AsyncWrite>(
        mut writer: Pin<&mut S>,
        file: &ZipFileData,
    ) -> ZipResult<()> {
        // local file header signature
        writer
            .write_u32_le(spec::LOCAL_FILE_HEADER_SIGNATURE)
            .await?;
        // version needed to extract
        writer.write_u16_le(file.version_needed()).await?;
        // general purpose bit flag
        let flag = if !file.file_name.is_ascii() {
            1u16 << 11
        } else {
            0
        } | if file.encrypted { 1u16 << 0 } else { 0 };
        writer.write_u16_le(flag).await?;
        // Compression method
        #[allow(deprecated)]
        writer
            .write_u16_le(file.compression_method.to_u16())
            .await?;
        // last mod file time and last mod file date
        writer
            .write_u16_le(file.last_modified_time.timepart())
            .await?;
        writer
            .write_u16_le(file.last_modified_time.datepart())
            .await?;
        // crc-32
        writer.write_u32_le(file.crc32).await?;
        // compressed size and uncompressed size
        if file.large_file {
            writer.write_u32_le(spec::ZIP64_BYTES_THR as u32).await?;
            writer.write_u32_le(spec::ZIP64_BYTES_THR as u32).await?;
        } else {
            writer.write_u32_le(file.compressed_size as u32).await?;
            writer.write_u32_le(file.uncompressed_size as u32).await?;
        }
        // file name length
        writer
            .write_u16_le(file.file_name.as_bytes().len() as u16)
            .await?;
        // extra field length
        let extra_field_length =
            if file.large_file { 20 } else { 0 } + file.extra_field.len() as u16;
        writer.write_u16_le(extra_field_length).await?;
        // file name
        writer.write_all(file.file_name.as_bytes()).await?;
        // zip64 extra field
        if file.large_file {
            write_local_zip64_extra_field(writer.as_mut(), file).await?;
        }

        Ok(())
    }

    async fn write_local_zip64_extra_field<S: io::AsyncWrite>(
        mut writer: Pin<&mut S>,
        file: &ZipFileData,
    ) -> ZipResult<()> {
        // This entry in the Local header MUST include BOTH original
        // and compressed file size fields.
        writer.write_u16_le(0x0001).await?;
        writer.write_u16_le(16).await?;
        writer.write_u64_le(file.uncompressed_size).await?;
        writer.write_u64_le(file.compressed_size).await?;
        // Excluded fields:
        // u32: disk start number
        Ok(())
    }

    pub async fn write_central_directory_header<S: io::AsyncWrite>(
        mut writer: Pin<&mut S>,
        file: &ZipFileData,
    ) -> ZipResult<()> {
        // buffer zip64 extra field to determine its variable length
        let zip64_extra_field = [0; 28];
        let zip64_extra_field_length =
            write_central_zip64_extra_field(writer.as_mut(), file).await?;

        // central file header signature
        writer
            .write_u32_le(spec::CENTRAL_DIRECTORY_HEADER_SIGNATURE)
            .await?;
        // version made by
        let version_made_by = (file.system as u16) << 8 | (file.version_made_by as u16);
        writer.write_u16_le(version_made_by).await?;
        // version needed to extract
        writer.write_u16_le(file.version_needed()).await?;
        // general puprose bit flag
        let flag = if !file.file_name.is_ascii() {
            1u16 << 11
        } else {
            0
        } | if file.encrypted { 1u16 << 0 } else { 0 };
        writer.write_u16_le(flag).await?;
        // compression method
        #[allow(deprecated)]
        writer
            .write_u16_le(file.compression_method.to_u16())
            .await?;
        // last mod file time + date
        writer
            .write_u16_le(file.last_modified_time.timepart())
            .await?;
        writer
            .write_u16_le(file.last_modified_time.datepart())
            .await?;
        // crc-32
        writer.write_u32_le(file.crc32).await?;
        // compressed size
        writer
            .write_u32_le(file.compressed_size.min(spec::ZIP64_BYTES_THR) as u32)
            .await?;
        // uncompressed size
        writer
            .write_u32_le(file.uncompressed_size.min(spec::ZIP64_BYTES_THR) as u32)
            .await?;
        // file name length
        writer
            .write_u16_le(file.file_name.as_bytes().len() as u16)
            .await?;
        // extra field length
        writer
            .write_u16_le(zip64_extra_field_length + file.extra_field.len() as u16)
            .await?;
        // file comment length
        writer.write_u16_le(0).await?;
        // disk number start
        writer.write_u16_le(0).await?;
        // internal file attribytes
        writer.write_u16_le(0).await?;
        // external file attributes
        writer.write_u32_le(file.external_attributes).await?;
        // relative offset of local header
        writer
            .write_u32_le(file.header_start.min(spec::ZIP64_BYTES_THR) as u32)
            .await?;
        // file name
        writer.write_all(file.file_name.as_bytes()).await?;
        // zip64 extra field
        writer
            .write_all(&zip64_extra_field[..zip64_extra_field_length as usize])
            .await?;
        // extra field
        writer.write_all(&file.extra_field).await?;
        // file comment
        // <none>

        Ok(())
    }

    async fn write_central_zip64_extra_field<S: io::AsyncWrite>(
        mut writer: Pin<&mut S>,
        file: &ZipFileData,
    ) -> ZipResult<u16> {
        // The order of the fields in the zip64 extended
        // information record is fixed, but the fields MUST
        // only appear if the corresponding Local or Central
        // directory record field is set to 0xFFFF or 0xFFFFFFFF.
        let mut size = 0;
        let uncompressed_size = file.uncompressed_size > spec::ZIP64_BYTES_THR;
        let compressed_size = file.compressed_size > spec::ZIP64_BYTES_THR;
        let header_start = file.header_start > spec::ZIP64_BYTES_THR;
        if uncompressed_size {
            size += 8;
        }
        if compressed_size {
            size += 8;
        }
        if header_start {
            size += 8;
        }
        if size > 0 {
            writer.write_u16_le(0x0001).await?;
            writer.write_u16_le(size).await?;
            size += 4;

            if uncompressed_size {
                writer.write_u64_le(file.uncompressed_size).await?;
            }
            if compressed_size {
                writer.write_u64_le(file.compressed_size).await?;
            }
            if header_start {
                writer.write_u64_le(file.header_start).await?;
            }
            // Excluded fields:
            // u32: disk start number
        }
        Ok(size)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use std::io::Cursor;

    #[test]
    #[should_panic(expected = "internal error: entered unreachable code: must call .finish()!")]
    fn test_drop() {
        let buf = Cursor::new(Vec::<u8>::new());
        let _f = ZipWriter::new(Box::pin(buf));
    }
}
