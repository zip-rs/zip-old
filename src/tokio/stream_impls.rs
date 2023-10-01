///```
/// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
/// use zip::tokio::stream_impls::deflate::*;
/// use flate2::Compression;
/// use tokio::io::{self, AsyncReadExt, AsyncWriteExt, AsyncSeekExt};
/// use std::{io::Cursor, pin::Pin};
///
/// let msg = "hello";
/// let buf = Cursor::new(msg.as_bytes());
/// let c = Compression::default();
/// let def = Deflater::new(buf, c);
/// let mut inf = Inflater::new(def);
///
/// let out_buf: Cursor<Vec<u8>> = Cursor::new(Vec::new());
/// let out_def = Deflater::new(out_buf, c);
/// let mut out_inf = Inflater::new(out_def);
///
/// io::copy(&mut inf, &mut out_inf).await?;
///
/// let mut final_buf: Cursor<Vec<u8>> =
///   out_inf.into_inner().into_inner();
/// final_buf.rewind().await?;
/// let mut s = String::new();
/// final_buf.read_to_string(&mut s).await?;
/// assert_eq!(&s, &"hello");
/// # Ok(())
/// # })}
///```
#[cfg(any(
    feature = "deflate",
    feature = "deflate-miniz",
    feature = "deflate-zlib"
))]
pub mod deflate {
    use crate::tokio::{
        buf_reader::BufReader,
        buf_writer::{AsyncBufWrite, BufWriter},
    };

