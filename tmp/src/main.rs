use std::io::{Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use getrandom::getrandom;
use once_cell::sync::Lazy;
use tempfile::tempdir;
use tokio::io;

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
    Lazy::new(|| Path::new("../benches/target.zip").to_path_buf());

static SMALL_ARCHIVE_PATH: Lazy<PathBuf> =
    Lazy::new(|| Path::new("../benches/small-target.zip").to_path_buf());

async fn get_big_archive() -> ZipResult<IntermediateFile> {
    Ok(IntermediateFile::open_from_path(&*BIG_ARCHIVE_PATH).await?)
}

async fn get_small_archive() -> ZipResult<IntermediateFile> {
    Ok(IntermediateFile::open_from_path(&*SMALL_ARCHIVE_PATH).await?)
}

#[tokio::main]
async fn main() -> ZipResult<()> {
    let handle = get_big_archive().await?;
    let len = handle.len();
    let tf = Path::new("out.zip");

    if let Some(_) = std::env::var("ASYNC").ok() {
        eprintln!("async!");
        for _ in 0..50 {
            let mut handle = handle.try_clone().await?;
            let mut out = IntermediateFile::create_at_path(&tf, len).await?;
            io::copy(&mut handle, &mut out).await?;
        }
    } else {
        eprintln!("no async!");
        let sync_handle = handle.try_into_sync().await?;
        for _ in 0..50 {
            let mut sync_handle = sync_handle.try_clone()?;
            let mut out = SyncIntermediateFile::create_at_path(&tf, len)?;
            std::io::copy(&mut sync_handle, &mut out)?;
        }
    }

    /* let mut sync_handle = handle.try_into_sync().await?; */
    /* let mut out = SyncIntermediateFile::create_at_path(&tf, len)?; */
    /* std::io::copy(&mut sync_handle, &mut out)?; */
    /* let mut src = zip::read::tokio::ZipArchive::new(handle).await?; */
    /* let out = Path::new("./tmp-out"); */
    /* Pin::new(&mut src) */
    /*     .extract(Arc::new(out.to_path_buf())) */
    /*     .await?; */

    /* let out = Path::new("./tmp-out2"); */
    /* let mut src = zip::read::ZipArchive::new(src.into_inner().try_into_sync().await?)?; */
    /* src.extract(out)?; */
    Ok(())
}
