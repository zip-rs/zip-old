/// As 2 separate read calls:
///```
/// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
/// use zip::tokio::{buf_reader::BufReader, buf_writer::BufWriter, stream_impls::deflate::*};
/// use flate2::{Decompress, Compress, Compression};
/// use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
/// use std::{io::Cursor, pin::Pin, num::NonZeroUsize};
///
/// let msg = "hello";
/// let c = Compression::default();
/// let buf_reader = BufReader::new(Cursor::new(msg.as_bytes()));
/// let mut def = Reader::with_state(Compress::new(c, false), buf_reader);
///
/// let mut buf = Vec::new();
/// {
///   use tokio::io::{AsyncReadExt, AsyncSeekExt};
///   def.read_to_end(&mut buf).await?;
///   assert_eq!(&buf, &[203, 72, 205, 201, 201, 7, 0]);
/// }
///
/// let mut out_inf = Writer::with_state(
///   Decompress::new(false),
///   BufWriter::new(Cursor::new(Vec::new())),
/// );
/// {
///   use tokio::io::{AsyncReadExt, AsyncSeekExt};
///   out_inf.write_all(&buf).await?;
///   out_inf.flush().await?;
///   out_inf.shutdown().await?;
///   let buf: Vec<u8> = out_inf.into_inner().into_inner().into_inner();
///   assert_eq!(&buf, b"hello");
/// }
/// # Ok(())
/// # })}
///```
///
/// Or, within a single `tokio::io::copy{,_buf}()`:
///```
/// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
/// use zip::tokio::{buf_reader::BufReader, buf_writer::BufWriter, stream_impls::deflate::*};
/// use flate2::{Decompress, Compress, Compression};
/// use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
/// use std::{io::Cursor, pin::Pin, num::NonZeroUsize};
///
/// let msg = "hello";
/// let c = Compression::default();
/// let buf_reader = BufReader::new(Cursor::new(msg.as_bytes()));
/// let mut def = Reader::with_state(Compress::new(c, false), buf_reader);
///
/// let mut out_inf = Writer::with_state(
///   Decompress::new(false),
///   BufWriter::new(Cursor::new(Vec::new())),
/// );
///
/// io::copy(&mut def, &mut out_inf).await?;
/// out_inf.flush().await?;
/// out_inf.shutdown().await?;
///
/// let mut final_buf: Vec<u8> = out_inf.into_inner().into_inner().into_inner();
/// let s = std::str::from_utf8(&final_buf).unwrap();
/// assert_eq!(&s, &msg);
/// # Ok(())
/// # })}
///```
#[cfg(any(
    feature = "deflate",
    feature = "deflate-miniz",
    feature = "deflate-zlib"
))]
pub mod deflate {
    use crate::tokio::buf_writer::{AsyncBufWrite, NonEmptyWriteSlice};

    use flate2::{
        Compress, CompressError, Decompress, DecompressError, FlushCompress, FlushDecompress,
        Status,
    };
    use pin_project_lite::pin_project;
    use tokio::io;

    use std::{
        fmt, mem,
        num::NonZeroUsize,
        pin::Pin,
        task::{ready, Context, Poll},
    };

    pub trait Ops {
        type Flush: Flush;
        type E: fmt::Display;
        fn total_in(&self) -> u64;
        fn total_out(&self) -> u64;
        fn encode_frame(
            self: Pin<&mut Self>,
            input: &[u8],
            output: &mut [u8],
            flush: Self::Flush,
        ) -> Result<Status, Self::E>;
    }

