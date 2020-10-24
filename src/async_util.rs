use std::{io::Read, pin::Pin};

use futures::{AsyncRead, AsyncReadExt};
use pin_project::pin_project;
use tokio::io::AsyncRead as TokioAsyncRead;

/// Wrapper for converting between tokio and futures async read types
#[pin_project]
pub struct Compat<T>(#[pin] T);

impl<T> Compat<T> {
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: AsyncRead> TokioAsyncRead for Compat<T> {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.project()
            .0
            // TODO: Is the initialized section what we want?
            .poll_read(cx, buf.initialized_mut())
            .map(|r| r.map(|_| ()))
    }
}

pub trait CompatExt<T>: Sized {
    fn compat(self) -> Compat<Self> {
        Compat(self)
    }

    fn compat_mut(&mut self) -> Compat<&mut Self> {
        Compat(self)
    }
}

impl<T: AsyncRead> CompatExt<T> for T {}

/// Adapter to block on an AsyncRead type to implement Read.
///
/// This should be removed once zip-rs's internals are completely converted.
#[pin_project]
pub struct Block<T>(#[pin] T);

impl<T: AsyncRead> Read for Pin<&mut Block<T>> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        async fn future<T: AsyncRead>(
            mut reader: Pin<&mut T>,
            buf: &mut [u8],
        ) -> Result<usize, std::io::Error> {
            reader.read(buf).await
        }

        futures::executor::block_on(future(self.project().0, buf))
    }
}

pub trait BlockExt<T>: Sized {
    fn block(self) -> Block<Self> {
        Block(self)
    }
}

impl<T: AsyncRead> BlockExt<T> for T {}
