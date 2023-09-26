use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use getrandom::getrandom;
use once_cell::sync::Lazy;
use tempfile::tempdir;
use tokio::{fs, io, runtime::Runtime};

use zip::{
    combinators::{FixedLengthFile, Limiter},
    result::ZipResult,
    write::FileOptions,
    ZipWriter,
};

fn generate_random_archive(
    num_entries: usize,
    entry_size: usize,
    options: FileOptions,
) -> ZipResult<Cursor<Box<[u8]>>> {
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

    Ok(Cursor::new(buf.into_boxed_slice()))
}

static BIG_ARCHIVE_PATH: Lazy<PathBuf> =
    Lazy::new(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/target.zip"));

static SMALL_ARCHIVE_PATH: Lazy<PathBuf> =
    Lazy::new(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/small-target.zip"));

const NUM_ENTRIES: usize = 1_000;
const ENTRY_SIZE: usize = 10_000;

pub fn bench_io(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();

    let mut group = c.benchmark_group("io");

    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let gen_td = tempdir().unwrap();
    let random_path = gen_td.path().join("random.zip");
    std::io::copy(
        &mut generate_random_archive(NUM_ENTRIES, ENTRY_SIZE, options).unwrap(),
        &mut std::fs::File::create(&random_path).unwrap(),
    )
    .unwrap();

    for (path, desc) in [
        (&*BIG_ARCHIVE_PATH, "big archive"),
        (&*SMALL_ARCHIVE_PATH, "small archive"),
        (&random_path, "random archive"),
    ] {
        let len = std::fs::metadata(&path).unwrap().len() as usize;
        let id = format!("{}({} bytes)", desc, len);

        group.throughput(Throughput::Bytes(len as u64));

        group.bench_function(BenchmarkId::new(&id, "<async copy>"), |b| {
            b.to_async(&rt).iter(|| async {
                let mut handle = FixedLengthFile::<fs::File>::read_from_path(path, len)
                    .await
                    .unwrap();
                let td = tempdir().unwrap();
                let tf = td.path().join("out.zip");
                let mut out = FixedLengthFile::<fs::File>::create_at_path(tf, len)
                    .await
                    .unwrap();
                assert_eq!(len as u64, io::copy(&mut handle, &mut out).await.unwrap());
            });
        });

        group.bench_function(BenchmarkId::new(&id, "<sync copy>"), |b| {
            b.iter(|| {
                let mut sync_handle =
                    FixedLengthFile::<std::fs::File>::read_from_path(path, len).unwrap();
                let td = tempdir().unwrap();
                let tf = td.path().join("out.zip");
                let mut out = FixedLengthFile::<std::fs::File>::create_at_path(tf, len).unwrap();
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

    let gen_td = tempdir().unwrap();
    let random_path = gen_td.path().join("random.zip");
    std::io::copy(
        &mut generate_random_archive(NUM_ENTRIES, ENTRY_SIZE, options).unwrap(),
        &mut std::fs::File::create(&random_path).unwrap(),
    )
    .unwrap();

    for (path, desc) in [
        (&*BIG_ARCHIVE_PATH, "big archive"),
        (&*SMALL_ARCHIVE_PATH, "small archive"),
        (&random_path, "random archive"),
    ] {
        let len = std::fs::metadata(&path).unwrap().len() as usize;
        let id = format!("{}({} bytes)", desc, len);

        group.throughput(Throughput::Bytes(len as u64));

        group.bench_function(BenchmarkId::new(&id, "<async extraction>"), |b| {
            b.to_async(&rt).iter(|| async {
                let td = tempdir().unwrap();
                let out_dir = Arc::new(td.path().to_path_buf());
                let handle = FixedLengthFile::<fs::File>::read_from_path(path, len)
                    .await
                    .unwrap();
                let mut zip = zip::read::tokio::ZipArchive::new(handle).await.unwrap();
                Pin::new(&mut zip).extract(out_dir).await.unwrap();
            })
        });

        group.bench_function(BenchmarkId::new(&id, "<sync extraction>"), |b| {
            b.iter(|| {
                let td = tempdir().unwrap();
                let sync_handle =
                    FixedLengthFile::<std::fs::File>::read_from_path(path, len).unwrap();
                let mut zip = zip::read::ZipArchive::new(sync_handle).unwrap();
                zip.extract(td.path()).unwrap();
            })
        });
    }
}

criterion_group!(benches, bench_io, bench_extract);
criterion_main!(benches);
