use std::io::prelude::*;

extern crate zip;

fn main() {
    std::process::exit(real_main());
}

fn real_main() -> i32 {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <filename>", args[0]);
        return 1;
    }
    let fname = std::path::Path::new(&*args[1]);
    let zipfile = std::fs::File::open(&fname).unwrap();

    let mut archive = zip::ZipArchive::new(zipfile).expect("Couldn't open zip file");
    let index = archive.index().unwrap();

    let file_data = match index.by_name("test/lorem_ipsum.txt") {
        Some(file_data) => file_data,
        None => {
            println!("File test/lorem_ipsum.txt not found");
            return 2;
        }
    };
    let mut file = archive.open(&file_data).expect("Couldn't open test/lorem_ipsum.txt");

    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    println!("{}", contents);

    return 0;
}
