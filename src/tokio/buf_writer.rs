use pin_project_lite::pin_project;
use tokio::io;

use std::{
    cmp,
    pin::Pin,
    slice,
    task::{ready, Context, Poll},
};

pub trait AsyncBufWrite: io::AsyncWrite {
    fn consume_mut(self: Pin<&mut Self>, amt: usize);

    fn poll_dump(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&mut [u8]>>;
}

// used by `BufReader` and `BufWriter`
// https://github.com/rust-lang/rust/blob/master/library/std/src/sys_common/io.rs#L1
const DEFAULT_BUF_SIZE: usize = 8 * 1024;

pin_project! {
    ///```
    /// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
    /// use zip::tokio::buf_writer::BufWriter;
    /// use tokio::io::AsyncWriteExt;
    /// use std::io::Cursor;
    ///
    /// let msg = "hello";
    /// let buf = Cursor::new(Vec::new());
    /// let mut buf_reader = BufWriter::new(buf);
    ///
    /// buf_reader.write_all(msg.as_bytes()).await?;
    /// let buf: Vec<u8> = buf_reader.into_inner().into_inner();
    /// let s = std::str::from_utf8(&buf).unwrap();
    /// assert_eq!(&s, &msg);
    /// # Ok(())
    /// # })}
    ///```
    pub struct BufWriter<W> {
        #[pin]
        pub(super) inner: W,
        pub(super) buf: Box<[u8]>,
        pub(super) pos: usize,
        pub(super) cap: usize,
    }
}

impl<W> BufWriter<W> {
    pub fn new(inner: W) -> Self {
        Self::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    pub fn with_capacity(capacity: usize, inner: W) -> Self {
        let buffer = vec![0; capacity];
        Self {
            inner,
            buf: buffer.into_boxed_slice(),
            pos: 0,
            cap: capacity,
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    #[inline]
    pub fn get_ref(&self) -> &W {
        &self.inner
    }

    #[inline]
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    #[inline]
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut W> {
        self.project().inner
    }

    pub fn into_inner(self) -> W {
        self.inner
    }

    #[inline]
    fn cur_len(&self) -> usize {
        debug_assert!(self.cap >= self.pos);
        self.cap - self.pos
    }

    #[inline]
    fn is_empty(&self) -> bool {
        self.cur_len() == 0
    }

    #[inline]
    pub fn buffer_mut(self: Pin<&mut Self>) -> &mut [u8] {
        let me = self.project();
        let pos = *me.pos;
        let cap = *me.cap;
        debug_assert!(pos <= cap);
        &mut me.buf[pos..cap]
    }

    #[inline]
    fn discard_buffer(self: Pin<&mut Self>) {
        let cap = self.capacity();
        let me = self.project();
        *me.pos = 0;
        *me.cap = cap;
    }

    #[inline]
    fn request_is_larger_than_buffer(&self, buf: &[u8]) -> bool {
        buf.len() >= self.capacity()
    }

    #[inline]
    fn should_bypass_buffer(&self, buf: &[u8]) -> bool {
        self.is_empty() && self.request_is_larger_than_buffer(buf)
    }
}

impl<W: io::AsyncWrite> BufWriter<W> {
    fn bypass_buffer(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let res = ready!(self.as_mut().get_pin_mut().poll_write(cx, buf));
        self.discard_buffer();
        Poll::Ready(res)
    }

    #[inline]
    fn reset_to_single_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<usize>> {
        let cap = self.capacity();
        let me = self.project();
        let buf: &[u8] = &me.buf;
        let len = ready!(me.inner.poll_write(cx, buf))?;
        *me.cap = cap;
        debug_assert!(cap >= len);
        *me.pos = cap - len;
        Poll::Ready(Ok(len))
    }
}

impl<W: io::AsyncWrite> io::AsyncWrite for BufWriter<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        if self.should_bypass_buffer(buf) {
            return self.bypass_buffer(cx, buf);
        }

        let rem = ready!(self.as_mut().poll_dump(cx))?;
        let amt = cmp::min(rem.len(), buf.len());
        rem[..amt].copy_from_slice(&buf[..amt]);
        self.consume_mut(amt);

        Poll::Ready(Ok(amt))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if !self.is_empty() {
            let flushed = ready!(self.as_mut().reset_to_single_write(cx))?;
            if flushed != 0 {
                return Poll::Pending;
            }
        }
        self.get_pin_mut().poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().poll_flush(cx))?;
        self.get_pin_mut().poll_shutdown(cx)
    }
}

impl<W: io::AsyncWrite> AsyncBufWrite for BufWriter<W> {
    fn consume_mut(self: Pin<&mut Self>, amt: usize) {
        let me = self.project();
        *me.pos = cmp::min(*me.pos + amt, *me.cap);
    }

    fn poll_dump(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&mut [u8]>> {
        if self.is_empty() {
            let len = ready!(self.as_mut().reset_to_single_write(cx))?;
            if len == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            debug_assert!(!self.is_empty());
        }
        Poll::Ready(Ok(self.buffer_mut()))
    }
}
