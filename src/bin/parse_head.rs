extern crate zip;

fn main()
{
    let mut stdin = std::io::stdin();
    let header = zip::spec::LocalFileHeader::parse(&mut stdin).unwrap();
    println!("{}", header);
    println!("{}", String::from_utf8(header.file_name));
}
