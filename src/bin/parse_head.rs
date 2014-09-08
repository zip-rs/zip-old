extern crate zip;

fn main()
{
    let args = std::os::args();
    let fname = Path::new(args[1].as_slice());
    let mut file = std::io::File::open(&fname);

    let header = zip::spec::LocalFileHeader::parse(&mut file).unwrap();
    println!("{}", header);
    println!("{:x}", header.crc32);
    println!("{}", String::from_utf8(header.file_name.clone()));
}
