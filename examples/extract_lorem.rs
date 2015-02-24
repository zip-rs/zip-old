#![feature(old_path, io, fs, env)]

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
    let fname = Path::new(&*args[1]);
    let file = std::fs::File::open(&fname).unwrap();

    let zipcontainer = zip::ZipReader::new(file).unwrap();
    
    let file = match zipcontainer.get("test/lorem_ipsum.txt")
    {
        Some(file) => file,
        None => { println!("File test/lorem_ipsum.txt not found"); return }
    };

    let mut contents = String::new();
    zipcontainer.read_file(file).unwrap().read_to_string(&mut contents).unwrap();
    println!("{}", contents);
}
