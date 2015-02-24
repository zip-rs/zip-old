#![feature(old_path, io, fs, env)]

extern crate zip;

use std::io;
use std::fs;

fn main()
{
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <filename>", args[0]);
        std::env::set_exit_status(1);
        return;
    }
    let fname = Path::new(&*args[1]);
    let file = fs::File::open(&fname).unwrap();

    let zipcontainer = zip::ZipReader::new(file).unwrap();

    for file in zipcontainer.files()
    {
        let outpath = sanitize_filename(&*file.file_name);
        println!("{}", outpath.display());

        let comment = &file.file_comment;
        if comment.len() > 0 { println!("  File comment: {}", comment); }

        fs::create_dir_all(&outpath.dir_path()).unwrap();

        if (&*file.file_name).ends_with("/") {
            create_directory(outpath);
        }
        else {
            write_file(&zipcontainer, file, outpath);
        }
    }
}

fn write_file(zipcontainer: &zip::ZipReader<fs::File>, file: &zip::ZipFile, outpath: Path)
{
    let mut outfile = fs::File::create(&outpath).unwrap();
    let mut reader = zipcontainer.read_file(file).unwrap();
    io::copy(&mut reader, &mut outfile).unwrap();
}

fn create_directory(outpath: Path)
{
    fs::create_dir_all(&outpath).unwrap();
}

fn sanitize_filename(filename: &str) -> Path
{
    let no_null_filename = match filename.find('\0') {
        Some(index) => &filename[0..index],
        None => filename,
    };

    Path::new(no_null_filename)
        .components()
        .skip_while(|component| *component == b"..")
        .fold(Path::new(""), |mut p, cur| {
            p.push(cur);
            p
        })
}
