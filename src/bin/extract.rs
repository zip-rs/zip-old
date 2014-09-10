extern crate zip;

fn main()
{
    let args = std::os::args();
    let fname = Path::new(args[1].as_slice());
    let file = std::io::File::open(&fname);

    let zipcontainer = zip::reader::ZipContainer::new(file).unwrap();

    for file in zipcontainer.files()
    {
        println!("{}", file.file_name_string());

        if file.uncompressed_size == 0 { continue }

        let outpath = Path::new(file.file_name.as_slice());
        let dirname = Path::new(outpath.dirname());

        std::io::fs::mkdir_recursive(&dirname, std::io::UserDir).unwrap();

        let mut outfile = std::io::File::create(&outpath);
        let mut reader = zipcontainer.read_file(file);
        copy(&mut reader, &mut outfile).unwrap();
    }
}

fn copy<R: Reader, W: Writer>(reader: &mut R, writer: &mut W) -> std::io::IoResult<()>
{
    let mut buffer = [0u8, ..4096];
    loop
    {
        match reader.read(&mut buffer)
        {
            Err(ref e) if e.kind == std::io::EndOfFile => break,
            Ok(n) => try!(writer.write(buffer.slice_to(n))),
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
