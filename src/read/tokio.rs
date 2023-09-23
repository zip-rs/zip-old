#![allow(missing_docs)]

use crate::compression::CompressionMethod;
use crate::crc32::Crc32Reader;
use crate::result::{ZipError, ZipResult};
use crate::spec;
use crate::types::ZipFileData;

use std::{
    cmp, fmt,
    io::Read,
    marker::Unpin,
    mem,
    ops::{self, DerefMut},
    path::{Path, PathBuf},
    pin::Pin,
    str,
    sync::Arc,
    task::{Context, Poll},
};

use async_stream::try_stream;
use futures_core::stream::Stream;
use futures_util::{pin_mut, stream::TryStreamExt};
use indexmap::IndexMap;
use parking_lot::{Mutex, RwLock};
use tokio::{
    fs,
    io::{self, AsyncReadExt, AsyncSeekExt},
    sync::mpsc,
};
use tokio_util::io::SyncIoBridge;

#[cfg(any(
    feature = "deflate",
    feature = "deflate-miniz",
    feature = "deflate-zlib"
))]
use flate2::read::DeflateDecoder;

#[cfg(feature = "bzip2")]
use bzip2::read::BzDecoder;

pub mod utils {
    use super::*;

    use bytes::{BufMut, BytesMut};
    use indexmap::IndexSet;
    use parking_lot::{lock_api::ArcRwLockUpgradableReadGuard, Mutex, RwLock};
    use tokio::{sync::oneshot, task};

    use std::future::Future;
    use std::slice;

    #[derive(Clone)]
    pub struct Limiter<S> {
        max_len: usize,
        internal_pos: usize,
        source_stream: S,
    }

    impl<S> Limiter<S> {
        pub fn take(source_stream: S, limit: usize) -> Self {
            Self {
                max_len: limit,
                internal_pos: 0,
                source_stream,
            }
        }

        #[inline]
        fn remaining_len(&self) -> usize {
            self.max_len - self.internal_pos
        }

        #[inline]
        fn limit_length(&self, requested_length: usize) -> usize {
            cmp::min(self.remaining_len(), requested_length)
        }

        #[inline]
        fn push_cursor(&mut self, len: usize) {
            debug_assert!(len <= self.remaining_len());
            self.internal_pos += len;
        }

        pub fn into_inner(self) -> S {
            self.source_stream
        }
    }

    impl<S: Read> Read for Limiter<S> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            debug_assert!(!buf.is_empty());

            let num_bytes_to_read: usize = self.limit_length(buf.len());
            if num_bytes_to_read == 0 {
                return Ok(0);
            }

