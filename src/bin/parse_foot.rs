extern crate zip;

fn main()
{
    let args = std::os::args();
    let fname = Path::new(args[1].as_slice());
    let mut file = std::io::File::open(&fname);

    let header = zip::spec::CentralDirectoryEnd::find_and_parse(&mut file).unwrap();
    println!("{}", header);

    file.seek(header.central_directory_offset as i64, std::io::SeekSet).unwrap();
    for i in range(0, header.number_of_files_on_this_disk)
    {
        println!("{}", zip::spec::CentralDirectoryHeader::parse(&mut file).unwrap());
    }
}
