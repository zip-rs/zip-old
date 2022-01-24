use criterion::{criterion_group, criterion_main};
use criterion::{BenchmarkId, Criterion};

use std::io::{Cursor, Read, Write};

use getrandom::getrandom;
use strum::IntoEnumIterator;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

fn generate_random_archive(size: usize, method: Option<CompressionMethod>) -> Vec<u8> {
    let data = Vec::new();
    let mut writer = ZipWriter::new(Cursor::new(data));
    let options = zip::write::FileOptions::default()
        .compression_method(method.unwrap_or(CompressionMethod::Stored));

    writer.start_file("random.dat", options).unwrap();

    // Generate some random data.
    let mut bytes = vec![0u8; size];
    getrandom(&mut bytes).unwrap();
    writer.write_all(&bytes).unwrap();

    writer.finish().unwrap().into_inner()
}

fn read_entry(bench: &mut Criterion) {
    let size = 1024 * 1024;
    let mut group = bench.benchmark_group("read_entry");
    for method in CompressionMethod::iter() {
        #[allow(deprecated)]
        if method == CompressionMethod::Unsupported(0) {
            continue;
        }

        group.bench_with_input(
            BenchmarkId::from_parameter(method),
            &method,
            |bench, method| {
                let bytes = generate_random_archive(size, Some(*method));

                bench.iter(|| {
                    let mut archive = ZipArchive::new(Cursor::new(bytes.as_slice())).unwrap();
                    let mut file = archive.by_name("random.dat").unwrap();
                    let mut buf = [0u8; 1024];

                    let mut total_bytes = 0;

                    loop {
                        let n = file.read(&mut buf).unwrap();
                        total_bytes += n;
                        if n == 0 {
                            return total_bytes;
                        }
                    }
                });
            },
        );
    }
}

fn write_random_archive(bench: &mut Criterion) {
    let size = 1024 * 1024;
    let mut group = bench.benchmark_group("write_random_archive");
    for method in CompressionMethod::iter() {
        #[allow(deprecated)]
        if method == CompressionMethod::Unsupported(0) {
            continue;
        }

        group.bench_with_input(BenchmarkId::from_parameter(method), &method, |b, method| {
            b.iter(|| {
                generate_random_archive(size, Some(*method));
            })
        });
    }

    group.finish();
}

criterion_group!(benches, read_entry, write_random_archive);
criterion_main!(benches);