            let bytes_read = self.source_stream.read(&mut buf[..num_bytes_to_read])?;
            /* dbg!(bytes_read); */
            if bytes_read > 0 {
                self.push_cursor(bytes_read);
            }
            Ok(bytes_read)
        }
    }

    impl<S: io::AsyncRead + Unpin> io::AsyncRead for Limiter<S> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            debug_assert!(buf.remaining() > 0);

            let num_bytes_to_read: usize = self.limit_length(buf.remaining());
            /* dbg!(num_bytes_to_read); */
            if num_bytes_to_read == 0 {
                return Poll::Ready(Ok(()));
            }

            let s = self.get_mut();
            let start = buf.filled().len();
            debug_assert_eq!(start, 0);
            buf.initialize_unfilled_to(num_bytes_to_read);
            let mut unfilled_buf = buf.take(num_bytes_to_read);
            match Pin::new(&mut s.source_stream).poll_read(cx, &mut unfilled_buf) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(x) => {
                    let filled_len = unfilled_buf.filled().len();
                    Poll::Ready(x.map(|()| {
                        let bytes_read = filled_len - start;
                        /* dbg!(bytes_read); */
                        assert!(bytes_read <= num_bytes_to_read);
                        if bytes_read > 0 {
                            buf.advance(bytes_read);
                            s.push_cursor(bytes_read);
                        }
                        /* dbg!(s.remaining_len()); */
                    }))
                }
            }
        }
    }

    ///```
    /// # fn main() -> zip::result::ZipResult<()> { tokio_test::block_on(async {
    /// use std::{io::{Cursor, prelude::*}, pin::Pin, sync::Arc};
    /// use tokio::{io::{self, AsyncReadExt}, fs};
    ///
    /// let mut buf = Cursor::new(Vec::new());
    /// buf.write_all(b"hello\n")?;
    /// buf.rewind()?;
    /// let mut f = zip::read::tokio::utils::ReadAdapter::new(buf);
    /// let mut buf: Vec<u8> = Vec::new();
    /// f.read_to_end(&mut buf).await?;
    /// assert_eq!(&buf, b"hello\n");
    /// # Ok(())
    /// # })}
    ///```
    pub struct ReadAdapter<S> {
        inner: Arc<Mutex<Option<S>>>,
        completion_rx: oneshot::Receiver<io::Result<()>>,
        completion_tx: Arc<Mutex<Option<oneshot::Sender<io::Result<()>>>>>,
        buf: Arc<RwLock<BytesMut>>,
    }
    impl<S> ReadAdapter<S> {
        pub fn new(inner: S) -> Self {
            let (tx, rx) = oneshot::channel::<io::Result<()>>();
            Self {
                inner: Arc::new(Mutex::new(Some(inner))),
                completion_rx: rx,
                completion_tx: Arc::new(Mutex::new(Some(tx))),
                buf: Arc::new(RwLock::new(BytesMut::new())),
            }
        }
        pub fn into_inner(self) -> S {
            self.inner.lock().take().unwrap()
        }
    }

    impl<S: Read + Unpin + Send + 'static> io::AsyncRead for ReadAdapter<S> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            if buf.remaining() == 0 {
                return Poll::Ready(Ok(()));
            }

            {
                let ring_buf = self.buf.upgradable_read_arc();
                if !ring_buf.is_empty() {
                    let len = cmp::min(buf.remaining(), ring_buf.len());

                    let data = {
                        let mut ring_buf = ArcRwLockUpgradableReadGuard::upgrade(ring_buf);
                        ring_buf.split_to(len)
                    };
                    buf.put(data);
                    return Poll::Ready(Ok(()));
                }
            }

            let s = self.get_mut();

            match Pin::new(&mut s.completion_rx).poll(cx) {
                Poll::Ready(x) => match x {
                    Err(_) => unreachable!(),
                    Ok(x) => Poll::Ready(x),
                },
                Poll::Pending => {
                    let requested_length = buf.remaining();
                    let completion_tx = s.completion_tx.clone();
                    let inner = s.inner.clone();
                    let ring_buf = s.buf.clone();

                    let waker = cx.waker().clone();
                    task::spawn_blocking(move || {
                        let mut ring_buf = ring_buf.write();
                        ring_buf.reserve(requested_length);

                        if let Some(ref mut inner) = *inner.lock() {
                            match inner.read(unsafe {
                                slice::from_raw_parts_mut(
                                    ring_buf.chunk_mut().as_mut_ptr(),
                                    requested_length,
                                )
                            }) {
                                Err(e) => {
                                    if let Some(completion_tx) = completion_tx.lock().take() {
                                        completion_tx.send(Err(e)).unwrap();
                                    }
                                }
                                Ok(n) => {
                                    if n == 0 {
                                        if let Some(completion_tx) = completion_tx.lock().take() {
                                            completion_tx.send(Ok(())).unwrap();
                                        }
                                    } else {
                                        unsafe {
                                            ring_buf.advance_mut(n);
                                        }
                                        waker.wake();
                                    }
                                }
                            }
                        } else {
                            if let Some(completion_tx) = completion_tx.lock().take() {
                                completion_tx.send(Ok(())).unwrap();
                            }
                        }
                    });

                    Poll::Pending
                }
            }
        }
    }

    #[derive(Debug, Clone)]
    struct CompletedPaths {
        seen: IndexSet<PathBuf>,
    }

    impl CompletedPaths {
        pub fn new() -> Self {
            Self {
                seen: IndexSet::new(),
            }
        }

        pub fn contains(&self, path: impl AsRef<Path>) -> bool {
            self.seen.contains(path.as_ref())
        }

        pub fn containing_dirs<'a>(
            path: &'a (impl AsRef<Path> + ?Sized),
        ) -> impl Iterator<Item = &'a Path> {
            let is_dir = path.as_ref().to_string_lossy().ends_with('/');
            path.as_ref()
                .ancestors()
                .inspect(|p| {
                    if p == &Path::new("/") {
                        unreachable!("did not expect absolute paths")
                    }
                })
                .filter_map(move |p| {
                    if &p == &path.as_ref() {
                        if is_dir {
                            Some(p)
                        } else {
                            None
                        }
                    } else if p == Path::new("") {
                        None
                    } else {
                        Some(p)
                    }
                })
        }

        pub fn new_containing_dirs_needed<'a>(
            &self,
            path: &'a (impl AsRef<Path> + ?Sized),
        ) -> Vec<&'a Path> {
            let mut ret: Vec<_> = Self::containing_dirs(path)
                /* Assuming we are given ancestors in order from child to parent. */
                .take_while(|p| !self.contains(p))
                .collect();
            /* Get dirs in order from parent to child. */
            ret.reverse();
            ret
        }

        pub fn write_dirs<'a>(&mut self, paths: &[&'a Path]) {
            for path in paths.iter() {
                if !self.contains(path) {
                    self.seen.insert(path.to_path_buf());
                }
            }
        }
    }

    #[derive(Debug)]
    pub enum IntermediateFile {
        Immediate(Arc<RwLock<Box<[u8]>>>, usize),
        Paging(Arc<Mutex<fs::File>>, Arc<PathBuf>, usize),
    }

    impl fmt::Display for IntermediateFile {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            let len = self.len();
            match self {
                Self::Immediate(arc, pos) => match str::from_utf8(arc.read().as_ref()) {
                    Ok(s) => write!(f, "Immediate(@{})[{}](\"{}\")", pos, s.len(), s),
                    Err(_) => write!(f, "Immediate[{}](<binary>)", len),
                    /* Err(_) => write!( */
                    /*     f, */
                    /*     "Immediate(@{})[{}](<binary> = \"{}\")", */
                    /*     pos, */
                    /*     arc.read().unwrap().len(), */
                    /*     String::from_utf8_lossy(arc.read().unwrap().as_ref()), */
                    /* ), */
                },
                Self::Paging(_, path, len) => write!(f, "Paging[{}]({})", len, path.display()),
            }
        }
    }

    impl IntermediateFile {
        pub async fn try_into_sync(self) -> io::Result<SyncIntermediateFile> {
            match self {
                Self::Immediate(arc, pos) => Ok(SyncIntermediateFile::Immediate(arc, pos)),
                Self::Paging(f, path, len) => Ok(SyncIntermediateFile::Paging(
                    /* TODO: .try_clone() only requires a read lock? */
                    Arc::new(Mutex::new(f.lock().try_clone().await?.into_std().await)),
                    path,
                    len,
                )),
            }
        }

        pub fn len(&self) -> usize {
            match self {
                Self::Immediate(arc, _) => arc.read().len(),
                Self::Paging(_, _, len) => *len,
            }
        }

        pub fn immediate(len: usize) -> Self {
            Self::Immediate(Arc::new(RwLock::new(vec![0; len].into_boxed_slice())), 0)
        }

        /* pub async fn paging(len: usize) -> io::Result<Self> { */
        /*     let f = tempfile::NamedTempFile::with_prefix("intermediate")?; */
        /*     let (mut f, path) = f.keep().unwrap(); */
        /*     f.set_len(len as u64)?; */
        /*     f.rewind()?; */
        /*     Ok(Self::Paging(UnsafeCell::new(f), path, len)) */
        /* } */

        pub async fn create_at_path(path: impl AsRef<Path>, len: usize) -> io::Result<Self> {
            let f = fs::File::create(path.as_ref()).await?;
            f.set_len(len as u64).await?;
            Ok(Self::Paging(
                Arc::new(Mutex::new(f)),
                Arc::new(path.as_ref().to_path_buf()),
                len,
            ))
        }

        pub async fn open_from_path(path: impl AsRef<Path>) -> io::Result<Self> {
            let mut f = fs::File::open(path.as_ref()).await?;
            let len = f.seek(io::SeekFrom::End(0)).await?;
            f.rewind().await?;
            Ok(Self::Paging(
                Arc::new(Mutex::new(f)),
                Arc::new(path.as_ref().to_path_buf()),
                len as usize,
            ))
        }

        pub async fn open_from_path_known_len(
            path: impl AsRef<Path>,
            len: usize,
        ) -> io::Result<Self> {
            let f = fs::File::open(path.as_ref()).await?;
            Ok(Self::Paging(
                Arc::new(Mutex::new(f)),
                Arc::new(path.as_ref().to_path_buf()),
                len,
            ))
        }

        pub async fn with_cursor_at(mut self, pos: io::SeekFrom) -> io::Result<Self> {
            self.seek(pos).await?;
            Ok(self)
        }

        pub async fn clone_handle(&self) -> io::Result<Self> {
            match self {
                Self::Immediate(arc, pos) => Ok(Self::Immediate(arc.clone(), *pos)),
                Self::Paging(prev_f, path, len) => {
                    let prev_cursor = prev_f.lock().stream_position().await?;
                    Ok(Self::open_from_path_known_len(path.as_ref(), *len)
                        .await?
                        .with_cursor_at(io::SeekFrom::Start(prev_cursor))
                        .await?)
                }
            }
        }

        pub fn from_bytes(bytes: &[u8]) -> Self {
            Self::Immediate(Arc::new(RwLock::new(bytes.into())), 0)
        }

        pub async fn remove_backing_file(&mut self) -> io::Result<()> {
            match self {
                Self::Immediate(_, _) => Ok(()),
                Self::Paging(_, path, _) => fs::remove_file(path.as_ref()).await,
            }
        }
    }

    impl io::AsyncRead for IntermediateFile {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            debug_assert!(buf.remaining() > 0);
            match self.get_mut() {
                Self::Immediate(arc, pos) => {
                    let beg = *pos;
                    let full_len = arc.read().len();
                    debug_assert!(full_len >= beg);
                    let end = cmp::min(beg + buf.remaining(), full_len);
                    let src = &arc.read()[beg..end];

                    buf.put_slice(src);

                    *pos += src.len();

                    Poll::Ready(Ok(()))
                }
                Self::Paging(file, _, _) => {
                    Pin::new(&mut file.lock().deref_mut()).poll_read(cx, buf)
                }
            }
        }
    }

    impl io::AsyncSeek for IntermediateFile {
        fn start_seek(self: Pin<&mut Self>, pos_arg: io::SeekFrom) -> io::Result<()> {
            match self.get_mut() {
                Self::Immediate(arc, pos) => {
                    let full_len = arc.read().len();
                    match pos_arg {
                        io::SeekFrom::Start(s) => {
                            *pos = cmp::min(s as usize, full_len);
                        }
                        io::SeekFrom::End(from_end) => {
                            let result = ((full_len as i64) + from_end) as isize;
                            assert!(full_len <= isize::MAX as usize);
                            let result = cmp::min(result, full_len as isize);
                            let result = cmp::max(result, 0) as usize;
                            *pos = result;
                        }
                        io::SeekFrom::Current(from_cur) => {
                            let result = ((*pos as i64) + from_cur) as isize;
                            assert!(full_len <= isize::MAX as usize);
                            let result = cmp::min(result, full_len as isize);
                            let result = cmp::max(result, 0) as usize;
                            *pos = result;
                        }
                    };
                    Ok(())
                }
                Self::Paging(file, _, _) => {
                    Pin::new(&mut file.lock().deref_mut()).start_seek(pos_arg)
                }
            }
        }
        fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
            match self.get_mut() {
                Self::Immediate(_, pos) => Poll::Ready(Ok(*pos as u64)),
                Self::Paging(file, _, _) => {
                    Pin::new(&mut file.lock().deref_mut()).poll_complete(cx)
                }
            }
        }
    }

    impl io::AsyncWrite for IntermediateFile {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            match self.get_mut() {
                Self::Immediate(arc, ref mut pos) => {
                    let beg = *pos;
                    let full_len = arc.read().len();
                    assert!(beg <= full_len);
                    let end = cmp::min(beg + buf.len(), full_len);

                    let dst = &mut arc.write()[beg..end];
                    dst.copy_from_slice(&buf[..dst.len()]);
                    *pos += dst.len();

                    Poll::Ready(Ok(dst.len()))
                }
                Self::Paging(file, _, _) => {
                    Pin::new(&mut file.lock().deref_mut()).poll_write(cx, buf)
                }
            }
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.get_mut() {
                Self::Immediate(_, _) => Poll::Ready(Ok(())),
                Self::Paging(file, _, _) => Pin::new(&mut file.lock().deref_mut()).poll_flush(cx),
            }
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.get_mut() {
                Self::Immediate(_, _) => Poll::Ready(Ok(())),
                Self::Paging(file, _, _) => {
                    Pin::new(&mut file.lock().deref_mut()).poll_shutdown(cx)
                }
            }
        }
    }

    pub mod sync {
        use parking_lot::{Mutex, RwLock};

        use std::{
            cmp, fmt, fs,
            io::{self, prelude::*},
            path::{Path, PathBuf},
            str,
            sync::Arc,
        };

        #[derive(Debug)]
        pub enum SyncIntermediateFile {
            Immediate(Arc<RwLock<Box<[u8]>>>, usize),
            Paging(Arc<Mutex<fs::File>>, Arc<PathBuf>, usize),
        }

        impl fmt::Display for SyncIntermediateFile {
            fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
                let len = self.len();
                match self {
                    Self::Immediate(arc, pos) => match str::from_utf8(arc.read().as_ref()) {
                        Ok(s) => write!(f, "Immediate(@{})[{}](\"{}\")", pos, s.len(), s),
                        Err(_) => write!(f, "Immediate[{}](<binary>)", len),
                        /* Err(_) => write!( */
                        /*     f, */
                        /*     "Immediate(@{})[{}](<binary> = \"{}\")", */
                        /*     pos, */
                        /*     arc.read().unwrap().len(), */
                        /*     String::from_utf8_lossy(arc.read().unwrap().as_ref()), */
                        /* ), */
                    },
                    Self::Paging(_, path, len) => write!(f, "Paging[{}]({})", len, path.display()),
                }
            }
        }

        impl SyncIntermediateFile {
            pub fn len(&self) -> usize {
                match self {
                    Self::Immediate(arc, _) => arc.read().len(),
                    Self::Paging(_, _, len) => *len,
                }
            }

            pub fn immediate(len: usize) -> Self {
                Self::Immediate(Arc::new(RwLock::new(vec![0; len].into_boxed_slice())), 0)
            }

            /* pub async fn paging(len: usize) -> io::Result<Self> { */
            /*     let f = tempfile::NamedTempFile::with_prefix("intermediate")?; */
            /*     let (mut f, path) = f.keep().unwrap(); */
            /*     f.set_len(len as u64)?; */
            /*     f.rewind()?; */
            /*     Ok(Self::Paging(UnsafeCell::new(f), path, len)) */
            /* } */

            pub fn create_at_path(path: impl AsRef<Path>, len: usize) -> io::Result<Self> {
                let f = fs::File::create(path.as_ref())?;
                f.set_len(len as u64)?;
                Ok(Self::Paging(
                    Arc::new(Mutex::new(f)),
                    Arc::new(path.as_ref().to_path_buf()),
                    len,
                ))
            }

            pub fn open_from_path(path: impl AsRef<Path>) -> io::Result<Self> {
                let mut f = fs::File::open(path.as_ref())?;
                let len = f.seek(io::SeekFrom::End(0))?;
                f.rewind()?;
                Ok(Self::Paging(
                    Arc::new(Mutex::new(f)),
                    Arc::new(path.as_ref().to_path_buf()),
                    len as usize,
                ))
            }

            pub fn open_from_path_known_len(
                path: impl AsRef<Path>,
                len: usize,
            ) -> io::Result<Self> {
                let f = fs::File::open(path.as_ref())?;
                Ok(Self::Paging(
                    Arc::new(Mutex::new(f)),
                    Arc::new(path.as_ref().to_path_buf()),
                    len,
                ))
            }

            pub fn with_cursor_at(mut self, pos: io::SeekFrom) -> io::Result<Self> {
                self.seek(pos)?;
                Ok(self)
            }

            pub fn clone_handle(&self) -> io::Result<Self> {
                match self {
                    Self::Immediate(arc, pos) => Ok(Self::Immediate(arc.clone(), *pos)),
                    Self::Paging(prev_f, path, len) => {
                        let prev_cursor = prev_f.lock().stream_position()?;
                        Ok(Self::open_from_path_known_len(path.as_ref(), *len)?
                            .with_cursor_at(io::SeekFrom::Start(prev_cursor))?)
                    }
                }
            }

            pub fn from_bytes(bytes: &[u8]) -> Self {
                Self::Immediate(Arc::new(RwLock::new(bytes.into())), 0)
            }

            pub fn remove_backing_file(&mut self) -> io::Result<()> {
                match self {
                    Self::Immediate(_, _) => Ok(()),
                    Self::Paging(_, path, _) => fs::remove_file(path.as_ref()),
                }
            }
        }

        impl std::io::Read for SyncIntermediateFile {
            fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
                debug_assert!(!buf.is_empty());
                match self {
                    Self::Immediate(arc, pos) => {
                        let beg = *pos;
                        let full_len = arc.read().len();
                        debug_assert!(full_len >= beg);
                        let end = cmp::min(beg + buf.len(), full_len);
                        let src = &arc.read()[beg..end];

                        buf[..src.len()].copy_from_slice(src);
                        *pos += src.len();

                        Ok(src.len())
                    }
                    Self::Paging(file, _, _) => file.lock().read(buf),
                }
            }
        }

        impl std::io::Seek for SyncIntermediateFile {
            fn seek(&mut self, pos_arg: io::SeekFrom) -> io::Result<u64> {
                match self {
                    Self::Immediate(arc, pos) => {
                        let full_len = arc.read().len();
                        match pos_arg {
                            io::SeekFrom::Start(s) => {
                                *pos = cmp::min(s as usize, full_len);
                            }
                            io::SeekFrom::End(from_end) => {
                                let result = ((full_len as i64) + from_end) as isize;
                                assert!(full_len <= isize::MAX as usize);
                                let result = cmp::min(result, full_len as isize);
                                let result = cmp::max(result, 0) as usize;
                                *pos = result;
                            }
                            io::SeekFrom::Current(from_cur) => {
                                let result = ((*pos as i64) + from_cur) as isize;
                                assert!(full_len <= isize::MAX as usize);
                                let result = cmp::min(result, full_len as isize);
                                let result = cmp::max(result, 0) as usize;
                                *pos = result;
                            }
                        };
                        Ok(*pos as u64)
                    }
                    Self::Paging(file, _, _) => file.lock().seek(pos_arg),
                }
            }
        }

        impl std::io::Write for SyncIntermediateFile {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                match self {
                    Self::Immediate(arc, pos) => {
                        let beg = *pos;
                        let full_len = arc.read().len();
                        assert!(beg <= full_len);
                        let end = cmp::min(beg + buf.len(), full_len);

                        let dst = &mut arc.write()[beg..end];
                        dst.copy_from_slice(&buf[..dst.len()]);
                        *pos += dst.len();
                        Ok(dst.len())
                    }
                    Self::Paging(file, _, _) => file.lock().write(buf),
                }
            }

            fn flush(&mut self) -> io::Result<()> {
                match self {
                    Self::Immediate(_, _) => Ok(()),
                    Self::Paging(file, _, _) => file.lock().flush(),
                }
            }
        }
    }
    pub use sync::SyncIntermediateFile;
}
pub use utils::{IntermediateFile, Limiter, ReadAdapter, SyncIntermediateFile};

