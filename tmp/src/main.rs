#![allow(unused_imports)]
#![allow(dead_code)]

use std::io::{Cursor, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use getrandom::getrandom;
use once_cell::sync::Lazy;
use tempfile::tempdir;
use tokio::{fs, io, task};

use zip::{
    combinators::FixedLengthFile,
    result::{ZipError, ZipResult},
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

#[tokio::main]
async fn main() -> ZipResult<()> {
    let len = fs::metadata(&*BIG_ARCHIVE_PATH).await?.len() as usize;

    /* let tf = Path::new("out.zip"); */
    /* if let Some(_) = std::env::var("ASYNC").ok() { */
    /*     eprintln!("async!"); */
    /*     for _ in 0..50 { */
    /*         let mut handle = handle.try_clone().await?; */
    /*         let mut out = IntermediateFile::create_at_path(&tf, len).await?; */
    /*         io::copy(&mut handle, &mut out).await?; */
    /*     } */
    /* } else { */
    /*     eprintln!("no async!"); */
    /*     let sync_handle = handle.try_into_sync().await?; */
    /*     for _ in 0..50 { */
    /*         let mut sync_handle = sync_handle.try_clone()?; */
    /*         let mut out = SyncIntermediateFile::create_at_path(&tf, len)?; */
    /*         std::io::copy(&mut sync_handle, &mut out)?; */
    /*     } */
    /* } */

    if let Some(_) = std::env::var("ASYNC").ok() {
        eprintln!("async!");
        let out = Arc::new(Path::new("./tmp-out").to_path_buf());
        for _ in 0..5 {
            let handle =
                FixedLengthFile::<fs::File>::read_from_path(&*BIG_ARCHIVE_PATH, len).await?;
            let mut src = zip::read::tokio::ZipArchive::new(handle).await?;
            Pin::new(&mut src).extract(out.clone()).await?;
        }
    } else {
        eprintln!("no async!");
        let out = Path::new("./tmp-out2");
        for _ in 0..5 {
            task::spawn_blocking(move || {
                let handle =
                    FixedLengthFile::<std::fs::File>::read_from_path(&*BIG_ARCHIVE_PATH, len)?;
                let mut src = zip::read::ZipArchive::new(handle)?;
                src.extract(out)?;
                Ok::<_, ZipError>(())
            })
            .await
            .unwrap()?;
        }
    }

    Ok(())
}
