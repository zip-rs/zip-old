use std::io::prelude::*;

extern crate zip;

fn main()
{
    std::process::exit(real_main());
}

fn real_main() -> i32
{
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        println!("Usage: {} <filename.zip> <filename_in_archive.zip>", args[0]);
        return 1;
    }
    let fname = std::path::Path::new(&*args[1]);
    let zipfile = std::fs::File::open(&fname).unwrap();

    let archive = zip::ZipArchive::new(zipfile).unwrap();
    
    let mut file = match archive.by_name(&args[2])
    {
        Ok(file) => file,
        Err(..) => { println!("File {} not found", &args[2]); return 2;}
    };

    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    println!("{}", contents);

    return 0;
}
