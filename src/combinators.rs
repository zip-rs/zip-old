#![allow(missing_docs)]

use tokio::io;

use std::{
    marker::Unpin,
    pin::Pin,
    task::{Context, Poll},
};

pub mod stream_adaptors {
    use super::*;
    use crate::channels::{futurized::*, *};

    use std::{cmp, future::Future, mem, slice, sync::Arc, task::ready};

    use bytes::{BufMut, BytesMut};
    use parking_lot::{lock_api::ArcRwLockUpgradableReadGuard, Mutex, RwLock};
    use tokio::{
        sync::{self, oneshot},
        task,
    };

    pub trait KnownExpanse {
        /* TODO: make this have a parameterized Self::Index type, used e.g. with RangeInclusive or
         * something. */
        fn full_length(&self) -> usize;
    }

    ///```
    /// # fn main() -> zip::result::ZipResult<()> { tokio_test::block_on(async {
    /// use std::io::{SeekFrom, Cursor, prelude::*};
    /// use tokio::io;
    /// use zip::combinators::Limiter;
    ///
    /// let mut buf = Cursor::new(Vec::new());
    /// buf.write_all(b"hello\n")?;
    /// buf.seek(SeekFrom::Start(1))?;
    ///
    /// let mut limited = Limiter::take(buf, 3);
    /// let mut s = String::new();
    /// limited.read_to_string(&mut s)?;
    /// assert_eq!(s, "ell");
    ///
    /// io::AsyncSeekExt::seek(&mut limited, SeekFrom::End(-1)).await?;
    /// s.clear();
    /// io::AsyncReadExt::read_to_string(&mut limited, &mut s).await?;
    /// assert_eq!(s, "l");
    /// # Ok(())
    /// # })}
    ///```
    #[derive(Debug, Clone)]
    pub struct Limiter<S> {
        pub max_len: usize,
        pub internal_pos: usize,
        pub source_stream: S,
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

        #[inline]
        fn convert_seek_request_to_relative(&self, op: io::SeekFrom) -> i64 {
            let cur = self.internal_pos as u64;
            let new_point = cmp::min(
                self.max_len as u64,
                match op {
                    io::SeekFrom::Start(new_point) => new_point,
                    io::SeekFrom::End(from_end) => {
                        cmp::max(0, self.max_len as i64 + from_end) as u64
                    }
                    io::SeekFrom::Current(from_cur) => cmp::max(0, cur as i64 + from_cur) as u64,
                },
            );
            let diff = new_point as i64 - cur as i64;
            diff
        }
    }

    impl<S> KnownExpanse for Limiter<S> {
        #[inline]
        fn full_length(&self) -> usize {
            self.max_len
        }
    }

    impl<S: std::io::Read> std::io::Read for Limiter<S> {
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

    impl<S: std::io::Seek> std::io::Seek for Limiter<S> {
        fn seek(&mut self, op: io::SeekFrom) -> io::Result<u64> {
            let diff = self.convert_seek_request_to_relative(op);
            /* FIXME: what to do if the source stream doesn't seek back or forward as far as we
             * asked it to? Should we be checking the result of .seek()? here? */
            self.internal_pos = ((self.internal_pos as i64) + diff) as usize;
            self.source_stream.seek(io::SeekFrom::Current(diff))
        }
    }

    impl<S: std::io::Write> std::io::Write for Limiter<S> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            debug_assert!(!buf.is_empty());

            let num_bytes_to_write: usize = self.limit_length(buf.len());
            if num_bytes_to_write == 0 {
                return Ok(0);
            }

            let bytes_written = self.source_stream.write(&buf[..num_bytes_to_write])?;
            if bytes_written > 0 {
                self.push_cursor(bytes_written);
            }
            Ok(bytes_written)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.source_stream.flush()
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

    impl<S: io::AsyncSeek + Unpin> io::AsyncSeek for Limiter<S> {
        fn start_seek(self: Pin<&mut Self>, op: io::SeekFrom) -> io::Result<()> {
            let diff = self.convert_seek_request_to_relative(op);
            let s = self.get_mut();
            s.internal_pos = ((s.internal_pos as i64) + diff) as usize;
            Pin::new(&mut s.source_stream).start_seek(io::SeekFrom::Current(diff))
        }
        fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
            let s = self.get_mut();
            Pin::new(&mut s.source_stream).poll_complete(cx)
        }
    }

    impl<S: io::AsyncWrite + Unpin> io::AsyncWrite for Limiter<S> {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            debug_assert!(!buf.is_empty());

            let num_bytes_to_write: usize = self.limit_length(buf.len());
            if num_bytes_to_write == 0 {
                return Poll::Ready(Ok(0));
            }

