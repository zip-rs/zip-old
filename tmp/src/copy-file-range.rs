use std::env;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::str::FromStr;
use std::sync::Arc;

use getrandom::getrandom;
use once_cell::sync::Lazy;
use tokio::{fs, io, task};

use zip::{
    result::{ZipError, ZipResult},
    write::FileOptions,
    CompressionMethod, ZipWriter,
};

fn generate_random_archive(
    num_entries: usize,
    entry_size: usize,
    out_path: &Path,
) -> ZipResult<()> {
    use std::io::Write;

    eprintln!("num_entries = {}", num_entries);
    eprintln!("entry_size = {}", entry_size);

    let out_handle = std::fs::File::create(out_path)?;
    let mut zip = ZipWriter::new(out_handle);
    /* No point compressing random entries. */
    let options = FileOptions::default().compression_method(CompressionMethod::Stored);

    let mut bytes = vec![0u8; entry_size];
    for i in 0..num_entries {
        let name = format!("random{}.dat", i);
        zip.start_file(name, options)?;
        getrandom(&mut bytes).unwrap();
        zip.write_all(&bytes)?;
    }

    let out_handle = zip.finish()?;
    out_handle.sync_all()?;

    Ok(())
}

async fn get_len(p: &Path) -> io::Result<u64> {
    Ok(fs::metadata(p).await?.len())
}

static BIG_ARCHIVE_PATH: Lazy<PathBuf> =
    Lazy::new(|| Path::new("../benches/target.zip").to_path_buf());

static SMALL_ARCHIVE_PATH: Lazy<PathBuf> =
    Lazy::new(|| Path::new("../benches/small-target.zip").to_path_buf());

fn flag_var(var_name: &str) -> bool {
    env::var(var_name)
        .ok()
        .filter(|v| v.starts_with('y'))
        .is_some()
}

fn path_var(var_name: &str) -> Option<PathBuf> {
    let var = env::var(var_name).ok()?;
    Some(var.into())
}

#[tokio::main]
async fn main() -> ZipResult<()> {
    let source = if flag_var("SMALL") || flag_var("small") {
        println!("small!");
        &*SMALL_ARCHIVE_PATH
    } else {
        println!("big!");
        &*BIG_ARCHIVE_PATH
    };

    use std::{fs, io};
    use zip::tokio::os::linux::*;

    let len = get_len(source).await?;
    println!("len = {}", len);

    let handle = fs::File::open(source)?;

    let mut src = FromGivenOffset::new(&handle, Role::Readable, 0)?;

    let out_path = path_var("OUT")
        .or(path_var("out"))
        .unwrap_or_else(|| PathBuf::from("./tmp-copy-out"));
    println!("out = {}", out_path.display());
    let out = fs::File::create(out_path)?;
    let mut dst = MutateInnerOffset::new(out, Role::Writable)?;

    task::spawn_blocking(move || {
        let mut remaining = len;
        while remaining > 0 {
            println!("remaining = {}", remaining);
            let written =
                copy_file_range_raw(Pin::new(&mut src), Pin::new(&mut dst), remaining as usize)?;
            assert!(written > 0);
            assert!(written as u64 <= remaining);
            println!("written = {}", written);
            remaining -= written as u64;
        }
        Ok::<_, ZipError>(())
    })
    .await
    .unwrap()?;

    Ok(())
}