    pin_project! {
        /// Compress:
        ///```
        /// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
        /// use zip::tokio::{stream_impls::deflate::Reader, buf_reader::BufReader};
        /// use flate2::{Compress, Compression};
        /// use tokio::io::{self, AsyncReadExt, AsyncBufRead};
        /// use std::io::Cursor;
        ///
        /// let msg = "hello";
        /// let buf = BufReader::new(Cursor::new(msg.as_bytes()));
        /// let c = Compression::default();
        /// let mut def = Reader::with_state(Compress::new(c, false), buf);
        ///
        /// let mut b = Vec::new();
        /// def.read_to_end(&mut b).await?;
        /// assert_eq!(&b, &[203, 72, 205, 201, 201, 7, 0]);
        /// # Ok(())
        /// # })}
        ///```
        ///
        /// Decompress:
        ///```
        /// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
        /// use zip::tokio::{stream_impls::deflate::Reader, buf_reader::BufReader};
        /// use flate2::Decompress;
        /// use tokio::io::{self, AsyncReadExt};
        /// use std::io::Cursor;
        ///
        /// let msg: &[u8] = &[203, 72, 205, 201, 201, 7, 0];
        /// let buf = BufReader::new(Cursor::new(msg));
        /// let mut inf = Reader::with_state(Decompress::new(false), buf);
        ///
        /// let mut s = String::new();
        /// inf.read_to_string(&mut s).await?;
        /// assert_eq!(&s, "hello");
        /// # Ok(())
        /// # })}
        ///```
        pub struct Reader<O, S> {
            #[pin]
            state: O,
            #[pin]
            inner: S,
        }
    }

    impl<O, S> Reader<O, S> {
        pub fn pin_state(self: Pin<&mut Self>) -> Pin<&mut O> {
            self.project().state
        }

        pub fn pin_stream(self: Pin<&mut Self>) -> Pin<&mut S> {
            self.project().inner
        }

        pub fn into_inner(self) -> S {
            self.inner
        }
    }

    impl<O: Ops, S: io::AsyncBufRead> Reader<O, S> {
        pub fn with_state(state: O, inner: S) -> Self {
            Self { state, inner }
        }
    }

