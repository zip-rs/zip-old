extern crate zip;

fn main()
{
    let args = std::os::args();
    let fname = Path::new(args[1].as_slice());
    let file = std::io::File::open(&fname);

    let mut zipcontainer = zip::reader::ZipContainer::new(file).unwrap();
    for i in zipcontainer.files()
    {
        println!("{}", i)
    }
}
