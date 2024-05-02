use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    str::FromStr,
};
use zip::write::SimpleFileOptions;

fn gather_files<'a, T: Into<&'a Path>>(path: T, files: &mut Vec<PathBuf>) {
    let path: &Path = path.into();

    for entry in path.read_dir().unwrap() {
        match entry {
            Ok(e) => {
                if e.path().is_dir() {
                    gather_files(e.path().as_ref(), files);
                } else if e.path().is_file() {
                    files.push(e.path());
                }
            }
            Err(_) => todo!(),
        }
    }
}

fn real_main() -> i32 {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 3 {
        println!("Usage: {} <existing archive> <folder_to_append>", args[0]);
        return 1;
    }

    let existing_archive_path = &*args[1];
    let append_dir_path = &*args[2];
    let archive = PathBuf::from_str(existing_archive_path).unwrap();
    let to_append = PathBuf::from_str(append_dir_path).unwrap();

    let existing_zip = OpenOptions::new()
        .read(true)
        .write(true)
        .open(archive)
        .unwrap();
    let mut append_zip = zip::ZipWriter::new_append(existing_zip).unwrap();

    let mut files: Vec<PathBuf> = vec![];
    gather_files(to_append.as_ref(), &mut files);

    for file in files {
        append_zip
            .start_file(file.to_string_lossy(), SimpleFileOptions::default())
            .unwrap();

        let mut f = File::open(file).unwrap();
        let _ = std::io::copy(&mut f, &mut append_zip);
    }

    append_zip.finish().unwrap();

    0
}

fn main() {
    std::process::exit(real_main());
}
