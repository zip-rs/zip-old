use crate::{
    result::ZipResult,
    tokio::read::{Shared, SharedData},
    types::ZipFileData,
};

pub mod linux;

use tokio::{
    fs,
    io::{self, unix::AsyncFd},
};

use std::{cmp, ops, os::fd::AsRawFd, path::PathBuf, pin::Pin, sync::Arc};

#[derive(Debug)]
pub struct SharedSubset {
    parent: Arc<Shared>,
    content_range: ops::Range<u64>,
    entry_range: ops::RangeInclusive<usize>,
}

impl SharedData for SharedSubset {
    #[inline]
    fn content_range(&self) -> ops::Range<u64> {
        debug_assert!(self.content_range.start <= self.content_range.end);
        debug_assert!(self.content_range.start >= self.parent.offset());
        debug_assert!(self.content_range.end <= self.parent.directory_start());
        self.content_range.clone()
    }
    #[inline]
    fn comment(&self) -> &[u8] {
        self.parent.comment()
    }
    #[inline]
    fn contiguous_entries(&self) -> &indexmap::map::Slice<String, ZipFileData> {
        debug_assert!(self.entry_range.start() <= self.entry_range.end());
        &self.parent.contiguous_entries()[self.entry_range.clone()]
    }
}

impl SharedSubset {
    pub(crate) fn split_contiguous_chunks(
        parent: Arc<Shared>,
        num_chunks: usize,
    ) -> Box<[SharedSubset]> {
        let chunk_size = cmp::max(1, parent.len() / num_chunks);
        let all_entry_indices: Vec<usize> = (0..parent.len()).collect();
        let chunked_entry_ranges: Vec<ops::RangeInclusive<usize>> = all_entry_indices
            .chunks(chunk_size)
            .map(|chunk_indices| {
                let min = *chunk_indices.iter().min().unwrap();
                let max = *chunk_indices.iter().max().unwrap();
                min..=max
            })
            .collect();
        assert!(chunked_entry_ranges.len() <= num_chunks);

        let parent_slice = parent.contiguous_entries();
        let chunked_slices: Vec<&indexmap::map::Slice<String, ZipFileData>> = chunked_entry_ranges
            .iter()
            .map(|range| &parent_slice[range.clone()])
            .collect();

        let chunk_regions: Vec<ops::Range<u64>> = chunked_slices
            .iter()
            .enumerate()
            .map(|(i, chunk)| {
                fn begin_pos(chunk: &indexmap::map::Slice<String, ZipFileData>) -> u64 {
                    assert!(!chunk.is_empty());
                    chunk.get_index(0).unwrap().1.header_start
                }
                if i == 0 {
                    assert_eq!(parent.offset(), begin_pos(chunk));
                }
                let beg = begin_pos(chunk);
                let end = chunked_slices
                    .get(i + 1)
                    .map(|chunk| *chunk)
                    .map(begin_pos)
                    .unwrap_or(parent.directory_start());
                beg..end
            })
            .collect();

        let subsets: Vec<SharedSubset> = chunk_regions
            .into_iter()
            .zip(chunked_entry_ranges.into_iter())
            .map(|(content_range, entry_range)| SharedSubset {
                parent: parent.clone(),
                content_range,
                entry_range,
            })
            .collect();

        subsets.into_boxed_slice()
    }
}

/* #[derive(Debug)] */
/* pub struct MappedZipArchive<S> { */
/*     shared: Arc<SharedSubset>, */
/*     reader: Option<Pin<Box<S>>>, */
/* } */

/* #[derive(Debug)] */
/* pub struct ZipFdArchive<S: AsRawFd> { */
/*     shared: Arc<Shared>, */
/*     fd: Pin<Box<AsyncFd<S>>>, */
/* } */

/* impl<S: AsRawFd + io::AsyncRead + io::AsyncSeek> ZipFdArchive<S> { */
/*     pub async fn new(reader: Pin<Box<S>>) -> ZipResult<Self> { */
/*         let (shared, reader) = Shared::parse(reader).await?; */
/*         Ok(Self { */
/*             fd: AsyncFd::with_interest( */
/*                 Pin::into_inner(reader), */
/*                 io::Interest::READABLE | io::Interest::WRITABLE | io::Interest::ERROR, */
/*             )?, */
/*             shared: Arc::new(shared), */
/*         }) */
/*     } */
/* } */

/* impl<S: AsRawFd> ZipFdArchive<S> { */
/*     const NUM_PIPES: usize = 8; */

