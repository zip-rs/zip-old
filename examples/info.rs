use std::path::Path;

extern crate zip;

fn main() {
    std::process::exit(real_main());
}

fn real_main() -> i32 {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        println!("Usage: {} <filename.zip> <filename.txt>", args[0]);
        return 1;
    }
    let zipfile = std::fs::File::open(&Path::new(&*args[1])).unwrap();
    let file_name = &*args[2];

    let archive = match zip::ZipArchive::new(zipfile) {
        Ok(zip) => zip,
        Err(e) => {
            println!("File {} not found", &e);
            return 2;
        }
    };

    let file = match archive.by_name(&file_name) {
        Ok(file) => file,
        Err(..) => {
            println!("File {} not found", &file_name);
            return 2;
        }
    };
    println!("Name: {}", file.name());
    println!("Comment: {}", file.comment());
    println!("Version: {:?}", file.version_made_by());
    println!("Compression: {}", file.compression());
    println!("Compressed size: {}", file.compressed_size());
    println!("Original   size: {}", file.size());
    println!("Ratio: {} %", file.compressed_size() * 100 / file.size());
    println!("Last modified: {:?}", file.last_modified());
    println!("crc32: {}", file.crc32());
    println!("Offset in file: {}", file.offset());

    return 0;
}
