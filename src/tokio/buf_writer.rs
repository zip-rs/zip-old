use pin_project_lite::pin_project;
use tokio::io;

use std::{
    cell, cmp, mem,
    num::NonZeroUsize,
    ops,
    pin::Pin,
    task::{ready, Context, Poll},
};

pub mod slices {
    use std::{cmp, mem, num::NonZeroUsize, ops, pin::Pin};

    #[derive(Debug, Copy, Clone)]
    pub struct NonEmptyReadSlice<'a, T> {
        data: &'a [T],
    }

    impl<'a, T> NonEmptyReadSlice<'a, T> {
        pub fn new(data: &'a [T]) -> Option<Self> {
            NonZeroUsize::new(data.len()).map(|_| Self { data })
        }

        #[inline]
        pub fn len(&self) -> NonZeroUsize {
            unsafe { NonZeroUsize::new_unchecked(self.data.len()) }
        }

        #[inline]
        pub fn maybe_uninit(&self) -> &'a [mem::MaybeUninit<T>] {
            unsafe { mem::transmute(&*self.data) }
        }
    }

    impl<'a, T> ops::Deref for NonEmptyReadSlice<'a, T> {
        type Target = [T];

        #[inline]
        fn deref(&self) -> &[T] {
            &self.data
        }
    }

    #[derive(Debug)]
    pub struct NonEmptyWriteSlice<'a, T> {
        data: &'a mut [T],
    }

    impl<'a, T> ops::Deref for NonEmptyWriteSlice<'a, T> {
        type Target = [T];

        #[inline]
        fn deref(&self) -> &[T] {
            &self.data
        }
    }

    impl<'a, T> ops::DerefMut for NonEmptyWriteSlice<'a, T> {
        #[inline]
        fn deref_mut(&mut self) -> &mut [T] {
            &mut self.data
        }
    }

    impl<'a, T> NonEmptyWriteSlice<'a, T> {
        pub fn new(data: &'a mut [T]) -> Option<Self> {
            NonZeroUsize::new(data.len()).map(|_| Self { data })
        }

        #[inline]
        pub fn len(&self) -> NonZeroUsize {
            unsafe { NonZeroUsize::new_unchecked(self.data.len()) }
        }

        #[inline]
        pub fn maybe_uninit(self: Pin<&'a mut Self>) -> Pin<&'a mut [mem::MaybeUninit<T>]> {
            unsafe { self.map_unchecked_mut(|s| mem::transmute(&mut *s.data)) }
        }
    }

    impl<'a, T: Copy> NonEmptyWriteSlice<'a, T> {
        pub fn copy_from_slice(
            self: Pin<&'a mut Self>,
            src: NonEmptyReadSlice<'a, T>,
        ) -> NonZeroUsize {
            let amt = cmp::min(self.len(), src.len());
            let dst: &'a mut [mem::MaybeUninit<T>] =
                unsafe { self.maybe_uninit().get_unchecked_mut() };
            let src: &'a [mem::MaybeUninit<T>] = src.maybe_uninit();
            dst[..amt.get()].copy_from_slice(&src[..amt.get()]);
            amt
        }
    }
}
pub use slices::{NonEmptyReadSlice, NonEmptyWriteSlice};

pub trait AsyncBufWrite: io::AsyncWrite {
    fn consume_read(self: Pin<&mut Self>, amt: NonZeroUsize);
    fn readable_data(&self) -> &[u8];

    fn consume_write(self: Pin<&mut Self>, amt: NonZeroUsize);
    fn try_writable(self: Pin<&mut Self>) -> Option<NonEmptyWriteSlice<'_, u8>>;
    fn poll_writable(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<NonEmptyWriteSlice<'_, u8>>>;

    fn reset(self: Pin<&mut Self>);
}

// used by `BufReader` and `BufWriter`
// https://github.com/rust-lang/rust/blob/master/library/std/src/sys_common/io.rs#L1
const DEFAULT_BUF_SIZE: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(8 * 1024) };

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
    /// buf_reader.flush().await?;
    /// buf_reader.shutdown().await?;
    /// let buf: Vec<u8> = buf_reader.into_inner().into_inner();
    /// let s = std::str::from_utf8(&buf).unwrap();
    /// assert_eq!(&s, &msg);
    /// # Ok(())
    /// # })}
    ///```
    pub struct BufWriter<W> {
        #[pin]
        inner: W,
        buf: Box<[u8]>,
        read_end: usize,
        write_end: usize,
    }
}

