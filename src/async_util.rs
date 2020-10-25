use futures::AsyncRead;
use pin_project::pin_project;
use tokio::io::AsyncRead as TokioAsyncRead;

/// Wrapper for converting between tokio and futures async read types
#[pin_project]
pub(crate) struct Compat<T>(#[pin] T);

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
            .poll_read(cx, buf.initialize_unfilled())
            .map(|r| r.map(|n| buf.set_filled(n)))
    }
}

pub(crate) trait CompatExt<T>: Sized {
    fn compat(self) -> Compat<Self> {
        Compat(self)
    }

    fn compat_mut(&mut self) -> Compat<&mut Self> {
        Compat(self)
    }
}

impl<T: AsyncRead> CompatExt<T> for T {}
