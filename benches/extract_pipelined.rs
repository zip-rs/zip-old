use bencher::{benchmark_group, benchmark_main};

use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;

use bencher::Bencher;
use getrandom::getrandom;
use tempfile::tempdir;
use zip::{read::Handle, result::ZipResult, write::FileOptions, ZipArchive, ZipWriter};

fn generate_random_archive(
    num_entries: usize,
    entry_size: usize,
    options: FileOptions,
) -> ZipResult<(usize, Vec<u8>)> {
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
    let len = buf.len();

    Ok((len, buf))
}

fn perform_pipelined<'a, P: AsRef<Path>>(src: ZipArchive<Handle<'a>>, target: P) -> ZipResult<()> {
    src.extract_pipelined(target)
}

fn perform_sync<R: Read + Seek, W: Write + Seek, P: AsRef<Path>>(
    mut src: ZipArchive<R>,
    target: P,
) -> ZipResult<()> {
    src.extract(target)
}

const NUM_ENTRIES: usize = 100;
const ENTRY_SIZE: usize = 100;

fn extract_pipelined(bench: &mut Bencher) {
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let (len, src) = generate_random_archive(NUM_ENTRIES, ENTRY_SIZE, options).unwrap();
    let src = ZipArchive::new(Handle::mem(&src)).unwrap();

    bench.bytes = len as u64;

    bench.iter(|| {
        let td = tempdir().unwrap();
        perform_pipelined(src.clone(), td).unwrap();
    });
}

fn extract_sync(bench: &mut Bencher) {
    let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let (len, src) = generate_random_archive(NUM_ENTRIES, ENTRY_SIZE, options).unwrap();

    bench.bytes = len as u64;

    bench.iter(|| {
        let td = tempdir().unwrap();
        perform_sync::<Cursor<Vec<u8>>, Cursor<Vec<u8>>, _>(
            ZipArchive::new(Cursor::new(src.clone())).unwrap(),
            td,
        )
        .unwrap();
    });
}

benchmark_group!(benches, extract_pipelined, extract_sync);
benchmark_main!(benches);