            let s = self.get_mut();
            match Pin::new(&mut s.source_stream).poll_write(cx, &buf[..num_bytes_to_write]) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(x) => Poll::Ready(x.map(|bytes_written| {
                    assert!(bytes_written <= num_bytes_to_write);
                    if bytes_written > 0 {
                        s.push_cursor(bytes_written);
                    }
                    bytes_written
                })),
            }
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let s = self.get_mut();
            Pin::new(&mut s.source_stream).poll_flush(cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let s = self.get_mut();
            Pin::new(&mut s.source_stream).poll_shutdown(cx)
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
    /// let mut f = zip::combinators::ReadAdapter::new(buf);
    /// let mut buf: Vec<u8> = Vec::new();
    /// f.read_to_end(&mut buf).await?;
    /// assert_eq!(&buf, b"hello\n");
    /// # Ok(())
    /// # })}
    ///```
    pub struct ReadAdapter<S> {
        inner: Arc<sync::Mutex<Option<(S, oneshot::Sender<io::Result<()>>)>>>,
        rx: oneshot::Receiver<io::Result<()>>,
        ring: RingFuturized,
    }
    impl<S> ReadAdapter<S> {
        pub fn new(inner: S, ring: Ring) -> Self {
            let (tx, rx) = oneshot::channel::<io::Result<()>>();
            Self {
                inner: Arc::new(sync::Mutex::new(Some((inner, tx)))),
                rx,
                ring: RingFuturized::wrap_ring(ring),
            }
        }

        pub fn into_inner(self) -> S {
            let (inner, tx) = task::block_in_place(move || self.inner.blocking_lock_owned())
                .take()
                .expect("inner stream already taken");
            tx.send(Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "inner stream was taken",
            )))
            .unwrap();
            inner
        }
    }

    impl<S: std::io::Read + Unpin + Send + 'static> io::AsyncRead for ReadAdapter<S> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            debug_assert!(buf.remaining() > 0);

            let s = self.get_mut();

            if let Poll::Ready(read_data) = s.ring.poll_read(cx, buf.remaining()) {
                debug_assert!(!read_data.is_empty());
                buf.put(&**read_data);
                return Poll::Ready(Ok(()));
            }

            if let Poll::Ready(result) = Pin::new(&mut s.rx).poll(cx) {
                return Poll::Ready(
                    result.expect("sender should not have been dropped without sending!"),
                );
            }

            let mut write_data = ready!(s.ring.poll_write(cx, buf.remaining()));
            if let Ok(mut inner) = s.inner.clone().try_lock_owned() {
                if inner.is_none() {
                    Poll::Ready(Ok(()))
                } else {
                    task::spawn_blocking(move || {
                        match inner.as_mut().unwrap().0.read(&mut write_data) {
                            Err(e) => {
                                if e.kind() == io::ErrorKind::Interrupted {
                                    write_data.truncate(0);
                                } else {
                                    let (_, tx) = inner.take().unwrap();
                                    tx.send(Err(e)).unwrap();
                                }
                            }
                            Ok(n) => {
                                if n == 0 {
                                    let (_, tx) = inner.take().unwrap();
                                    tx.send(Ok(())).unwrap();
                                } else {
                                    write_data.truncate(n);
                                }
                            }
                        }
                    });
                    Poll::Pending
                }
            } else {
                let inner = s.inner.clone();
                task::spawn(async move {
                    let mut inner = inner.lock_owned().await;
                    if inner.is_none() {
                        return;
                    }
                    task::spawn_blocking(move || {
                        match inner.as_mut().unwrap().0.read(&mut write_data) {
                            Err(e) => {
                                if e.kind() == io::ErrorKind::Interrupted {
                                    write_data.truncate(0);
                                } else {
                                    let (_, tx) = inner.take().unwrap();
                                    tx.send(Err(e)).unwrap();
                                }
                            }
                            Ok(n) => {
                                write_data.truncate(n);
                            }
                        }
                    });
                });
                Poll::Pending
            }
        }
    }
}
pub use stream_adaptors::{Limiter, ReadAdapter};

mod file_adaptors {
    use super::*;

    use std::io::Cursor;

    use tokio::fs;

    #[derive(Debug)]
    pub enum FixedLengthFile<F> {
        Immediate(Cursor<Box<[u8]>>),
        Paging(Limiter<F>),
    }

    impl<F> stream_adaptors::KnownExpanse for FixedLengthFile<F> {
        #[inline]
        fn full_length(&self) -> usize {
            match self {
                Self::Immediate(cursor) => cursor.get_ref().len(),
                Self::Paging(limited) => limited.full_length(),
            }
        }
    }

    impl FixedLengthFile<fs::File> {
        pub async fn into_sync(self) -> FixedLengthFile<std::fs::File> {
            match self {
                Self::Immediate(cursor) => FixedLengthFile::Immediate(cursor),
                Self::Paging(Limiter {
                    max_len,
                    internal_pos,
                    source_stream,
                }) => FixedLengthFile::Paging(Limiter {
                    max_len,
                    internal_pos,
                    source_stream: source_stream.into_std().await,
                }),
            }
        }
    }

