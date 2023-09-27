#![allow(missing_docs)]

#[cfg(any(
    feature = "deflate",
    feature = "deflate-miniz",
    feature = "deflate-zlib"
))]
pub mod deflate {
    use flate2::{Decompress, FlushDecompress, Status};
    use tokio::io;

    use std::{
        pin::Pin,
        task::{ready, Context, Poll},
    };

    pub struct Deflater<S> {
        inner: S,
        transformer: Decompress,
    }

    impl<S> Deflater<S> {
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

    impl<S: io::AsyncBufRead + Unpin> io::AsyncRead for Deflater<S> {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            debug_assert!(buf.remaining() > 0);

            let s = self.get_mut();

            let input = match ready!(Pin::new(&mut s.inner).poll_fill_buf(cx)) {
                Ok(input) => input,
                Err(e) => {
                    return Poll::Ready(Err(e));
                }
            };

            let eof = input.is_empty();
            let before_out = s.transformer.total_out();
            let before_in = s.transformer.total_in();
            let flush = if eof {
                FlushDecompress::Finish
            } else {
                FlushDecompress::None
            };

            let ret = s
                .transformer
                .decompress(input, buf.initialize_unfilled(), flush);

            let num_read = s.transformer.total_out() - before_out;
            let num_consumed = s.transformer.total_in() - before_in;

            buf.set_filled(buf.filled().len() + num_read as usize);
            Pin::new(&mut s.inner).consume(num_consumed as usize);

            match ret {
                Ok(Status::Ok | Status::BufError) if num_read == 0 && !eof => Poll::Pending,
                Ok(Status::Ok | Status::BufError | Status::StreamEnd) => Poll::Ready(Ok(())),
                Err(_) => Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "corrupt deflate stream",
                ))),
            }
        }
    }
}
