/* Taken from https://docs.rs/tokio/latest/src/tokio/io/util/buf_reader.rs.html to fix a few
 * issues. */

use pin_project_lite::pin_project;
#[cfg(doc)]
use tokio::io::AsyncRead;
use tokio::io::{self, AsyncBufRead};

use std::{
    cmp, fmt, num,
    pin::Pin,
    task::{ready, Context, Poll},
};

// used by `BufReader` and `BufWriter`
// https://github.com/rust-lang/rust/blob/master/library/std/src/sys_common/io.rs#L1
const DEFAULT_BUF_SIZE: num::NonZeroUsize = unsafe { num::NonZeroUsize::new_unchecked(8 * 1024) };

pin_project! {
    /// The `BufReader` struct adds buffering to any reader.
    ///
    /// It can be excessively inefficient to work directly with a [`AsyncRead`]
    /// instance. A `BufReader` performs large, infrequent reads on the underlying
    /// [`AsyncRead`] and maintains an in-memory buffer of the results.
    ///
    /// `BufReader` can improve the speed of programs that make *small* and
    /// *repeated* read calls to the same file or network socket. It does not
    /// help when reading very large amounts at once, or reading just one or a few
    /// times. It also provides no advantage when reading from a source that is
    /// already in memory, like a `Vec<u8>`.
    ///
    /// When the `BufReader` is dropped, the contents of its buffer will be
    /// discarded. Creating multiple instances of a `BufReader` on the same
    /// stream can cause data loss.
    ///
    ///```
    /// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
    /// use zip::tokio::buf_reader::BufReader;
    /// use tokio::io::AsyncReadExt;
    /// use std::io::Cursor;
    ///
    /// let msg = "hello";
    /// let buf = Cursor::new(msg.as_bytes());
    /// let mut buf_reader = BufReader::new(buf);
    ///
    /// let mut s = String::new();
    /// buf_reader.read_to_string(&mut s).await?;
    /// assert_eq!(&s, &msg);
    /// # Ok(())
    /// # })}
    ///```
    pub struct BufReader<R> {
        #[pin]
        inner: R,
        buf: Box<[u8]>,
        pos: usize,
        cap: usize,
    }
}

impl<R> BufReader<R> {
    /// Creates a new `BufReader` with a default buffer capacity. The default is currently 8 KB,
    /// but may change in the future.
    pub fn new(inner: R) -> Self {
        Self::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    /// Creates a new `BufReader` with the specified buffer capacity.
    pub fn with_capacity(capacity: num::NonZeroUsize, inner: R) -> Self {
        let buffer = vec![0; capacity.into()];
        Self {
            inner,
            buf: buffer.into_boxed_slice(),
            pos: 0,
            cap: 0,
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    /// Gets a reference to the underlying reader.
    ///
    /// It is inadvisable to directly read from the underlying reader.
    #[inline]
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Gets a mutable reference to the underlying reader.
    ///
    /// It is inadvisable to directly read from the underlying reader.
    #[inline]
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Gets a pinned mutable reference to the underlying reader.
    ///
    /// It is inadvisable to directly read from the underlying reader.
    #[inline]
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut R> {
        self.project().inner
    }

    /// Consumes this `BufReader`, returning the underlying reader.
    ///
    /// Note that any leftover data in the internal buffer is lost.
    pub fn into_inner(self) -> R {
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

    /// Returns a reference to the internally buffered data.
    ///
    /// Unlike `fill_buf`, this will not attempt to fill the buffer if it is empty.
    #[inline]
    pub fn buffer(&self) -> &[u8] {
        &self.buf[self.pos..self.cap]
    }

    #[inline]
    fn buffer_pin<'a>(self: Pin<&'a mut Self>) -> &'a [u8] {
        let me = self.project();
        let pos: usize = *me.pos;
        let cap: usize = *me.cap;
        debug_assert!(pos <= cap);
        &me.buf[pos..cap]
    }

    /// Invalidates all data in the internal buffer.
    #[inline]
    fn discard_buffer(self: Pin<&mut Self>) {
        let me = self.project();
        *me.pos = 0;
        *me.cap = 0;
    }

    #[inline]
    fn request_is_larger_than_buffer(&self, buf: &io::ReadBuf<'_>) -> bool {
        buf.remaining() >= self.capacity()
    }

    #[inline]
    fn should_bypass_buffer(&self, buf: &io::ReadBuf<'_>) -> bool {
        self.is_empty() && self.request_is_larger_than_buffer(buf)
    }
}

impl<R: io::AsyncRead> BufReader<R> {
    fn bypass_buffer(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let res = ready!(self.as_mut().get_pin_mut().poll_read(cx, buf));
        self.discard_buffer();
        Poll::Ready(res)
    }

    #[inline]
    fn reset_to_single_read(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let me = self.project();
        let mut buf = io::ReadBuf::new(me.buf);
        ready!(me.inner.poll_read(cx, &mut buf))?;
        /* debug_assert!(buf.filled().len() > 0); */
        *me.cap = buf.filled().len();
        *me.pos = 0;
        Poll::Ready(Ok(()))
    }
}

impl<R: io::AsyncRead> io::AsyncRead for BufReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // If we don't have any buffered data and we're doing a massive read
        // (larger than our internal buffer), bypass our internal buffer
        // entirely.
        if self.should_bypass_buffer(buf) {
            return self.bypass_buffer(cx, buf);
        }
        let rem = ready!(self.as_mut().poll_fill_buf(cx))?;
        let amt = cmp::min(rem.len(), buf.remaining());
        buf.put_slice(&rem[..amt]);
        self.consume(amt);
        Poll::Ready(Ok(()))
    }
}

impl<R: io::AsyncRead> io::AsyncBufRead for BufReader<R> {
    fn poll_fill_buf(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        if self.is_empty() {
            ready!(self.as_mut().reset_to_single_read(cx))?;
        }
        let buf: &[u8] = self.buffer_pin();
        Poll::Ready(Ok(buf))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        if amt == 0 {
            return;
        }
        let me = self.project();
        *me.pos = cmp::min(*me.pos + amt, *me.cap);
    }
}

impl<R: fmt::Debug> fmt::Debug for BufReader<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BufReader")
            .field("reader", &self.inner)
            .field(
                "buffer",
                &format_args!("{}/{}", self.cap - self.pos, self.buf.len()),
            )
            .finish()
    }
}
