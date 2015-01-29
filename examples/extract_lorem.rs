#![allow(unstable)]

extern crate zip;

fn main()
{
    let args = std::os::args();
    if args.len() < 2 {
        println!("Usage: {} <filename>", args[0]);
        std::os::set_exit_status(1);
        return;
    }
    let fname = Path::new(&*args[1]);
    let file = std::old_io::File::open(&fname).unwrap();

    let zipcontainer = zip::ZipReader::new(file).unwrap();
    
    let file = match zipcontainer.get("test/lorem_ipsum.txt")
    {
        Some(file) => file,
        None => { println!("File test/lorem_ipsum.txt not found"); return }
    };

    let data = zipcontainer.read_file(file).unwrap().read_to_end().unwrap();
    let contents = String::from_utf8(data).unwrap();
    println!("{}", contents);
}
