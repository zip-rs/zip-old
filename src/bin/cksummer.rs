extern crate zip;

fn main()
{
    let stdin = std::io::stdin();
    let mut crc_reader = zip::crc32::Crc32Reader::new(stdin);

    let mut buf = [0u8, ..4096];
    loop
    {
        match crc_reader.read(&mut buf)
        {
            Err(_) => break,
            _ => {},
        }
    }
    crc_reader.read_to_end().unwrap();

    println!("{:x}", crc_reader.get_checksum());
}