pub trait ReaderWrapper<S>: io::AsyncRead + Unpin {
    fn construct(data: &ZipFileData, s: Limiter<S>) -> Self
    where
        Self: Sized;
    fn into_inner(self) -> Limiter<S>;
}

pub struct StoredReader<S>(Crc32Reader<Limiter<S>>);

impl<S: io::AsyncRead + Unpin> io::AsyncRead for StoredReader<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
    }
}

impl<S: io::AsyncRead + Unpin> ReaderWrapper<S> for StoredReader<S> {
    fn construct(data: &ZipFileData, s: Limiter<S>) -> Self {
        Self(Crc32Reader::new(s, data.crc32, false))
    }
    fn into_inner(self) -> Limiter<S> {
        self.0.into_inner()
    }
}

pub struct DeflateReader<S>(Crc32Reader<ReadAdapter<DeflateDecoder<SyncIoBridge<Limiter<S>>>>>);

impl<S: io::AsyncRead + Unpin + Send + 'static> io::AsyncRead for DeflateReader<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
    }
}

impl<S: io::AsyncRead + Unpin + Send + 'static> ReaderWrapper<S> for DeflateReader<S> {
    fn construct(data: &ZipFileData, s: Limiter<S>) -> Self {
        Self(Crc32Reader::new(
            ReadAdapter::new(DeflateDecoder::new(SyncIoBridge::new(s))),
            data.crc32,
            false,
        ))
    }
    fn into_inner(self) -> Limiter<S> {
        self.0.into_inner().into_inner().into_inner().into_inner()
    }
}

