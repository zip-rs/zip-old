#![allow(unused_variables)]
#![allow(dead_code)]
use anyhow::Context;
use clap::{Parser, ValueEnum};
use std::io::prelude::*;
use zip::{result::ZipError, write::SimpleFileOptions};

use std::fs::File;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

#[derive(Parser)]
#[command(about, long_about = None)]
struct Args {
    // Source directory
    source: PathBuf,
    // Destination zipfile
    destination: PathBuf,
    // Compression method
    #[arg(value_enum)]
    compression_method: CompressionMethod,
}

#[derive(Clone, ValueEnum)]
enum CompressionMethod {
    Stored,
    Deflated,
    DeflatedMiniz,
    DeflatedZlib,
    Bzip2,
    Zstd,
}

fn main() {
    std::process::exit(real_main());
}

const METHOD_STORED: Option<zip::CompressionMethod> = Some(zip::CompressionMethod::Stored);

#[cfg(feature = "_deflate-any")]
const METHOD_DEFLATED: Option<zip::CompressionMethod> = Some(zip::CompressionMethod::Deflated);
#[cfg(not(feature = "_deflate-any"))]
const METHOD_DEFLATED: Option<zip::CompressionMethod> = None;

#[cfg(feature = "bzip2")]
const METHOD_BZIP2: Option<zip::CompressionMethod> = Some(zip::CompressionMethod::Bzip2);
#[cfg(not(feature = "bzip2"))]
const METHOD_BZIP2: Option<zip::CompressionMethod> = None;

#[cfg(feature = "zstd")]
const METHOD_ZSTD: Option<zip::CompressionMethod> = Some(zip::CompressionMethod::Zstd);
#[cfg(not(feature = "zstd"))]
const METHOD_ZSTD: Option<zip::CompressionMethod> = None;

fn real_main() -> i32 {
    let args = Args::parse();
    let src_dir = &args.source;
    let dst_file = &args.destination;
    let method = match args.compression_method {
        CompressionMethod::Stored => zip::CompressionMethod::Stored,
        CompressionMethod::Deflated => {
            #[cfg(not(feature = "deflate"))]
            {
                println!("The `deflate` feature is not enabled");
                return 1;
            }
            #[cfg(feature = "deflate")]
            zip::CompressionMethod::Deflated
        }
        CompressionMethod::DeflatedMiniz => {
            #[cfg(not(feature = "deflate-miniz"))]
            {
                println!("The `deflate-miniz` feature is not enabled");
                return 1;
            }
            #[cfg(feature = "deflate-miniz")]
            zip::CompressionMethod::Deflated
        }
        CompressionMethod::DeflatedZlib => {
            #[cfg(not(feature = "deflate-zlib"))]
            {
                println!("The `deflate-zlib` feature is not enabled");
                return 1;
            }
            #[cfg(feature = "deflate-zlib")]
            zip::CompressionMethod::Deflated
        }
        CompressionMethod::Bzip2 => {
            #[cfg(not(feature = "bzip2"))]
            {
                println!("The `bzip2` feature is not enabled");
                return 1;
            }
            #[cfg(feature = "bzip2")]
            zip::CompressionMethod::Bzip2
        }
        CompressionMethod::Zstd => {
            #[cfg(not(feature = "zstd"))]
            {
                println!("The `zstd` feature is not enabled");
                return 1;
            }
            #[cfg(feature = "zstd")]
            zip::CompressionMethod::Zstd
        }
    };
    match doit(src_dir, dst_file, method) {
        Ok(_) => println!("done: {:?} written to {:?}", src_dir, dst_file),
        Err(e) => eprintln!("Error: {e:?}"),
    }

    0
}

fn zip_dir<T>(
    it: &mut dyn Iterator<Item = DirEntry>,
    prefix: &Path,
    writer: T,
    method: zip::CompressionMethod,
) -> anyhow::Result<()>
where
    T: Write + Seek,
{
    let mut zip = zip::ZipWriter::new(writer);
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .unix_permissions(0o755);

    let prefix = Path::new(prefix);
    let mut buffer = Vec::new();
    for entry in it {
        let path = entry.path();
        let name = path.strip_prefix(prefix).unwrap();
        let path_as_string = name
            .to_str()
            .map(str::to_owned)
            .with_context(|| format!("{name:?} Is a Non UTF-8 Path"))?;

        // Write file or directory explicitly
        // Some unzip tools unzip files with directory paths correctly, some do not!
        if path.is_file() {
            println!("adding file {path:?} as {name:?} ...");
            zip.start_file(path_as_string, options)?;
            let mut f = File::open(path)?;

            f.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
            buffer.clear();
        } else if !name.as_os_str().is_empty() {
            // Only if not root! Avoids path spec / warning
            // and mapname conversion failed error on unzip
            println!("adding dir {path_as_string:?} as {name:?} ...");
            zip.add_directory(path_as_string, options)?;
        }
    }
    zip.finish()?;
    Ok(())
}

fn doit(src_dir: &Path, dst_file: &Path, method: zip::CompressionMethod) -> anyhow::Result<()> {
    if !Path::new(src_dir).is_dir() {
        return Err(ZipError::FileNotFound.into());
    }

    let path = Path::new(dst_file);
    let file = File::create(path).unwrap();

    let walkdir = WalkDir::new(src_dir);
    let it = walkdir.into_iter();

    zip_dir(&mut it.filter_map(|e| e.ok()), src_dir, file, method)?;

    Ok(())
}
