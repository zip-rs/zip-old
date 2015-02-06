#![feature(path, io, os, env, core)]

extern crate zip;

use std::old_io;

fn main()
{
    let args = std::env::args().map(|v| v.into_string().unwrap()).collect::<Vec<_>>();
    if args.len() < 2 {
        println!("Usage: {} <filename>", args[0]);
        std::env::set_exit_status(1);
        return;
    }
    let fname = Path::new(&*args[1]);
    let file = old_io::File::open(&fname).unwrap();

    let zipcontainer = zip::ZipReader::new(file).unwrap();

    for file in zipcontainer.files()
    {
        let outpath = sanitize_filename(&*file.file_name);
        println!("{}", outpath.display());

        let comment = &file.file_comment;
        if comment.len() > 0 { println!("  File comment: {}", comment); }

        old_io::fs::mkdir_recursive(&outpath.dir_path(), old_io::USER_DIR).unwrap();

        if (&*file.file_name).ends_with("/") {
            create_directory(outpath);
        }
        else {
            write_file(&zipcontainer, file, outpath);
        }
    }
}

fn write_file(zipcontainer: &zip::ZipReader<old_io::File>, file: &zip::ZipFile, outpath: Path)
{
    let mut outfile = old_io::File::create(&outpath);
    let mut reader = zipcontainer.read_file(file).unwrap();
    old_io::util::copy(&mut reader, &mut outfile).unwrap();
    old_io::fs::chmod(&outpath, old_io::USER_FILE).unwrap();
}

fn create_directory(outpath: Path)
{
    old_io::fs::mkdir_recursive(&outpath, old_io::USER_DIR).unwrap();
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