    impl FixedLengthFile<std::fs::File> {
        pub fn into_async(self) -> FixedLengthFile<fs::File> {
            match self {
                Self::Immediate(cursor) => FixedLengthFile::Immediate(cursor),
                Self::Paging(Limiter {
                    max_len,
                    internal_pos,
                    source_stream,
                }) => FixedLengthFile::Paging(Limiter {
                    max_len,
                    internal_pos,
                    source_stream: fs::File::from_std(source_stream),
                }),
            }
        }
    }

    /* impl fmt::Display for FixedLengthFile { */
    /*     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { */
    /*         let len = self.len(); */
    /*         match self { */
    /*             Self::Immediate(arc, pos) => match str::from_utf8(arc.read().as_ref()) { */
    /*                 Ok(s) => write!(f, "Immediate(@{})[{}](\"{}\")", pos, s.len(), s), */
    /*                 Err(_) => write!(f, "Immediate[{}](<binary>)", len), */
    /*                 /\* Err(_) => write!( *\/ */
    /*                 /\*     f, *\/ */
    /*                 /\*     "Immediate(@{})[{}](<binary> = \"{}\")", *\/ */
    /*                 /\*     pos, *\/ */
    /*                 /\*     arc.read().unwrap().len(), *\/ */
    /*                 /\*     String::from_utf8_lossy(arc.read().unwrap().as_ref()), *\/ */
    /*                 /\* ), *\/ */
    /*             }, */
    /*             Self::Paging(_, path, len) => write!(f, "Paging[{}]({})", len, path.display()), */
    /*         } */
    /*     } */
    /* } */

    impl<F: std::io::Read> std::io::Read for FixedLengthFile<F> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            debug_assert!(!buf.is_empty());
            match self {
                Self::Immediate(ref mut cursor) => cursor.read(buf),
                Self::Paging(ref mut limited) => limited.read(buf),
            }
        }
    }

    impl<F: std::io::Seek> std::io::Seek for FixedLengthFile<F> {
        fn seek(&mut self, op: io::SeekFrom) -> io::Result<u64> {
            match self {
                Self::Immediate(ref mut cursor) => cursor.seek(op),
                Self::Paging(ref mut limited) => limited.seek(op),
            }
        }
    }

    impl<F: std::io::Write> std::io::Write for FixedLengthFile<F> {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            match self {
                Self::Immediate(ref mut cursor) => cursor.write(buf),
                Self::Paging(ref mut limited) => limited.write(buf),
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            match self {
                Self::Immediate(ref mut cursor) => cursor.flush(),
                Self::Paging(ref mut limited) => limited.flush(),
            }
        }
    }

    impl<F: io::AsyncRead + Unpin> io::AsyncRead for FixedLengthFile<F> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            debug_assert!(buf.remaining() > 0);
            match self.get_mut() {
                Self::Immediate(ref mut cursor) => Pin::new(cursor).poll_read(cx, buf),
                Self::Paging(ref mut limited) => Pin::new(limited).poll_read(cx, buf),
            }
        }
    }

    impl<F: io::AsyncSeek + Unpin> io::AsyncSeek for FixedLengthFile<F> {
        fn start_seek(self: Pin<&mut Self>, op: io::SeekFrom) -> io::Result<()> {
            match self.get_mut() {
                Self::Immediate(ref mut cursor) => Pin::new(cursor).start_seek(op),
                Self::Paging(ref mut limited) => Pin::new(limited).start_seek(op),
            }
        }
        fn poll_complete(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
            match self.get_mut() {
                Self::Immediate(ref mut cursor) => Pin::new(cursor).poll_complete(cx),
                Self::Paging(ref mut limited) => Pin::new(limited).poll_complete(cx),
            }
        }
    }

    impl<F: io::AsyncWrite + Unpin> io::AsyncWrite for FixedLengthFile<F> {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            debug_assert!(!buf.is_empty());
            match self.get_mut() {
                Self::Immediate(ref mut cursor) => Pin::new(cursor).poll_write(cx, buf),
                Self::Paging(ref mut limited) => Pin::new(limited).poll_write(cx, buf),
            }
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.get_mut() {
                Self::Immediate(ref mut cursor) => Pin::new(cursor).poll_flush(cx),
                Self::Paging(ref mut limited) => Pin::new(limited).poll_flush(cx),
            }
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.get_mut() {
                Self::Immediate(ref mut cursor) => Pin::new(cursor).poll_shutdown(cx),
                Self::Paging(ref mut limited) => Pin::new(limited).poll_shutdown(cx),
            }
        }
    }
}