impl<W> BufWriter<W> {
    pub fn new(inner: W) -> Self {
        Self::with_capacity(DEFAULT_BUF_SIZE, inner)
    }

    pub fn with_capacity(capacity: NonZeroUsize, inner: W) -> Self {
        let buffer = vec![0; capacity.get()];
        Self {
            inner,
            buf: buffer.into_boxed_slice(),
            read_end: 0,
            write_end: 0,
        }
    }

    #[inline]
    pub fn capacity(&self) -> NonZeroUsize {
        unsafe { NonZeroUsize::new_unchecked(self.buf.len()) }
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
}

impl<W: io::AsyncWrite> BufWriter<W> {
    fn flush_one_readable(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        assert!(!self.readable_data().is_empty());
        dbg!(self.readable_data());

        let mut me = self.as_mut().project();
        let read_buf: &[u8] = &me.buf[*me.read_end..*me.write_end];
        match NonZeroUsize::new(ready!(me.inner.poll_write(cx, read_buf))?) {
            None => {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
            }
            Some(read) => {
                eprintln!("read = {}", read);
                self.consume_read(read);
            }
        }

        Poll::Ready(Ok(()))
    }

    fn flush_readable(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        while !self.readable_data().is_empty() {
            ready!(self.as_mut().flush_one_readable(cx))?;
        }
        Poll::Ready(Ok(()))
    }
}

impl<W: io::AsyncWrite> AsyncBufWrite for BufWriter<W> {
    #[inline]
    fn consume_read(self: Pin<&mut Self>, amt: NonZeroUsize) {
        let n = dbg!(self.readable_data().len());
        dbg!(amt.get());
        debug_assert!(self.readable_data().len() >= amt.get());
        let me = self.project();
        /* *me.read_end += cmp::min(amt.get(), n); */
        *me.read_end += amt.get();
    }

    #[inline]
    fn readable_data(&self) -> &[u8] {
        debug_assert!(self.read_end <= self.write_end);
        debug_assert!(self.write_end <= self.buf.len());
        dbg!(self.write_end - self.read_end);
        &self.buf[self.read_end..self.write_end]
    }

    #[inline]
    fn consume_write(self: Pin<&mut Self>, amt: NonZeroUsize) {
        debug_assert!(self.capacity().get() - self.write_end >= amt.get());
        let me = self.project();
        *me.write_end += amt.get();
    }

    #[inline]
    fn try_writable(self: Pin<&mut Self>) -> Option<NonEmptyWriteSlice<'_, u8>> {
        if self.write_end == self.buf.len() {
            return None;
        }
        let write_end = self.write_end;
        NonEmptyWriteSlice::new(&mut self.project().buf[write_end..])
    }

    fn poll_writable(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<NonEmptyWriteSlice<'_, u8>>> {
        let s = cell::UnsafeCell::new(self);
        if let Some(write_buf) = unsafe { &mut *s.get() }.as_mut().try_writable() {
            return Poll::Ready(Ok(write_buf));
        }

        ready!(unsafe { &mut *s.get() }.as_mut().flush_readable(cx))?;

        unsafe { &mut *s.get() }.as_mut().reset();

        Poll::Ready(Ok(s.into_inner().try_writable().unwrap()))
    }

    #[inline]
    fn reset(self: Pin<&mut Self>) {
        let me = self.project();
        *me.read_end = 0;
        *me.write_end = 0;
    }
}

impl<W: io::AsyncWrite> io::AsyncWrite for BufWriter<W> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let buf = NonEmptyReadSlice::new(buf).unwrap();
        let mut rem: NonEmptyWriteSlice<'_, u8> = ready!(self.as_mut().poll_writable(cx))?;

        let amt = unsafe { Pin::new_unchecked(&mut rem) }.copy_from_slice(buf);
        dbg!(amt);
        self.as_mut().consume_write(amt);

        Poll::Ready(Ok(amt.get()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().flush_readable(cx))?;
        self.get_pin_mut().poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        ready!(self.as_mut().poll_flush(cx))?;
        self.get_pin_mut().poll_shutdown(cx)
    }
}
