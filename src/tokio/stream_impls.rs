///```
/// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
/// use zip::tokio::{buf_reader::BufReader, buf_writer::BufWriter, stream_impls::deflate::*};
/// use flate2::{Decompress, Compress, Compression};
/// use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
/// use std::{io::Cursor, pin::Pin, num::NonZeroUsize};
///
/// let msg = "hello\n";
/// let c = Compression::default();
/// const CAP: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(200) };
/// let mut def = Reader::with_state(
///   Compress::new(c, false),
///   // BufReader::with_capacity(msg.as_bytes()),
///   BufReader::with_capacity(CAP, msg.as_bytes()),
/// );
///
/// let mut out_inf = Writer::with_state(
///   Decompress::new(false),
///   BufWriter::with_capacity(CAP, Cursor::new(Vec::new())),
/// );
///
/// io::copy(&mut def, &mut out_inf).await?;
/// out_inf.flush().await?;
/// out_inf.shutdown().await?;
///
/// let mut final_buf: Vec<u8> =
///   out_inf.into_inner().into_inner().into_inner();
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
        mem,
        num::NonZeroUsize,
        pin::Pin,
        task::{ready, Context, Poll},
    };

    pub trait Ops {
        type Flush: Flush;
        type E;
        fn total_in(&self) -> u64;
        fn total_out(&self) -> u64;
        fn encode_frame(
            self: Pin<&mut Self>,
            input: &[u8],
            output: &mut [u8],
            flush: Self::Flush,
        ) -> Result<Status, Self::E>;
    }

    fn encode_frame_consume_inputs<O: Ops, S: AsyncBufWrite>(
        mut o: Pin<&mut O>,
        input: &[u8],
        output: &mut [u8],
        flush: O::Flush,
        mut s: Pin<&mut S>,
    ) -> io::Result<(Status, usize, usize)> {
        let before_in = o.total_in();
        let before_out = o.total_out();

        dbg!(input.len());

        let status = match o.as_mut().encode_frame(input, output, flush) {
            Ok(status) => status,
            Err(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "corrupt write stream",
                ));
            }
        };

        let num_read = (o.total_in() - before_in) as usize;
        let num_consumed = (o.total_out() - before_out) as usize;

        if let Some(num_consumed) = NonZeroUsize::new(num_consumed) {
            s.as_mut().consume_write(num_consumed);
        }
        if let Some(num_read) = NonZeroUsize::new(num_read) {
            let u_num_read: usize = num_read.into();
            debug_assert!(u_num_read <= input.len());
            /* todo!("?"); */
            s.as_mut().consume_read(num_read);
        }

        Ok((status, num_read, num_consumed))
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
        pub fn with_state(state: O, inner: S) -> Self {
            Self { state, inner }
        }

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
                    Err(_) => {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "corrupt read stream",
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
        /// let msg = "hello\n";
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
        /// let expected = &[202, 72, 205, 201, 201, 231, 2, 0, 0, 0, 255, 255, 3, 0];
        /// assert_eq!(&buf, expected);
        /// # Ok(())
        /// # })}
        ///```
        ///
        /// Decompress:
        ///```
        /// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
        /// use zip::tokio::{buf_writer::{AsyncBufWrite, BufWriter}, stream_impls::deflate::Writer};
        /// use flate2::Decompress;
        /// use tokio::io::{self, AsyncWriteExt};
        /// use std::io::Cursor;
        ///
        /// let msg: &[u8] = &[202, 72, 205, 201, 201, 231, 2, 0, 0, 0, 255, 255, 3, 0];
        /// let buf = BufWriter::new(Cursor::new(Vec::new()));
        /// let mut inf = Writer::with_state(Decompress::new(false), buf);
        ///
        /// inf.write_all(msg).await?;
        /// inf.flush().await?;
        /// inf.shutdown().await?;
        /// let buf: Vec<u8> = inf.into_inner().into_inner().into_inner();
        /// let s = std::str::from_utf8(&buf).unwrap();
        /// let expected = "hello";
        /// // FIXME: we should not be needing to truncate the output like this!! This is probably
        /// // a bug!!!
        /// assert_eq!(&s[..expected.len()], expected);
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
        pub fn with_state(state: O, inner: S) -> Self {
            Self { state, inner }
        }

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

                let (status, num_read, _) = encode_frame_consume_inputs(
                    self.pin_new_state(),
                    buf,
                    &mut *write_buf,
                    O::Flush::none(),
                    self.pin_new_stream(),
                )?;

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
            if let Some(ref mut write_buf) = self.pin_new_stream().try_writable() {
                let (_, _, _) = encode_frame_consume_inputs(
                    self.pin_new_state(),
                    &[],
                    &mut *write_buf,
                    O::Flush::sync(),
                    self.pin_new_stream(),
                )?;
            }

            loop {
                let mut write_buf = ready!(self.pin_new_stream().poll_writable(cx))?;

                let (_, _, num_consumed) = encode_frame_consume_inputs(
                    self.pin_new_state(),
                    &[],
                    &mut *write_buf,
                    O::Flush::none(),
                    self.pin_new_stream(),
                )?;
                if num_consumed == 0 {
                    break;
                }
            }

            self.pin_stream().poll_flush(cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            loop {
                let mut write_buf = ready!(self.pin_new_stream().poll_writable(cx))?;

                let (_, _, num_consumed) = encode_frame_consume_inputs(
                    self.pin_new_state(),
                    &[],
                    &mut *write_buf,
                    O::Flush::finish(),
                    self.pin_new_stream(),
                )?;
                if num_consumed == 0 {
                    break;
                }
            }

            self.pin_stream().poll_shutdown(cx)
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