pub struct BzipReader<S>(Crc32Reader<ReadAdapter<BzDecoder<SyncIoBridge<Limiter<S>>>>>);

impl<S: io::AsyncRead + Unpin + Send + 'static> io::AsyncRead for BzipReader<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().0).poll_read(cx, buf)
    }
}

impl<S: io::AsyncRead + Unpin + Send + 'static> ReaderWrapper<S> for BzipReader<S> {
    fn construct(data: &ZipFileData, s: Limiter<S>) -> Self {
        Self(Crc32Reader::new(
            ReadAdapter::new(BzDecoder::new(SyncIoBridge::new(s))),
            data.crc32,
            false,
        ))
    }
    fn into_inner(self) -> Limiter<S> {
        self.0.into_inner().into_inner().into_inner().into_inner()
    }
}

#[derive(Default)]
pub enum ZipFileWrappedReader<S> {
    #[default]
    NoOp,
    Stored(StoredReader<S>),
    Deflated(DeflateReader<S>),
    Bzip2(BzipReader<S>),
}

impl<S: io::AsyncRead + Unpin + Send + 'static> io::AsyncRead for ZipFileWrappedReader<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::NoOp => unreachable!(),
            Self::Stored(r) => Pin::new(r).poll_read(cx, buf),
            Self::Deflated(r) => Pin::new(r).poll_read(cx, buf),
            Self::Bzip2(r) => Pin::new(r).poll_read(cx, buf),
        }
    }
}