    impl<O: Ops, S: io::AsyncBufRead> io::AsyncBufRead for Reader<O, S> {
        fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
            self.pin_stream().poll_fill_buf(cx)
        }
        fn consume(self: Pin<&mut Self>, amt: usize) {
            self.pin_stream().consume(amt);
        }
    }

    impl<O: Ops, S: io::AsyncBufRead> io::AsyncRead for Reader<O, S> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            debug_assert!(buf.remaining() > 0);

            let mut me = self.project();

            loop {
                let input: &[u8] = ready!(me.inner.as_mut().poll_fill_buf(cx))?;
                let eof: bool = input.is_empty();
                let (before_out, before_in): (u64, u64) =
                    { (me.state.total_out(), me.state.total_in()) };
                let flush = if eof {
                    O::Flush::finish()
                } else {
                    O::Flush::none()
                };

                let ret = me
                    .state
                    .as_mut()
                    .encode_frame(input, buf.initialize_unfilled(), flush);

                let (num_read, num_consumed): (usize, usize) = (
                    (me.state.total_out() - before_out) as usize,
                    (me.state.total_in() - before_in) as usize,
                );

                buf.set_filled(buf.filled().len() + num_read as usize);
                me.inner.as_mut().consume(num_consumed);

                match ret {
                    Ok(Status::Ok | Status::BufError) if num_read == 0 && !eof => (),
                    Ok(Status::Ok | Status::BufError | Status::StreamEnd) => {
                        return Poll::Ready(Ok(()));
                    }
                    Err(e) => {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("corrupt read stream({})", e),
                        )))
                    }
                }
            }
        }
    }

    pin_project! {
        /// Compress:
        ///```
        /// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
        /// use zip::tokio::{stream_impls::deflate::Writer, buf_writer::BufWriter};
        /// use flate2::{Compress, Compression};
        /// use tokio::io::{self, AsyncWriteExt};
        /// use std::io::Cursor;
        ///
        /// let msg = "hello";
        /// let c = Compression::default();
        /// let mut def = Writer::with_state(
        ///   Compress::new(c, false),
        ///   BufWriter::new(Cursor::new(Vec::new())),
        /// );
        ///
        /// def.write_all(msg.as_bytes()).await?;
        /// def.flush().await?;
        /// def.shutdown().await?;
        /// let buf: Vec<u8> = def.into_inner().into_inner().into_inner();
        /// let expected = &[202, 72, 205, 201, 201, 7, 0, 0, 0, 255, 255, 3, 0];
        /// assert_eq!(&buf, expected);
        /// # Ok(())
        /// # })}
        ///```
        ///
        /// Decompress:
        ///```
        /// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
        /// use zip::tokio::{buf_writer::BufWriter, stream_impls::deflate::Writer};
        /// use flate2::Decompress;
        /// use tokio::io::{self, AsyncWriteExt};
        /// use std::{cmp, io::Cursor};
        ///
        /// let msg: &[u8] = &[202, 72, 205, 201, 201, 231, 2, 0, 0, 0, 255, 255, 3, 0];
        /// let buf = BufWriter::new(Cursor::new(Vec::new()));
        /// let mut inf = Writer::with_state(Decompress::new(false), buf);
        ///
        /// inf.write_all(msg).await?;
        /// inf.flush().await?;
        /// inf.shutdown().await?;
        /// let buf: Vec<u8> = inf.into_inner().into_inner().into_inner();
        /// let expected = b"hello\n";
        /// assert_eq!(&buf, &expected);
        /// # Ok(())
        /// # })}
        ///```
        pub struct Writer<O, S> {
            #[pin]
            state: O,
            #[pin]
            inner: S,
        }
    }

    impl<O, S> Writer<O, S> {
        fn pin_new_state(&self) -> Pin<&mut O> {
            unsafe {
                let state: *mut O = mem::transmute(&self.state);
                Pin::new_unchecked(&mut *state)
            }
        }

        pub fn pin_state(self: Pin<&mut Self>) -> Pin<&mut O> {
            self.project().state
        }

        fn pin_new_stream(&self) -> Pin<&mut S> {
            unsafe {
                let inner: *mut S = mem::transmute(&self.inner);
                Pin::new_unchecked(&mut *inner)
            }
        }

        pub fn pin_stream(self: Pin<&mut Self>) -> Pin<&mut S> {
            self.project().inner
        }

        pub fn into_inner(self) -> S {
            self.inner
        }
    }

    impl<O: Ops, S: AsyncBufWrite> Writer<O, S> {
        pub fn with_state(state: O, inner: S) -> Self {
            Self { state, inner }
        }
    }

    impl<O: Ops, S: AsyncBufWrite> io::AsyncWrite for Writer<O, S> {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            debug_assert!(buf.len() > 0);

            loop {
                let mut write_buf: NonEmptyWriteSlice<'_, u8> =
                    ready!(self.pin_new_stream().poll_writable(cx))?;

                let before_in = self.state.total_in();
                let before_out = self.state.total_out();

                let status = self
                    .pin_new_state()
                    .encode_frame(buf, &mut *write_buf, O::Flush::none())
                    .map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("corrupt write stream({})/1", e),
                        )
                    })?;

                let num_read = (self.state.total_in() - before_in) as usize;
                let num_consumed = (self.state.total_out() - before_out) as usize;

                if let Some(num_consumed) = NonZeroUsize::new(num_consumed) {
                    self.pin_new_stream().consume_write(num_consumed);
                }

                match (num_read, status) {
                    (0, Status::Ok | Status::BufError) => {
                        continue;
                    }
                    (n, _) => {
                        return Poll::Ready(Ok(n));
                    }
                }
            }
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            {
                let mut write_buf: NonEmptyWriteSlice<'_, u8> =
                    ready!(self.pin_new_stream().poll_writable(cx))?;

                let before_out = self.state.total_out();

                self.pin_new_state()
                    .encode_frame(&[], &mut *write_buf, O::Flush::sync())
                    .map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("corrupt write stream({})/2", e),
                        )
                    })?;

                let num_consumed = (self.state.total_out() - before_out) as usize;
                if let Some(num_consumed) = NonZeroUsize::new(num_consumed) {
                    self.pin_new_stream().consume_write(num_consumed);
                }
            }

            loop {
                let mut write_buf = ready!(self.pin_new_stream().poll_writable(cx))?;

                let before_out = self.state.total_out();

                self.pin_new_state()
                    .encode_frame(&[], &mut *write_buf, O::Flush::none())
                    .map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("corrupt write stream({})/3", e),
                        )
                    })?;

                let num_consumed = (self.state.total_out() - before_out) as usize;
                if let Some(num_consumed) = NonZeroUsize::new(num_consumed) {
                    self.pin_new_stream().consume_write(num_consumed);
                } else {
                    break;
                }
            }

            self.pin_stream().poll_flush(cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            loop {
                let mut write_buf = ready!(self.pin_new_stream().poll_writable(cx))?;

                let before_out = self.state.total_out();

                self.pin_new_state()
                    .encode_frame(&[], &mut *write_buf, O::Flush::finish())
                    .map_err(|e| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("corrupt write stream({})/4", e),
                        )
                    })?;

                let num_consumed = (self.state.total_out() - before_out) as usize;
                if let Some(num_consumed) = NonZeroUsize::new(num_consumed) {
                    self.pin_new_stream().consume_write(num_consumed);
                } else {
                    break;
                }
            }

            self.pin_stream().poll_shutdown(cx)
        }
    }

    impl<O: Ops, S: AsyncBufWrite> AsyncBufWrite for Writer<O, S> {
        #[inline]
        fn consume_read(self: Pin<&mut Self>, amt: NonZeroUsize) {
            self.pin_stream().consume_read(amt);
        }
        #[inline]
        fn readable_data(&self) -> &[u8] {
            self.inner.readable_data()
        }

        #[inline]
        fn consume_write(self: Pin<&mut Self>, amt: NonZeroUsize) {
            self.pin_stream().consume_write(amt);
        }
        #[inline]
        fn try_writable(self: Pin<&mut Self>) -> Option<NonEmptyWriteSlice<'_, u8>> {
            self.pin_stream().try_writable()
        }
        #[inline]
        fn poll_writable(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<io::Result<NonEmptyWriteSlice<'_, u8>>> {
            self.pin_stream().poll_writable(cx)
        }

        #[inline]
        fn reset(self: Pin<&mut Self>) {
            self.pin_stream().reset();
        }
    }

    impl Ops for Compress {
        type Flush = FlushCompress;
        type E = CompressError;
        #[inline]
        fn total_in(&self) -> u64 {
            self.total_in()
        }
        #[inline]
        fn total_out(&self) -> u64 {
            self.total_out()
        }
        #[inline]
        fn encode_frame(
            mut self: Pin<&mut Self>,
            input: &[u8],
            output: &mut [u8],
            flush: Self::Flush,
        ) -> Result<Status, Self::E> {
            self.as_mut().compress(input, output, flush)
        }
    }

    impl Ops for Decompress {
        type Flush = FlushDecompress;
        type E = DecompressError;
        #[inline]
        fn total_in(&self) -> u64 {
            self.total_in()
        }
        #[inline]
        fn total_out(&self) -> u64 {
            self.total_out()
        }
        #[inline]
        fn encode_frame(
            mut self: Pin<&mut Self>,
            input: &[u8],
            output: &mut [u8],
            flush: Self::Flush,
        ) -> Result<Status, Self::E> {
            self.as_mut().decompress(input, output, flush)
        }
    }

    pub trait Flush {
        fn none() -> Self;
        fn sync() -> Self;
        fn finish() -> Self;
    }

    impl Flush for FlushCompress {
        fn none() -> Self {
            Self::None
        }
        fn sync() -> Self {
            Self::Sync
        }
        fn finish() -> Self {
            Self::Finish
        }
    }

    impl Flush for FlushDecompress {
        fn none() -> Self {
            Self::None
        }
        fn sync() -> Self {
            Self::Sync
        }
        fn finish() -> Self {
            Self::Finish
        }
    }
}
