extern crate zip;

fn main()
{
    let args = std::os::args();
    if args.len() < 2 {
        println!("Usage: {} <filename>", args[0]);
        std::os::set_exit_status(1);
        return;
    }
    let fname = Path::new(args[1].as_slice());
    let file = std::io::File::open(&fname).unwrap();

    let zipcontainer = zip::ZipReader::new(file).unwrap();

    for file in zipcontainer.files()
    {
        let outpath = sanitize_filename(file.file_name.as_slice());
        println!("{}", outpath.display());

        let comment = &file.file_comment;
        if comment.len() > 0 { println!("  File comment: {}", comment); }

        std::io::fs::mkdir_recursive(&outpath.dir_path(), std::io::USER_DIR).unwrap();

        if file.file_name.as_slice().ends_with("/") {
            create_directory(outpath);
        }
        else {
            write_file(&zipcontainer, file, outpath);
        }
    }
}

fn write_file(zipcontainer: &zip::ZipReader<std::io::File>, file: &zip::ZipFile, outpath: Path)
{
    let mut outfile = std::io::File::create(&outpath);
    let mut reader = zipcontainer.read_file(file).unwrap();
    std::io::util::copy(&mut reader, &mut outfile).unwrap();
    std::io::fs::chmod(&outpath, std::io::USER_FILE).unwrap();
}

fn create_directory(outpath: Path)
{
    std::io::fs::mkdir_recursive(&outpath, std::io::USER_DIR).unwrap();
}

fn sanitize_filename(filename: &str) -> Path
{
    let no_null_filename = match filename.find('\0') {
        Some(index) => filename.slice_to(index),
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
