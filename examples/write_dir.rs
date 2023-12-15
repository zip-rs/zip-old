use clap::{Parser, ValueEnum};
use std::io::prelude::*;
use std::io::{Seek, Write};
use std::iter::Iterator;
use zip::result::ZipError;
use zip::write::FileOptions;

use std::fs::File;
use std::path::Path;
use walkdir::{DirEntry, WalkDir};

#[derive(Parser)]
#[command(about, long_about = None)]
struct Args {
    // Source directory
    source_directory: String,
    // Destination zipfile 
    destination_zipfile: String,
    // Compression method 
    #[arg(value_enum)]
    compression_method: CompressionMethod,
}

#[derive(Clone, ValueEnum)]
enum CompressionMethod {
    MethodStored,
    MethodDeflated,
    MethodDeflatedMiniz,
    MethodDeflatedZlib,
    MethodBzip2,
    MethodZstd,
}

fn main() {
    std::process::exit(real_main());
}

const METHOD_STORED: Option<zip::CompressionMethod> = Some(zip::CompressionMethod::Stored);

#[cfg(any(
    feature = "deflate",
    feature = "deflate-miniz",
    feature = "deflate-zlib"
))]
const METHOD_DEFLATED: Option<zip::CompressionMethod> = Some(zip::CompressionMethod::Deflated);
#[cfg(not(any(
    feature = "deflate",
    feature = "deflate-miniz",
    feature = "deflate-zlib"
)))]
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
    // let args: Vec<_> = std::env::args().collect();
    // if args.len() < 3 {
    //     println!(
    //         "Usage: {} <source_directory> <destination_zipfile>",
    //         args[0]
    //     );
    //     return 1;
    // }

    let args = Args::parse();
    let src_dir = &args.source_directory;
    let dst_file = &args.destination_zipfile;
    let method: Option<zip::CompressionMethod> = match args.compression_method {
        CompressionMethod::MethodStored => Some(zip::CompressionMethod::Stored),
        CompressionMethod::MethodDeflated => {
            #[cfg(not(feature = "deflate"))]
            println!("The `deflate` feature is not enabled");
            None;
            #[cfg(feature = "deflate")]
            Some(zip::CompressionMethod::Deflated)
        },
        CompressionMethod::MethodDeflatedMiniz => {
            #[cfg(not(feature = "deflate-miniz"))]
            println!("The `deflate-miniz` feature is not enabled");
            None;
            #[cfg(feature = "deflate-miniz")]
            Some(zip::CompressionMethod::Deflated)
        },
        CompressionMethod::MethodDeflatedZlib => {
            #[cfg(not(feature = "deflate-zlib"))]
            println!("The `deflate-zlib` feature is not enabled");
            None;
            #[cfg(feature = "deflate-zlib")]
            Some(zip::CompressionMethod::Deflated)
        },
        CompressionMethod::MethodBzip2 => {
            #[cfg(not(feature = "bzip2"))]
            println!("The `bzip2` feature is not enabled");
            None;
            #[cfg(feature = "bzip2")]
            Some(zip::CompressionMethod::Bzip2)
        },
        CompressionMethod::MethodZstd => {
            #[cfg(not(feature = "zstd"))]
            println!("The `zstd` feature is not enabled");
            None;
            #[cfg(feature = "zstd")]
            Some(zip::CompressionMethod::Zstd)
        }
    };
    // for &method in [METHOD_STORED, METHOD_DEFLATED, METHOD_BZIP2, METHOD_ZSTD].iter() {
    // if method.is_none() {
    //     continue;
    // }
    match doit(src_dir, dst_file, method) {
        Ok(_) => println!("done: {src_dir} written to {dst_file}"),
        Err(e) => println!("Error: {e:?}"),
    }
    //}

    0
}

fn zip_dir<T>(
    it: &mut dyn Iterator<Item = DirEntry>,
    prefix: &str,
    writer: T,
    method: zip::CompressionMethod,
) -> zip::result::ZipResult<()>
where
    T: Write + Seek,
{
    let mut zip = zip::ZipWriter::new(writer);
    let options = FileOptions::default()
        .compression_method(method)
        .unix_permissions(0o755);

    let mut buffer = Vec::new();
    for entry in it {
        let path = entry.path();
        let name = path.strip_prefix(Path::new(prefix)).unwrap();

        // Write file or directory explicitly
        // Some unzip tools unzip files with directory paths correctly, some do not!
        if path.is_file() {
            println!("adding file {path:?} as {name:?} ...");
            #[allow(deprecated)]
            zip.start_file_from_path(name, options)?;
            let mut f = File::open(path)?;

            f.read_to_end(&mut buffer)?;
            zip.write_all(&buffer)?;
            buffer.clear();
        } else if !name.as_os_str().is_empty() {
            // Only if not root! Avoids path spec / warning
            // and mapname conversion failed error on unzip
            println!("adding dir {path:?} as {name:?} ...");
            #[allow(deprecated)]
            zip.add_directory_from_path(name, options)?;
        }
    }
    zip.finish()?;
    Result::Ok(())
}

fn doit(
    src_dir: &str,
    dst_file: &str,
    method: zip::CompressionMethod,
) -> zip::result::ZipResult<()> {
    if !Path::new(src_dir).is_dir() {
        return Err(ZipError::FileNotFound);
    }

    let path = Path::new(dst_file);
    let file = File::create(path).unwrap();

    let walkdir = WalkDir::new(src_dir);
    let it = walkdir.into_iter();

    zip_dir(&mut it.filter_map(|e| e.ok()), src_dir, file, method)?;

    Ok(())
}
