use std::io::prelude::*;
use zip::write::FileOptions;

use walkdir::WalkDir;
use std::path::Path;
use std::fs::File;

extern crate zip;
extern crate walkdir;

fn main() {
    std::process::exit(real_main());
}

fn real_main() -> i32 {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        println!("Usage: {} <source_directory> <destination_zipfile>",
                 args[0]);
        return 1;
    }

    let src_dir = &*args[1];
    let dst_file = &*args[2];
    match doit(src_dir, dst_file) {
        Ok(_) => println!("done: {} written to {}", src_dir, dst_file),
        Err(e) => println!("Error: {:?}", e),
    }

    return 0;
}

fn doit(src_dir: &str, dst_file: &str) -> zip::result::ZipResult<()> {
    if !Path::new(src_dir).is_dir() {
        return Ok(());
    }

    let path = Path::new(dst_file);
    let file = File::create(&path).unwrap();

    let mut zip = zip::ZipWriter::new(file);

    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o755);

    let walkdir = WalkDir::new(src_dir.to_string());

    let it = walkdir.into_iter();

    for dent in it.filter_map(|e| e.ok()) {
        let path = dent.path();
        let name = path.strip_prefix(Path::new(src_dir))
            .unwrap()
            .to_str()
            .unwrap();


        if path.is_file() {
            println!("adding {:?} as {:?} ...", path, name);
            try!(zip.start_file(name, options));
            let mut f = File::open(path)?;
            let mut buffer = Vec::new();
            f.read_to_end(&mut buffer)?;
            try!(zip.write_all(&*buffer));
        }
    }

    try!(zip.finish());

    Ok(())
}