impl<S: io::AsyncRead + Unpin + Send + 'static> ReaderWrapper<S> for ZipFileWrappedReader<S> {
    fn construct(data: &ZipFileData, s: Limiter<S>) -> Self {
        match data.compression_method {
            CompressionMethod::Stored => Self::Stored(StoredReader::<S>::construct(data, s)),
            #[cfg(any(
                feature = "deflate",
                feature = "deflate-miniz",
                feature = "deflate-zlib"
            ))]
            CompressionMethod::Deflated => Self::Deflated(DeflateReader::<S>::construct(data, s)),
            #[cfg(feature = "bzip2")]
            CompressionMethod::Bzip2 => Self::Bzip2(BzipReader::<S>::construct(data, s)),
            _ => todo!("other compression methods not supported yet!"),
        }
    }
    fn into_inner(self) -> Limiter<S> {
        match self {
            Self::NoOp => unreachable!(),
            Self::Stored(r) => r.into_inner(),
            Self::Deflated(r) => r.into_inner(),
            Self::Bzip2(r) => r.into_inner(),
        }
    }
}

pub async fn find_content<S: io::AsyncRead + io::AsyncSeek + Unpin>(
    data: &ZipFileData,
    mut reader: S,
) -> ZipResult<Limiter<S>> {
    // Parse local header
    reader.seek(io::SeekFrom::Start(data.header_start)).await?;

    let signature = reader.read_u32_le().await?;
    if signature != spec::LOCAL_FILE_HEADER_SIGNATURE {
        return Err(ZipError::InvalidArchive("Invalid local file header"));
    }

    reader.seek(io::SeekFrom::Current(22)).await?;
    let file_name_length = reader.read_u16_le().await? as u64;
    /* NB: zip files have separate local and central extra data records. The length of the local
     * extra field is being parsed here. The value of this field cannot be inferred from the
     * central record data. */
    let extra_field_length = reader.read_u16_le().await? as u64;
    let magic_and_header = 4 + 22 + 2 + 2;
    let data_start = data.header_start + magic_and_header + file_name_length + extra_field_length;
    data.data_start.store(data_start);

    reader.seek(io::SeekFrom::Start(data_start)).await?;
    Ok(Limiter::take(reader, data.compressed_size as usize))
}

pub async fn get_reader<S: io::AsyncRead + io::AsyncSeek + Unpin + Send + 'static>(
    data: &ZipFileData,
    reader: S,
) -> ZipResult<ZipFileWrappedReader<S>> {
    let limited_reader = find_content(data, reader).await?;
    Ok(ZipFileWrappedReader::<S>::construct(data, limited_reader))
}

#[derive(Debug)]
pub struct Shared {
    files: IndexMap<String, ZipFileData>,
    offset: u64,
    comment: Vec<u8>,
}

pub struct ZipFile<S: io::AsyncRead + Unpin + Send + 'static> {
    shared: Arc<Shared>,
    index: usize,
    wrapped_reader: ZipFileWrappedReader<S>,
    parent_reader: Arc<Mutex<Option<S>>>,
}

impl<S: io::AsyncRead + Unpin + Send + 'static> ops::Drop for ZipFile<S> {
    fn drop(&mut self) {
        match mem::take(&mut self.wrapped_reader) {
            ZipFileWrappedReader::NoOp => (),
            x => {
                let _ = self
                    .parent_reader
                    .lock()
                    .insert(x.into_inner().into_inner());
            }
        }
    }
}

