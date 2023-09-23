use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use getrandom::getrandom;
use once_cell::sync::Lazy;
use tempfile::{tempdir, tempfile};
use tokio::{io, runtime::Runtime};

use zip::{
    read::tokio::{IntermediateFile, SyncIntermediateFile},
    result::ZipResult,
    write::FileOptions,
    ZipWriter,
};

fn generate_random_archive(
    num_entries: usize,
    entry_size: usize,
    options: FileOptions,
) -> ZipResult<Vec<u8>> {
    use std::io::Write;

    let buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(buf);

    let mut bytes = vec![0u8; entry_size];
    for i in 0..num_entries {
        let name = format!("random{}.dat", i);
        zip.start_file(name, options)?;
        getrandom(&mut bytes).unwrap();
        zip.write_all(&bytes)?;
    }

    let buf = zip.finish()?.into_inner();

    Ok(buf)
}

static BIG_ARCHIVE_PATH: Lazy<PathBuf> =
    Lazy::new(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/target.zip"));

static SMALL_ARCHIVE_PATH: Lazy<PathBuf> =
    Lazy::new(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/small-target.zip"));

async fn get_big_archive() -> ZipResult<IntermediateFile> {
    Ok(IntermediateFile::open_from_path(&*BIG_ARCHIVE_PATH).await?)
}

async fn get_small_archive() -> ZipResult<IntermediateFile> {
    Ok(IntermediateFile::open_from_path(&*SMALL_ARCHIVE_PATH).await?)
}

const NUM_ENTRIES: usize = 1_000;
const ENTRY_SIZE: usize = 10_000;

pub fn bench_io(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("io");

    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let random = generate_random_archive(NUM_ENTRIES, ENTRY_SIZE, options).unwrap();

    for (handle, desc) in [
        (rt.block_on(get_big_archive()).unwrap(), "big archive"),
        (rt.block_on(get_small_archive()).unwrap(), "small archive"),
        (IntermediateFile::from_bytes(&random[..]), "random archive"),
    ] {
        let id = format!("{}({} bytes)", desc, handle.len());

        let len = handle.len();
        group.throughput(Throughput::Bytes(len as u64));

        let h2 = rt.block_on(handle.clone_handle()).unwrap();
        group.bench_function(BenchmarkId::new(&id, "<async copy>"), |b| {
            b.to_async(&rt).iter(|| async {
                let mut handle = handle.clone_handle().await.unwrap();
                let td = tempdir().unwrap();
                let tf = td.path().join("out.zip");
                let mut out = IntermediateFile::create_at_path(&tf, len).await.unwrap();
                assert_eq!(len as u64, io::copy(&mut handle, &mut out).await.unwrap());
            });
        });

        let sync_handle = rt.block_on(h2.try_into_sync()).unwrap();
        group.bench_function(BenchmarkId::new(&id, "<sync copy>"), |b| {
            b.iter(|| {
                let mut sync_handle = sync_handle.clone_handle().unwrap();
                let td = tempdir().unwrap();
                let tf = td.path().join("out.zip");
                let mut out = SyncIntermediateFile::create_at_path(&tf, len).unwrap();
                assert_eq!(
                    len as u64,
                    std::io::copy(&mut sync_handle, &mut out).unwrap()
                );
            });
        });
    }
}

pub fn bench_extract(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    let mut group = c.benchmark_group("extract");

    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let random = generate_random_archive(NUM_ENTRIES, ENTRY_SIZE, options).unwrap();

    for (handle, desc) in [
        /* (rt.block_on(get_big_archive()).unwrap(), "big archive"), */
        /* (rt.block_on(get_small_archive()).unwrap(), "small archive"), */
        (IntermediateFile::from_bytes(&random[..]), "random archive"),
    ] {
        let id = format!("{}({} bytes)", desc, handle.len());

        group.throughput(Throughput::Bytes(handle.len() as u64));

        let h2 = rt.block_on(handle.clone_handle()).unwrap();
        group.bench_function(BenchmarkId::new(&id, "<async extraction>"), |b| {
            b.to_async(&rt).iter(|| async {
                let td = tempdir().unwrap();
                let out_dir = Arc::new(td.path().to_path_buf());
                let handle = handle.clone_handle().await.unwrap();
                let mut zip = zip::read::tokio::ZipArchive::new(handle).await.unwrap();
                Pin::new(&mut zip).extract(out_dir.clone()).await.unwrap();
            })
        });

        let sync_handle = rt.block_on(h2.try_into_sync()).unwrap();
        group.bench_function(BenchmarkId::new(&id, "<sync extraction>"), |b| {
            b.iter(|| {
                let td = tempdir().unwrap();
                let sync_handle = sync_handle.clone_handle().unwrap();
                let mut zip = zip::read::ZipArchive::new(sync_handle).unwrap();
                zip.extract(td.path()).unwrap();
            })
        });
    }
}

criterion_group!(benches, bench_io, bench_extract);
criterion_main!(benches);
