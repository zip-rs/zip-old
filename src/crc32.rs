//! Helper module to compute a CRC32 checksum

use std::marker::Unpin;
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use crc32fast::Hasher;
use tokio::io;

/// Reader that validates the CRC32 when it reaches the EOF.
pub struct Crc32Reader<R> {
    inner: R,
    hasher: Hasher,
    check: u32,
    /// Signals if `inner` stores aes encrypted data.
    /// AE-2 encrypted data doesn't use crc and sets the value to 0.
    ae2_encrypted: bool,
}

impl<R> Crc32Reader<R> {
    /// Get a new Crc32Reader which checks the inner reader against checksum.
    /// The check is disabled if `ae2_encrypted == true`.
    pub(crate) fn new(inner: R, checksum: u32, ae2_encrypted: bool) -> Crc32Reader<R> {
        Crc32Reader {
            inner,
            hasher: Hasher::new(),
            check: checksum,
            ae2_encrypted,
        }
    }

    #[inline]
    fn check_matches(&self) -> bool {
        self.check == self.hasher.clone().finalize()
    }

    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: std::io::Read> std::io::Read for Crc32Reader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let invalid_check = !buf.is_empty() && !self.check_matches() && !self.ae2_encrypted;

        let count = match self.inner.read(buf) {
            Ok(0) if invalid_check => {
                return Err(io::Error::new(io::ErrorKind::Other, "Invalid checksum"))
            }
            Ok(n) => n,
            Err(e) => return Err(e),
        };
        self.hasher.update(&buf[0..count]);
        Ok(count)
    }
}

impl<R: io::AsyncRead + Unpin> io::AsyncRead for Crc32Reader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let start = buf.filled().len();

        let s = self.get_mut();

        if let Err(e) = ready!(Pin::new(&mut s.inner).poll_read(cx, buf)) {
            return Poll::Ready(Err(e));
        }

        let written: usize = buf.filled().len() - start;
        if written == 0 {
            return Poll::Ready(if !s.ae2_encrypted && !s.check_matches() {
                Err(io::Error::new(io::ErrorKind::Other, "Invalid checksum"))
            } else {
                Ok(())
            });
        }

        s.hasher.update(&buf.filled()[start..]);
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::result::ZipResult;

    #[test]
    fn test_empty_reader() {
        use std::io::Read;
        let data: &[u8] = b"";
        let mut buf = [0; 1];

        let mut reader = Crc32Reader::new(data, 0, false);
        assert_eq!(reader.read(&mut buf).unwrap(), 0);

        let mut reader = Crc32Reader::new(data, 1, false);
        assert!(reader
            .read(&mut buf)
            .unwrap_err()
            .to_string()
            .contains("Invalid checksum"));
    }

    #[tokio::test]
    async fn test_tokio_empty_reader() -> ZipResult<()> {
        use tokio::io::AsyncReadExt;
        let data: &[u8] = b"";
        let mut buf = [0; 1];

        let mut reader = Crc32Reader::new(data, 0, false);
        assert_eq!(reader.read(&mut buf).await?, 0);

        let mut reader = Crc32Reader::new(data, 1, false);
        assert!(reader
            .read(&mut buf)
            .await
            .unwrap_err()
            .to_string()
            .contains("Invalid checksum"));
        Ok(())
    }

    #[test]
    fn test_byte_by_byte() {
        use std::io::Read;
        let data: &[u8] = b"1234";
        let mut buf = [0; 1];

        let mut reader = Crc32Reader::new(data, 0x9be3e0a3, false);
        assert_eq!(reader.read(&mut buf).unwrap(), 1);
        assert_eq!(reader.read(&mut buf).unwrap(), 1);
        assert_eq!(reader.read(&mut buf).unwrap(), 1);
        assert_eq!(reader.read(&mut buf).unwrap(), 1);
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
        // Can keep reading 0 bytes after the end
        assert_eq!(reader.read(&mut buf).unwrap(), 0);
    }

    #[tokio::test]
    async fn test_tokio_byte_by_byte() -> ZipResult<()> {
        use tokio::io::AsyncReadExt;
        let data: &[u8] = b"1234";
        let mut buf = [0; 1];

        let mut reader = Crc32Reader::new(data, 0x9be3e0a3, false);
        assert_eq!(reader.read(&mut buf).await?, 1);
        assert_eq!(reader.read(&mut buf).await?, 1);
        assert_eq!(reader.read(&mut buf).await?, 1);
        assert_eq!(reader.read(&mut buf).await?, 1);
        assert_eq!(reader.read(&mut buf).await?, 0);
        // Can keep reading 0 bytes after the end
        assert_eq!(reader.read(&mut buf).await?, 0);

        Ok(())
    }

    #[test]
    fn test_zero_read() {
        use std::io::Read;
        let data: &[u8] = b"1234";
        let mut buf = [0; 5];

        let mut reader = Crc32Reader::new(data, 0x9be3e0a3, false);
        assert_eq!(reader.read(&mut buf[..0]).unwrap(), 0);
        assert_eq!(reader.read(&mut buf).unwrap(), 4);
    }

    #[tokio::test]
    async fn test_tokio_zero_read() -> ZipResult<()> {
        use tokio::io::AsyncReadExt;
        let data: &[u8] = b"1234";
        let mut buf = [0; 5];

        let mut reader = Crc32Reader::new(data, 0x9be3e0a3, false);
        assert_eq!(reader.read(&mut buf[..0]).await?, 0);
        assert_eq!(reader.read(&mut buf).await?, 4);

        Ok(())
    }
}