impl<S: io::AsyncRead + Unpin + Send + 'static> ZipFile<S> {
    #[inline]
    pub fn data(&self) -> &ZipFileData {
        let (_, data) = self.shared.as_ref().files.get_index(self.index).unwrap();
        data
    }

    pub async fn extract_single(self: Pin<&mut Self>, target: Arc<PathBuf>) -> ZipResult<PathBuf> {
        match self.data().enclosed_name().and_then(|s| s.to_str()) {
            None => Err(ZipError::InvalidArchive(
                "could not extract enclosed_name()",
            )),
            Some(name) => {
                let is_dir = name.ends_with('/');
                let resulting_path = target.join(name);
                if is_dir {
                    fs::create_dir_all(&resulting_path).await?;
                } else {
                    match resulting_path.parent() {
                        None => (),
                        Some(ref p) if p == &Path::new("") => (),
                        Some(p) => {
                            fs::create_dir_all(p).await?;
                        }
                    }
                    let mut f = fs::File::create(&resulting_path).await?;
                    io::copy(self.get_mut(), &mut f).await?;
                }
                Ok(resulting_path)
            }
        }
    }
}

impl<S: io::AsyncRead + Unpin + Send + 'static> io::AsyncRead for ZipFile<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().wrapped_reader).poll_read(cx, buf)
    }
}

#[derive(Clone, Debug)]
pub struct ZipArchive<S> {
    reader: Arc<Mutex<Option<S>>>,
    shared: Arc<Shared>,
}

impl<S: io::AsyncRead + io::AsyncSeek + Unpin> ZipArchive<S> {
    pub(crate) async fn get_directory_counts(
        reader: Pin<&mut S>,
        footer: &spec::CentralDirectoryEnd,
        cde_start_pos: u64,
    ) -> ZipResult<(u64, u64, usize)> {
        // See if there's a ZIP64 footer. The ZIP64 locator if present will
        // have its signature 20 bytes in front of the standard footer. The
        // standard footer, in turn, is 22+N bytes large, where N is the
        // comment length. Therefore:
        let reader = reader.get_mut();
        let zip64locator = if reader
            .seek(io::SeekFrom::End(
                -(20 + 22 + footer.zip_file_comment.len() as i64),
            ))
            .await
            .is_ok()
        {
            match spec::Zip64CentralDirectoryEndLocator::parse_async(Pin::new(reader)).await {
                Ok(loc) => Some(loc),
                Err(ZipError::InvalidArchive(_)) => {
                    // No ZIP64 header; that's actually fine. We're done here.
                    None
                }
                Err(e) => {
                    // Yikes, a real problem
                    return Err(e);
                }
            }
        } else {
            // Empty Zip files will have nothing else so this error might be fine. If
            // not, we'll find out soon.
            None
        };

        match zip64locator {
            None => {
                // Some zip files have data prepended to them, resulting in the
                // offsets all being too small. Get the amount of error by comparing
                // the actual file position we found the CDE at with the offset
                // recorded in the CDE.
                let archive_offset = cde_start_pos
                    .checked_sub(footer.central_directory_size as u64)
                    .and_then(|x| x.checked_sub(footer.central_directory_offset as u64))
                    .ok_or(ZipError::InvalidArchive(
                        "Invalid central directory size or offset",
                    ))?;

                let directory_start = footer.central_directory_offset as u64 + archive_offset;
                let number_of_files = footer.number_of_files_on_this_disk as usize;
                Ok((archive_offset, directory_start, number_of_files))
            }
            Some(locator64) => {
                // If we got here, this is indeed a ZIP64 file.

                if !footer.record_too_small()
                    && footer.disk_number as u32 != locator64.disk_with_central_directory
                {
                    return Err(ZipError::UnsupportedArchive(
                        "Support for multi-disk files is not implemented",
                    ));
                }

                // We need to reassess `archive_offset`. We know where the ZIP64
                // central-directory-end structure *should* be, but unfortunately we
                // don't know how to precisely relate that location to our current
                // actual offset in the file, since there may be junk at its
                // beginning. Therefore we need to perform another search, as in
                // read::CentralDirectoryEnd::find_and_parse, except now we search
                // forward.

                let search_upper_bound = cde_start_pos
                    .checked_sub(60) // minimum size of Zip64CentralDirectoryEnd + Zip64CentralDirectoryEndLocator
                    .ok_or(ZipError::InvalidArchive(
                        "File cannot contain ZIP64 central directory end",
                    ))?;
                let (footer, archive_offset) =
                    spec::Zip64CentralDirectoryEnd::find_and_parse_async(
                        Pin::new(reader),
                        locator64.end_of_central_directory_offset,
                        search_upper_bound,
                    )
                    .await?;

                if footer.disk_number != footer.disk_with_central_directory {
                    return Err(ZipError::UnsupportedArchive(
                        "Support for multi-disk files is not implemented",
                    ));
                }

                let directory_start = footer
                    .central_directory_offset
                    .checked_add(archive_offset)
                    .ok_or({
                        ZipError::InvalidArchive("Invalid central directory size or offset")
                    })?;

                Ok((
                    archive_offset,
                    directory_start,
                    footer.number_of_files as usize,
                ))
            }
        }
    }

    pub async fn new(mut reader: S) -> ZipResult<Self> {
        let (footer, cde_start_pos) =
            spec::CentralDirectoryEnd::find_and_parse_async(Pin::new(&mut reader)).await?;

        if !footer.record_too_small() && footer.disk_number != footer.disk_with_central_directory {
            return Err(ZipError::UnsupportedArchive(
                "Support for multi-disk files is not implemented",
            ));
        }

        let (archive_offset, directory_start, number_of_files) =
            Self::get_directory_counts(Pin::new(&mut reader), &footer, cde_start_pos).await?;

        // If the parsed number of files is greater than the offset then
        // something fishy is going on and we shouldn't trust number_of_files.
        let file_capacity = if number_of_files > cde_start_pos as usize {
            0
        } else {
            number_of_files
        };

        let mut files = IndexMap::with_capacity(file_capacity);

        if reader
            .seek(io::SeekFrom::Start(directory_start))
            .await
            .is_err()
        {
            return Err(ZipError::InvalidArchive(
                "Could not seek to start of central directory",
            ));
        }

        for _ in 0..number_of_files {
            let file = central_header_to_zip_file(Pin::new(&mut reader), archive_offset).await?;
            assert!(files.insert(file.file_name.clone(), file).is_none());
        }

        let shared = Arc::new(Shared {
            files,
            offset: archive_offset,
            comment: footer.zip_file_comment,
        });

        Ok(ZipArchive {
            reader: Arc::new(Mutex::new(Some(reader))),
            shared,
        })
    }
}