    use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};
    use pin_project_lite::pin_project;
    use tokio::io;

    use std::{
        io::IoSlice,
        pin::Pin,
        task::{ready, Context, Poll},
    };

    pin_project! {
        /// Read:
        ///```
        /// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
        /// use zip::tokio::{stream_impls::deflate::Inflater};
        /// use tokio::io::{self, AsyncReadExt};
        /// use std::io::Cursor;
        ///
        /// let msg: &[u8] = &[203, 72, 205, 201, 201, 7, 0];
        /// let buf = Cursor::new(msg);
        /// let mut inf = Inflater::new(buf);
        ///
        /// let mut s = String::new();
        /// inf.read_to_string(&mut s).await?;
        /// assert_eq!(&s, "hello");
        /// # Ok(())
        /// # })}
        ///```
        ///
        /// Write:
        ///```
        /// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
        /// use zip::tokio::{stream_impls::deflate::Inflater};
        /// use tokio::io::{self, AsyncWriteExt};
        /// use std::io::Cursor;
        ///
        /// let msg: &[u8] = &[203, 72, 205, 201, 201, 7, 0];
        /// let buf = Cursor::new(Vec::<u8>::new());
        /// let mut inf = Inflater::new(buf);
        ///
        /// inf.write_all(msg).await?;
        /// inf.flush().await?;
        /// let buf: Vec<u8> = inf.into_inner().into_inner();
        /// assert_eq!(&buf, &[]);
        /// let s = std::str::from_utf8(&buf).unwrap();
        /// assert_eq!(&s, &"hello");
        /// # Ok(())
        /// # })}
        ///```
        pub struct Inflater<S> {
            inner: S,
            #[pin]
            transformer: Decompress,
        }
    }

    impl<S> Inflater<S> {
        pub fn new(inner: S) -> Self {
            Self {
                inner,
                transformer: Decompress::new(false),
            }
        }

        pub fn into_inner(self) -> S {
            self.inner
        }
    }

    impl<S> Inflater<S> {
        pub fn buffered_read(inner: S) -> Inflater<BufReader<S>> {
            Inflater::new(BufReader::with_capacity(32 * 1024, inner))
        }

        pub fn buffered_write(inner: S) -> Inflater<BufWriter<S>> {
            Inflater::new(BufWriter::with_capacity(32 * 1024, inner))
        }
    }

    impl<S: io::AsyncBufRead + Unpin> io::AsyncRead for Inflater<S> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            debug_assert!(buf.remaining() > 0);

            let mut me = self.project();

            loop {
                let input = ready!(Pin::new(&mut *me.inner).poll_fill_buf(cx))?;

                let eof = input.is_empty();
                let before_out = me.transformer.total_out();
                let before_in = me.transformer.total_in();
                let flush = if eof {
                    FlushDecompress::Finish
                } else {
                    FlushDecompress::None
                };

                let ret = me
                    .transformer
                    .decompress(input, buf.initialize_unfilled(), flush);

                let num_read = me.transformer.total_out() - before_out;
                let num_consumed = me.transformer.total_in() - before_in;

                buf.set_filled(buf.filled().len() + num_read as usize);
                Pin::new(&mut *me.inner).consume(num_consumed as usize);

                match ret {
                    Ok(Status::Ok | Status::BufError) if num_read == 0 && !eof => (),
                    Ok(Status::Ok | Status::BufError | Status::StreamEnd) => {
                        return Poll::Ready(Ok(()))
                    }
                    Err(_) => {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "corrupt deflate stream",
                        )))
                    }
                }
            }
        }
    }

    impl<S: io::AsyncBufRead + Unpin> io::AsyncBufRead for Inflater<S> {
        fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
            Pin::new(&mut *self.project().inner).poll_fill_buf(cx)
        }

        fn consume(self: Pin<&mut Self>, len: usize) {
            Pin::new(&mut *self.project().inner).consume(len)
        }
    }

    /* impl<S: AsyncBufWrite + Unpin> io::AsyncWrite for Inflater<S> { */
    /*     fn poll_write( */
    /*         self: Pin<&mut Self>, */
    /*         cx: &mut Context<'_>, */
    /*         buf: &[u8], */
    /*     ) -> Poll<io::Result<usize>> { */
    /*         debug_assert!(buf.len() > 0); */

    /*         let mut me = self.project(); */

    /*         loop { */
    /*             let output = Pin::new(&mut *me.inner).remaining_buffer_mut(); */
    /*         } */

    /*         Pin::new(&mut self.project().inner).poll_write(cx, buf) */
    /*     } */

    /*     fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> { */
    /*         Pin::new(&mut self.project().inner).poll_flush(cx) */
    /*     } */

    /*     fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> { */
    /*         Pin::new(&mut self.project().inner).poll_shutdown(cx) */
    /*     } */
    /* } */

    pin_project! {
        /// Read:
        ///```
        /// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
        /// use zip::tokio::{stream_impls::deflate::Deflater};
        /// use flate2::Compression;
        /// use tokio::io::{self, AsyncReadExt, AsyncWriteExt};
        /// use std::io::Cursor;
        ///
        /// let msg = "hello";
        /// let buf = Cursor::new(msg.as_bytes());
        /// let c = Compression::default();
        /// let mut def = Deflater::new(buf, c);
        ///
        /// let mut b = Vec::new();
        /// def.read_to_end(&mut b).await?;
        /// assert_eq!(&b, &[203, 72, 205, 201, 201, 7, 0]);
        /// # Ok(())
        /// # })}
        ///```
        pub struct Deflater<S> {
            inner: S,
            #[pin]
            transformer: Compress,
        }
    }

    impl<S> Deflater<S> {
        pub fn new(inner: S, compression: Compression) -> Self {
            Self {
                inner,
                transformer: Compress::new(compression, false),
            }
        }

        pub fn into_inner(self) -> S {
            self.inner
        }
    }

    impl<S> Deflater<S> {
        pub fn buffered(inner: S, compression: Compression) -> Deflater<BufReader<S>> {
            Deflater::new(BufReader::new(inner), compression)
        }
    }

    impl<S: io::AsyncBufRead + Unpin> io::AsyncRead for Deflater<S> {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            debug_assert!(buf.remaining() > 0);

            let mut me = self.project();

            loop {
                let input = ready!(Pin::new(&mut *me.inner).poll_fill_buf(cx))?;

                dbg!(&input);
                let eof = input.is_empty();
                dbg!(eof);
                let before_out = me.transformer.total_out();
                let before_in = me.transformer.total_in();
                let flush = if eof {
                    FlushCompress::Finish
                } else {
                    FlushCompress::None
                };

                let ret = me
                    .transformer
                    .as_mut()
                    .compress(input, buf.initialize_unfilled(), flush);
                dbg!(&ret);

                let num_read = me.transformer.total_out() - before_out;
                let num_consumed = me.transformer.total_in() - before_in;
                dbg!(num_read);
                dbg!(num_consumed);

                buf.set_filled(buf.filled().len() + num_read as usize);
                Pin::new(&mut *me.inner).consume(num_consumed as usize);

                match ret {
                    Ok(Status::Ok | Status::BufError) if num_read == 0 && !eof => (),
                    Ok(Status::Ok | Status::BufError | Status::StreamEnd) => {
                        return Poll::Ready(Ok(()))
                    }
                    Err(_) => {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "corrupt deflate stream",
                        )))
                    }
                }
            }
        }
    }

    impl<S: io::AsyncBufRead + Unpin> io::AsyncBufRead for Deflater<S> {
        fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
            Pin::new(&mut *self.project().inner).poll_fill_buf(cx)
        }

        fn consume(self: Pin<&mut Self>, len: usize) {
            Pin::new(&mut *self.project().inner).consume(len)
        }
    }

    /* impl<S: io::AsyncWrite + Unpin> io::AsyncWrite for Deflater<S> { */
    /*     fn poll_write( */
    /*         self: Pin<&mut Self>, */
    /*         cx: &mut Context<'_>, */
    /*         buf: &[u8], */
    /*     ) -> Poll<io::Result<usize>> { */
    /*         Pin::new(&mut self.project().inner).poll_write(cx, buf) */
    /*     } */

    /*     fn poll_write_vectored( */
    /*         self: Pin<&mut Self>, */
    /*         cx: &mut Context<'_>, */
    /*         bufs: &[IoSlice<'_>], */
    /*     ) -> Poll<io::Result<usize>> { */
    /*         Pin::new(&mut self.project().inner).poll_write_vectored(cx, bufs) */
    /*     } */

    /*     fn is_write_vectored(&self) -> bool { */
    /*         self.inner.is_write_vectored() */
    /*     } */

    /*     fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> { */
    /*         Pin::new(&mut self.project().inner).poll_flush(cx) */
    /*     } */

    /*     fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> { */
    /*         Pin::new(&mut self.project().inner).poll_shutdown(cx) */
    /*     } */
    /* } */
}
