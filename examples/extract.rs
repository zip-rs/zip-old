extern crate zip;

use std::io;
use std::fs;

fn main() {
    std::process::exit(real_main());
}

fn real_main() -> i32
{
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        println!("Usage: {} <filename>", args[0]);
        return 1;
    }
    let fname = std::path::Path::new(&*args[1]);
    let file = fs::File::open(&fname).unwrap();

    let mut archive = zip::ZipArchive::new(file).unwrap();

    for i in 0..archive.len()
    {
        let mut file = archive.by_index(i).unwrap();
        let outpath = sanitize_filename(file.name());
        println!("{}", outpath.display());

        {
            let comment = file.comment();
            if comment.len() > 0 { println!("  File comment: {}", comment); }
        }

        create_directory(outpath.parent().unwrap_or(std::path::Path::new("")));

        if (&*file.name()).ends_with("/") {
            create_directory(&outpath);
        }
        else {
            write_file(&mut file, &outpath);
        }
    }

    return 0;
}

fn write_file(reader: &mut zip::read::ZipFile, outpath: &std::path::Path)
{
    let mut outfile = fs::File::create(&outpath).unwrap();
    io::copy(reader, &mut outfile).unwrap();
}

fn create_directory(outpath: &std::path::Path)
{
    fs::create_dir_all(&outpath).unwrap();
}

fn sanitize_filename(filename: &str) -> std::path::PathBuf
{
    let no_null_filename = match filename.find('\0') {
        Some(index) => &filename[0..index],
        None => filename,
    };

    std::path::Path::new(no_null_filename)
        .components()
        .filter(|component| *component != std::path::Component::ParentDir)
        .fold(std::path::PathBuf::new(), |mut path, ref cur| {
            path.push(cur.as_os_str());
            path
        })
}
