use crate::{
    spec,
    tokio::{
        buf_writer::BufWriter,
        combinators::{IntoInner, Limiter},
        stream_impls::deflate,
    },
    types::ZipFileData,
    write::ZipWriterStats,
};

use pin_project::pin_project;
use tokio::io;

use std::{
    pin::Pin,
    task::{Context, Poll},
};

#[cfg(any(
    feature = "deflate",
    feature = "deflate-miniz",
    feature = "deflate-zlib"
))]
use flate2::{Compress, Compression};

pub struct ZipWriter<S> {
    inner: Option<S>,
    files: Vec<ZipFileData>,
    stats: ZipWriterStats,
    writing_to_file: bool,
    writing_to_extra_field: bool,
    writing_to_central_extra_field_only: bool,
    writing_raw: bool,
    comment: Vec<u8>,
}

#[pin_project]
pub struct StoredWriter<S>(#[pin] S);

impl<S> StoredWriter<S> {
    pub fn pin_stream(self: Pin<&mut Self>) -> Pin<&mut S> {
        self.project().0
    }
}

impl<S> IntoInner<S> for StoredWriter<S> {
    fn into_inner(self) -> S {
        self.0
    }
}

impl<S: io::AsyncWrite> io::AsyncWrite for StoredWriter<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.pin_stream().poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.pin_stream().poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.pin_stream().poll_shutdown(cx)
    }
}

#[pin_project]
pub struct DeflatedWriter<S>(#[pin] deflate::Writer<Compress, BufWriter<S>>);

impl<S> DeflatedWriter<S> {
    pub fn pin_stream(self: Pin<&mut Self>) -> Pin<&mut deflate::Writer<Compress, BufWriter<S>>> {
        self.project().0
    }
}

impl<S> IntoInner<S> for DeflatedWriter<S> {
    fn into_inner(self) -> S {
        self.0.into_inner().into_inner()
    }
}

impl<S: io::AsyncWrite> io::AsyncWrite for DeflatedWriter<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.pin_stream().poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.pin_stream().poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.pin_stream().poll_shutdown(cx)
    }
}

#[pin_project(project = WrappedProj)]
pub enum ZipFileWrappedWriter<S> {
    Stored(#[pin] StoredWriter<S>),
    Deflated(#[pin] DeflatedWriter<S>),
}

impl<S: io::AsyncWrite> io::AsyncWrite for ZipFileWrappedWriter<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.project() {
            WrappedProj::Stored(w) => w.poll_write(cx, buf),
            WrappedProj::Deflated(w) => w.poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.project() {
            WrappedProj::Stored(w) => w.poll_flush(cx),
            WrappedProj::Deflated(w) => w.poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.project() {
            WrappedProj::Stored(w) => w.poll_shutdown(cx),
            WrappedProj::Deflated(w) => w.poll_shutdown(cx),
        }
    }
}