impl<S> ZipArchive<S> {
    pub fn len(&self) -> usize {
        self.shared.files.len()
    }

    pub fn is_empty(&self) -> bool {
        self.shared.files.is_empty()
    }

    pub fn offset(&self) -> u64 {
        self.shared.offset
    }

    pub fn comment(&self) -> &[u8] {
        &self.shared.comment
    }

    pub fn file_names(&self) -> impl Iterator<Item = &str> {
        self.shared.files.keys().map(|s| s.as_str())
    }

    pub fn into_inner(self) -> S {
        self.reader.lock().take().unwrap()
    }
}

impl<S: io::AsyncRead + io::AsyncSeek + Unpin + Send + 'static> ZipArchive<S> {
    pub async fn by_name(self: Pin<&mut Self>, name: &str) -> ZipResult<ZipFile<S>> {
        let index = match self.shared.files.get_index_of(name) {
            None => {
                return Err(ZipError::FileNotFound);
            }
            Some(n) => n,
        };
        self.by_index(index).await
    }

    pub async fn by_index(self: Pin<&mut Self>, index: usize) -> ZipResult<ZipFile<S>> {
        let s = self.get_mut();
        let data = match s.shared.as_ref().files.get_index(index) {
            None => {
                return Err(ZipError::FileNotFound);
            }
            Some((_, data)) => data,
        };
        let shared = s.shared.clone();
        let parent_reader = s.reader.clone();
        let reader = s.reader.lock().take().unwrap();
        let wrapped_reader = get_reader(data, reader).await?;
        Ok(ZipFile {
            shared,
            index,
            wrapped_reader,
            parent_reader,
        })
    }

    pub fn entries_stream(self: Pin<&mut Self>) -> impl Stream<Item = ZipResult<ZipFile<S>>> + '_ {
        try_stream! {
            let s = self.get_mut();

            for i in 0..s.len() {
                let f = Pin::new(&mut *s).by_index(i).await?;
                yield f;
            }
        }
    }

    ///```
    /// # fn main() -> zip::result::ZipResult<()> { tokio_test::block_on(async {
    /// use std::{io::{Cursor, prelude::*}, pin::Pin, sync::Arc};
    /// use tokio::{io, fs};
    ///
    /// let buf = Cursor::new(Vec::new());
    /// let mut f = zip::ZipWriter::new(buf);
    /// let options = zip::write::FileOptions::default()
    ///   .compression_method(zip::CompressionMethod::Deflated);
    /// f.start_file("a/b.txt", options)?;
    /// f.write_all(b"hello\n")?;
    /// let buf = f.finish()?;
    /// let mut f = zip::read::tokio::ZipArchive::new(buf).await?;
    ///
    /// let t = tempfile::tempdir()?;
    ///
    /// let root = t.path();
    /// Pin::new(&mut f).extract(Arc::new(root.to_path_buf())).await?;
    /// let msg = fs::read_to_string(root.join("a/b.txt")).await?;
    /// assert_eq!(&msg, "hello\n");
    /// # Ok(())
    /// # })}
    ///```
    pub async fn extract(self: Pin<&mut Self>, target: Arc<PathBuf>) -> ZipResult<()> {
        let entries = self.entries_stream();
        pin_mut!(entries);

        while let Some(mut file) = entries.try_next().await? {
            let _path = Pin::new(&mut file).extract_single(target.clone()).await?;
        }
        Ok(())
    }
}

/// Parse a central directory entry to collect the information for the file.
pub(crate) async fn central_header_to_zip_file<R: io::AsyncRead + io::AsyncSeek>(
    mut reader: Pin<&mut R>,
    archive_offset: u64,
) -> ZipResult<ZipFileData> {
    let central_header_start = reader.stream_position().await?;

    // Parse central header
    let signature = reader.read_u32_le().await?;
    if signature != spec::CENTRAL_DIRECTORY_HEADER_SIGNATURE {
        Err(ZipError::InvalidArchive("Invalid Central Directory header"))
    } else {
        central_header_to_zip_file_inner(reader, archive_offset, central_header_start).await
    }
}

/// Parse a central directory entry to collect the information for the file.
async fn central_header_to_zip_file_inner<R: io::AsyncRead>(
    mut reader: Pin<&mut R>,
    archive_offset: u64,
    central_header_start: u64,
) -> ZipResult<ZipFileData> {
    use crate::cp437::FromCp437;
    use crate::types::{AtomicU64, DateTime, System};

    let version_made_by = reader.read_u16_le().await?;
    let _version_to_extract = reader.read_u16_le().await?;
    let flags = reader.read_u16_le().await?;
    let encrypted = flags & 1 == 1;
    let is_utf8 = flags & (1 << 11) != 0;
    let using_data_descriptor = flags & (1 << 3) != 0;
    let compression_method = reader.read_u16_le().await?;
    let last_mod_time = reader.read_u16_le().await?;
    let last_mod_date = reader.read_u16_le().await?;
    let crc32 = reader.read_u32_le().await?;
    let compressed_size = reader.read_u32_le().await?;
    let uncompressed_size = reader.read_u32_le().await?;
    let file_name_length = reader.read_u16_le().await? as usize;
    let extra_field_length = reader.read_u16_le().await? as usize;
    let file_comment_length = reader.read_u16_le().await? as usize;
    let _disk_number = reader.read_u16_le().await?;
    let _internal_file_attributes = reader.read_u16_le().await?;
    let external_file_attributes = reader.read_u32_le().await?;
    let offset = reader.read_u32_le().await? as u64;
    let mut file_name_raw = vec![0; file_name_length];
    reader.read_exact(&mut file_name_raw).await?;
    let mut extra_field = vec![0; extra_field_length];
    reader.read_exact(&mut extra_field).await?;
    let mut file_comment_raw = vec![0; file_comment_length];
    reader.read_exact(&mut file_comment_raw).await?;

    let file_name = match is_utf8 {
        true => String::from_utf8_lossy(&file_name_raw).into_owned(),
        false => file_name_raw.clone().from_cp437(),
    };
    let file_comment = match is_utf8 {
        true => String::from_utf8_lossy(&file_comment_raw).into_owned(),
        false => file_comment_raw.from_cp437(),
    };

    // Construct the result
    let mut result = ZipFileData {
        system: System::from_u8((version_made_by >> 8) as u8),
        version_made_by: version_made_by as u8,
        encrypted,
        using_data_descriptor,
        compression_method: {
            #[allow(deprecated)]
            CompressionMethod::from_u16(compression_method)
        },
        compression_level: None,
        last_modified_time: DateTime::from_msdos(last_mod_date, last_mod_time),
        crc32,
        compressed_size: compressed_size as u64,
        uncompressed_size: uncompressed_size as u64,
        file_name,
        file_name_raw,
        extra_field,
        file_comment,
        header_start: offset,
        central_header_start,
        data_start: AtomicU64::new(0),
        external_attributes: external_file_attributes,
        large_file: false,
        aes_mode: None,
    };

    match parse_extra_field(&mut result).await {
        Ok(..) | Err(ZipError::Io(..)) => {}
        Err(e) => return Err(e),
    }

    let aes_enabled = result.compression_method == CompressionMethod::AES;
    if aes_enabled && result.aes_mode.is_none() {
        return Err(ZipError::InvalidArchive(
            "AES encryption without AES extra data field",
        ));
    }

    // Account for shifted zip offsets.
    result.header_start = result
        .header_start
        .checked_add(archive_offset)
        .ok_or(ZipError::InvalidArchive("Archive header is too large"))?;

    Ok(result)
}

