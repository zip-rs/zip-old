extern crate zip;

fn main()
{
    let args = std::os::args();
    let fname = Path::new(args[1].as_slice());
    let file = std::io::File::open(&fname);

    let zipcontainer = zip::ZipReader::new(file).unwrap();

    for file in zipcontainer.files()
    {
        println!("{}", file.file_name);
        let comment = &file.file_comment;
        if comment.len() > 0 { println!("  File comment: {}", comment); }

        if file.uncompressed_size == 0 { continue }

        let outpath = Path::new(file.file_name.as_slice());
        let dirname = Path::new(outpath.dirname());

        std::io::fs::mkdir_recursive(&dirname, std::io::USER_DIR).unwrap();

        let mut outfile = std::io::File::create(&outpath);
        let mut reader = zipcontainer.read_file(file);
        std::io::util::copy(&mut reader, &mut outfile).unwrap();
    }
}
