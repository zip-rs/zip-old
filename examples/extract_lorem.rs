#![feature(exit_status)]

use std::io::prelude::*;

extern crate zip;

fn main()
{
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <filename>", args[0]);
        std::env::set_exit_status(1);
        return;
    }
    let fname = std::path::Path::new(&*args[1]);
    let zipfile = std::fs::File::open(&fname).unwrap();

    let mut archive = zip::ZipArchive::new(zipfile).unwrap();
    
    let mut file = match archive.by_name("test/lorem_ipsum.txt")
    {
        Ok(file) => file,
        Err(..) => { println!("File test/lorem_ipsum.txt not found"); return }
    };

    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    println!("{}", contents);
}