async fn parse_extra_field(file: &mut ZipFileData) -> ZipResult<()> {
    use crate::types::{AesMode, AesVendorVersion};
    use std::io::Cursor;

    let mut reader = Cursor::new(&file.extra_field);

    while (reader.position() as usize) < file.extra_field.len() {
        let kind = reader.read_u16_le().await?;
        let len = reader.read_u16_le().await?;
        let mut len_left = len as i64;
        match kind {
            // Zip64 extended information extra field
            0x0001 => {
                if file.uncompressed_size == spec::ZIP64_BYTES_THR {
                    file.large_file = true;
                    file.uncompressed_size = reader.read_u64_le().await?;
                    len_left -= 8;
                }
                if file.compressed_size == spec::ZIP64_BYTES_THR {
                    file.large_file = true;
                    file.compressed_size = reader.read_u64_le().await?;
                    len_left -= 8;
                }
                if file.header_start == spec::ZIP64_BYTES_THR {
                    file.header_start = reader.read_u64_le().await?;
                    len_left -= 8;
                }
            }
            0x9901 => {
                // AES
                if len != 7 {
                    return Err(ZipError::UnsupportedArchive(
                        "AES extra data field has an unsupported length",
                    ));
                }
                let vendor_version = reader.read_u16_le().await?;
                let vendor_id = reader.read_u16_le().await?;
                let aes_mode = reader.read_u8().await?;
                let compression_method = reader.read_u16_le().await?;

                if vendor_id != 0x4541 {
                    return Err(ZipError::InvalidArchive("Invalid AES vendor"));
                }
                let vendor_version = match vendor_version {
                    0x0001 => AesVendorVersion::Ae1,
                    0x0002 => AesVendorVersion::Ae2,
                    _ => return Err(ZipError::InvalidArchive("Invalid AES vendor version")),
                };
                match aes_mode {
                    0x01 => file.aes_mode = Some((AesMode::Aes128, vendor_version)),
                    0x02 => file.aes_mode = Some((AesMode::Aes192, vendor_version)),
                    0x03 => file.aes_mode = Some((AesMode::Aes256, vendor_version)),
                    _ => return Err(ZipError::InvalidArchive("Invalid AES encryption strength")),
                };
                file.compression_method = {
                    #[allow(deprecated)]
                    CompressionMethod::from_u16(compression_method)
                };
            }
            _ => {
                // Other fields are ignored
            }
        }

        // We could also check for < 0 to check for errors
        if len_left > 0 {
            reader.seek(io::SeekFrom::Current(len_left)).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        compression::CompressionMethod,
        write::{FileOptions, ZipWriter},
    };

    use std::io::Cursor;

    #[tokio::test]
    async fn test_find_content() -> ZipResult<()> {
        let buf = Cursor::new(Vec::new());
        let buf = {
            use std::io::Write;
            let mut f = ZipWriter::new(buf);
            let options = FileOptions::default().compression_method(CompressionMethod::Stored);
            f.start_file("a/b.txt", options)?;
            f.write_all(b"hello\n")?;
            f.finish()?
        };
        let mut f = ZipArchive::new(buf).await?;

        assert_eq!(1, f.len());
        let data = Pin::new(&mut f).by_index(0).await?.data().clone();
        assert_eq!(b"a/b.txt", &data.file_name_raw[..]);

        let mut limited = find_content(&data, f.into_inner()).await?;

        let mut buf = String::new();
        std::io::Read::read_to_string(&mut limited, &mut buf)?;
        assert_eq!(&buf, "hello\n");

        let mut buf = String::new();
        let f = limited.into_inner();
        let mut limited = find_content(&data, f).await?;
        io::AsyncReadExt::read_to_string(&mut limited, &mut buf).await?;
        assert_eq!(&buf, "hello\n");

        Ok(())
    }

    #[tokio::test]
    async fn test_get_reader() -> ZipResult<()> {
        let buf = Cursor::new(Vec::new());
        let buf = {
            use std::io::Write;
            let mut f = ZipWriter::new(buf);
            let options = FileOptions::default().compression_method(CompressionMethod::Deflated);
            f.start_file("a/b.txt", options)?;
            f.write_all(b"hello\n")?;
            f.finish()?
        };
        let mut f = ZipArchive::new(buf).await?;

        assert_eq!(1, f.len());
        let data = Pin::new(&mut f).by_index(0).await?.data().clone();
        assert_eq!(data.crc32, 909783072);
        assert_eq!(b"a/b.txt", &data.file_name_raw[..]);

        let mut limited = get_reader(&data, f.into_inner()).await?;

        let mut buf = String::new();
        io::AsyncReadExt::read_to_string(&mut limited, &mut buf).await?;
        assert_eq!(&buf, "hello\n");

        Ok(())
    }
}