/*     pub async fn splicing_extract(self: Pin<&mut Self>, root: Arc<PathBuf>) -> ZipResult<()> { */
/*         /\* use futures_util::{stream, StreamExt}; *\/ */

/*         fs::create_dir_all(&*root).await?; */

/*         let s = self.get_mut(); */

/*         let subsets = SharedSubset::split_contiguous_chunks(s.shared.clone(), Self::NUM_PIPES); */

/*         /\* stream::iter(subsets.into_iter()) *\/ */
/*         /\*     .map(Ok) *\/ */
/*         /\*     .try_for_each_concurrent(None, move |(range, chunk)| async move { *\/ */
/*         /\*         let (mut r, mut w) = tokio_pipe::pipe()?; *\/ */
/*         /\*         task::spawn(async move { *\/ */
/*         /\*             assert!(range.end >= range.start); *\/ */
/*         /\*             assert!(range.end <= i64::MAX as u64); *\/ */
/*         /\*             let mut off_in: i64 = range.start as i64; *\/ */
/*         /\*             let mut remaining: usize = range.end - range.start; *\/ */
/*         /\*             while remaining > 0 { *\/ */
/*         /\*                 let written = w *\/ */
/*         /\*                     .splice_from(&mut s.fd, Some(&mut off_in), remaining) *\/ */
/*         /\*                     .await?; *\/ */
/*         /\*                 assert!(written <= remaining); *\/ */
/*         /\*                 remaining -= written; *\/ */
/*         /\*                 off_in += written as i64; *\/ */
/*         /\*             } *\/ */
/*         /\*             Ok::<_, ZipError>(()) *\/ */
/*         /\*         }); *\/ */
/*         /\*         Ok::<_, ZipError>(()) *\/ */
/*         /\*     }) *\/ */
/*         /\*     .await?; *\/ */

/*         todo!("impl with copy_file_range and/or tokio_pipe!") */
/*     } */
/* } */

#[cfg(test)]
mod test {
    use super::*;

    use crate::{
        tokio::{read::ZipArchive, write::ZipWriter},
        write::FileOptions,
    };
    use std::{io::Cursor, pin::Pin};
    use tokio::io::{self, AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

    #[tokio::test]
    async fn test_split_contiguous_chunks() -> ZipResult<()> {
        let options = FileOptions::default();

        let buf = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(Box::pin(buf));
        let mut zp = Pin::new(&mut zip);
        zp.as_mut().start_file("a.txt", options).await?;
        zp.write_all(b"hello\n").await?;
        let mut src = zip.finish_into_readable().await?;
        let src = Pin::new(&mut src);

        let buf = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(Box::pin(buf));
        let mut zp = Pin::new(&mut zip);
        zp.as_mut().start_file("b.txt", options).await?;
        zp.write_all(b"hey\n").await?;
        let mut src2 = zip.finish_into_readable().await?;
        let src2 = Pin::new(&mut src2);

        let buf = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(Box::pin(buf));
        let mut zp = Pin::new(&mut zip);
        zp.as_mut().start_file("c/d.txt", options).await?;
        zp.write_all(b"asdf!\n").await?;
        let mut src3 = zip.finish_into_readable().await?;
        let src3 = Pin::new(&mut src3);

        let prefix = [0u8; 200];
        let mut buf = Cursor::new(Vec::new());
        buf.write_all(&prefix).await?;
        let mut zip = ZipWriter::new(Box::pin(buf));
        let mut zp = Pin::new(&mut zip);
        zp.as_mut().merge_archive(src).await?;
        zp.as_mut().merge_archive(src2).await?;
        zp.merge_archive(src3).await?;
        let result = zip.finish_into_readable().await?;

        let parent = result.shared();
        assert_eq!(parent.len(), 3);
        assert_eq!(parent.offset(), prefix.len() as u64);
        assert_eq!(200..329, parent.content_range());

        let split_result = SharedSubset::split_contiguous_chunks(parent.clone(), 3);
        assert_eq!(split_result.len(), 3);

        assert_eq!(200..243, split_result[0].content_range);
        assert_eq!(1, split_result[0].len());
        assert_eq!(0..=0, split_result[0].entry_range);

        assert_eq!(243..284, split_result[1].content_range);
        assert_eq!(1, split_result[1].len());
        assert_eq!(1..=1, split_result[1].entry_range);

        assert_eq!(284..329, split_result[2].content_range);
        assert_eq!(1, split_result[2].len());
        assert_eq!(2..=2, split_result[2].entry_range);
        assert_eq!(329, parent.directory_start());

        Ok(())
    }
}
