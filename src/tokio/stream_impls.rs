///```
/// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
/// use zip::tokio::{buf_writer::BufWriter, stream_impls::deflate::*};
/// use flate2::Compression;
/// use tokio::io::{self, AsyncReadExt, AsyncWriteExt, AsyncSeekExt};
/// use std::{io::Cursor, pin::Pin};
///
/// let msg = "hello";
/// let buf = Cursor::new(msg.as_bytes());
/// let c = Compression::default();
/// let mut def = Deflater::buffered_read(buf, c);
///
/// let mut out_inf = Inflater::buffered_write(Cursor::new(Vec::new()));
///
/// io::copy(&mut def, &mut out_inf).await?;
/// out_inf.flush().await?;
/// out_inf.shutdown().await?;
///
/// let mut final_buf: Cursor<Vec<u8>> =
///   out_inf.into_inner().into_inner();
/// final_buf.rewind().await?;
/// let mut s = String::new();
/// final_buf.read_to_string(&mut s).await?;
/// assert_eq!(&s[..msg.len()], msg);
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
        buf_writer::{AsyncBufWrite, BufWriter, NonEmptyReadSlice, NonEmptyWriteSlice},
    };

    use flate2::{
        Compress, CompressError, Compression, Decompress, DecompressError, FlushCompress,
        FlushDecompress, Status,
    };
    use pin_project_lite::pin_project;
    use tokio::io;

    use std::{
        cell, mem,
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
        ///```
        /// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
        /// use zip::tokio::{buf_writer::{AsyncBufWrite, BufWriter}, stream_impls::deflate::Writer};
        /// use flate2::Decompress;
        /// use tokio::io::{self, AsyncWriteExt};
        /// use std::io::Cursor;
        ///
        /// let msg: &[u8] = &[203, 72, 205, 201, 201, 7, 0];
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
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            debug_assert!(buf.len() > 0);

            loop {
                let mut write_buf: NonEmptyWriteSlice<'_, u8> =
                    ready!(self.pin_new_stream().poll_writable(cx))?;

                /* let mut me = self.as_mut().project(); */
                let readable_len: usize = self.inner.readable_data().len();
                dbg!(readable_len);

                let (status, num_read, _) = encode_frame_consume_inputs(
                    self.pin_new_state(),
                    /* &buf[..readable_len], */
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

        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
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
        /// let mut inf = Inflater::buffered_read(buf);
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
        /// let buf = Cursor::new(Vec::new());
        /// let mut inf = Inflater::buffered_write(buf);
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
            Inflater::new(BufWriter::with_capacity(
                NonZeroUsize::new(32 * 1024).unwrap(),
                inner,
            ))
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
        #[inline]
        fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
            Pin::new(&mut *self.project().inner).poll_fill_buf(cx)
        }

        #[inline]
        fn consume(self: Pin<&mut Self>, len: usize) {
            Pin::new(&mut *self.project().inner).consume(len)
        }
    }

    impl<S: AsyncBufWrite + Unpin> io::AsyncWrite for Inflater<S> {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            debug_assert!(buf.len() > 0);

            let mut me = self.project();

            loop {
                let mut rem = ready!(Pin::new(&mut *me.inner).poll_writable(cx))?;

                let before_in = me.transformer.total_in();
                let ret = me
                    .transformer
                    .decompress(buf, &mut *rem, FlushDecompress::None);
                let written = (me.transformer.total_in() - before_in) as usize;
                let is_stream_end = matches![ret, Ok(Status::StreamEnd)];

                if let Some(written) = NonZeroUsize::new(written) {
                    Pin::new(&mut *me.inner).consume_write(written);
                }

                match ret {
                    Ok(_) if written == 0 && !is_stream_end => (),
                    Ok(Status::Ok | Status::BufError | Status::StreamEnd) => {
                        return Poll::Ready(Ok(written));
                    }
                    Err(_) => {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "corrupt inflate stream",
                        )));
                    }
                }
            }
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let mut me = self.project();

            if let Some(ref mut write_buf) = Pin::new(&mut *me.inner).try_writable() {
                me.transformer
                    .decompress(&[], &mut *write_buf, FlushDecompress::Sync)
                    .unwrap();
            }

            loop {
                let mut rem = ready!(Pin::new(&mut *me.inner).poll_writable(cx))?;
                let before = me.transformer.total_out();
                me.transformer
                    .decompress(&[], &mut *rem, FlushDecompress::None)
                    .unwrap();

                let len = (me.transformer.total_out() - before) as usize;
                if let Some(len) = NonZeroUsize::new(len) {
                    Pin::new(&mut *me.inner).consume_write(len);
                } else {
                    break;
                }
            }

            Pin::new(&mut *me.inner).poll_flush(cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let mut me = self.project();

            loop {
                let mut rem = ready!(Pin::new(&mut *me.inner).poll_writable(cx))?;
                let before = me.transformer.total_out();
                me.transformer
                    .decompress(&[], &mut *rem, FlushDecompress::Finish)
                    .unwrap();

                let len = (me.transformer.total_out() - before) as usize;
                if let Some(len) = NonZeroUsize::new(len) {
                    Pin::new(&mut *me.inner).consume_write(len);
                } else {
                    break;
                }
            }

            Pin::new(&mut *me.inner).poll_shutdown(cx)
        }
    }

    impl<S: AsyncBufWrite + Unpin> AsyncBufWrite for Inflater<S> {
        #[inline]
        fn consume_read(self: Pin<&mut Self>, amt: NonZeroUsize) {
            Pin::new(&mut *self.project().inner).consume_read(amt);
        }
        #[inline]
        fn readable_data(&self) -> &[u8] {
            self.inner.readable_data()
        }
        #[inline]
        fn consume_write(self: Pin<&mut Self>, amt: NonZeroUsize) {
            Pin::new(&mut *self.project().inner).consume_write(amt);
        }
        #[inline]
        fn try_writable(self: Pin<&mut Self>) -> Option<NonEmptyWriteSlice<'_, u8>> {
            Pin::new(&mut *self.project().inner).try_writable()
        }
        #[inline]
        fn poll_writable(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<io::Result<NonEmptyWriteSlice<'_, u8>>> {
            Pin::new(&mut *self.project().inner).poll_writable(cx)
        }
        #[inline]
        fn reset(self: Pin<&mut Self>) {
            Pin::new(&mut *self.project().inner).reset();
        }
    }

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
        /// let mut def = Deflater::buffered_read(buf, c);
        ///
        /// let mut b = Vec::new();
        /// def.read_to_end(&mut b).await?;
        /// assert_eq!(&b, &[203, 72, 205, 201, 201, 7, 0]);
        /// # Ok(())
        /// # })}
        ///```
        ///
        /// Write:
        ///```
        /// # fn main() -> std::io::Result<()> { tokio_test::block_on(async {
        /// use zip::tokio::{stream_impls::deflate::Deflater, buf_writer::BufWriter};
        /// use flate2::Compression;
        /// use tokio::io::{self, AsyncWriteExt};
        /// use std::io::Cursor;
        ///
        /// let msg = "hello";
        /// let c = Compression::default();
        /// let mut def = Deflater::buffered_write(Cursor::new(Vec::new()), c);
        ///
        /// def.write_all(msg.as_bytes()).await?;
        /// def.flush().await?;
        /// def.shutdown().await?;
        /// let buf: Vec<u8> = def.into_inner().into_inner().into_inner();
        /// assert_eq!(&buf, &[203, 72, 205, 201, 201, 7, 0]);
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
        pub fn buffered_read(inner: S, compression: Compression) -> Deflater<BufReader<S>> {
            Deflater::new(BufReader::new(inner), compression)
        }
        pub fn buffered_write(inner: S, compression: Compression) -> Deflater<BufWriter<S>> {
            Deflater::new(BufWriter::new(inner), compression)
        }
    }

    impl<S: io::AsyncBufRead + Unpin> io::AsyncRead for Deflater<S> {
        fn poll_read(
            self: Pin<&mut Self>,
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

    impl<S: AsyncBufWrite + Unpin> io::AsyncWrite for Deflater<S> {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            debug_assert!(buf.len() > 0);

            let mut me = self.project();

            loop {
                let mut rem = ready!(Pin::new(&mut *me.inner).poll_writable(cx))?;
                dbg!(rem.len());

                let before_in = me.transformer.total_in();
                let before_out = me.transformer.total_out();
                let ret = me.transformer.compress(buf, &mut *rem, FlushCompress::None);
                let written = (me.transformer.total_in() - before_in) as usize;
                let read = (me.transformer.total_out() - before_out) as usize;
                let is_stream_end = matches![ret, Ok(Status::StreamEnd)];

                match ret {
                    Ok(_) if written == 0 && !is_stream_end => (),
                    Ok(Status::Ok | Status::BufError | Status::StreamEnd) => {
                        if let Some(read) = NonZeroUsize::new(read) {
                            Pin::new(&mut *me.inner).consume_read(read);
                        }
                        if let Some(written) = NonZeroUsize::new(written) {
                            Pin::new(&mut *me.inner).consume_write(written);
                        }
                        return Poll::Ready(Ok(written));
                    }
                    Err(_) => {
                        return Poll::Ready(Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "corrupt deflate stream",
                        )));
                    }
                }
            }
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let mut me = self.project();

            if let Some(ref mut write_buf) = Pin::new(&mut *me.inner).try_writable() {
                me.transformer
                    .compress(&[], &mut *write_buf, FlushCompress::Sync)
                    .unwrap();
            }

            loop {
                let mut rem = ready!(Pin::new(&mut *me.inner).poll_writable(cx))?;
                let before = me.transformer.total_out();
                me.transformer
                    .compress(&[], &mut *rem, FlushCompress::None)
                    .unwrap();

                let len = (me.transformer.total_out() - before) as usize;
                if let Some(len) = NonZeroUsize::new(len) {
                    Pin::new(&mut *me.inner).consume_write(len);
                } else {
                    break;
                }
            }

            Pin::new(&mut *me.inner).poll_flush(cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let mut me = self.project();

            loop {
                let mut rem = ready!(Pin::new(&mut *me.inner).poll_writable(cx))?;
                let before = me.transformer.total_out();
                me.transformer
                    .compress(&[], &mut *rem, FlushCompress::Finish)
                    .unwrap();

                let len = (me.transformer.total_out() - before) as usize;
                if let Some(len) = NonZeroUsize::new(len) {
                    Pin::new(&mut *me.inner).consume_write(len);
                } else {
                    break;
                }
            }

            Pin::new(&mut *me.inner).poll_shutdown(cx)
        }
    }

    impl<S: AsyncBufWrite + Unpin> AsyncBufWrite for Deflater<S> {
        #[inline]
        fn consume_read(self: Pin<&mut Self>, amt: NonZeroUsize) {
            Pin::new(&mut *self.project().inner).consume_read(amt);
        }
        #[inline]
        fn readable_data(&self) -> &[u8] {
            self.inner.readable_data()
        }
        #[inline]
        fn consume_write(self: Pin<&mut Self>, amt: NonZeroUsize) {
            Pin::new(&mut *self.project().inner).consume_write(amt);
        }
        #[inline]
        fn try_writable(self: Pin<&mut Self>) -> Option<NonEmptyWriteSlice<'_, u8>> {
            Pin::new(&mut *self.project().inner).try_writable()
        }
        #[inline]
        fn poll_writable(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<io::Result<NonEmptyWriteSlice<'_, u8>>> {
            Pin::new(&mut *self.project().inner).poll_writable(cx)
        }
        #[inline]
        fn reset(self: Pin<&mut Self>) {
            Pin::new(&mut *self.project().inner).reset();
        }
    }
}
