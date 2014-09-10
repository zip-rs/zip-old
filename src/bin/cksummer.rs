extern crate zip;

fn main()
{
    let mut stdin = std::io::stdin();
    let mut crc = 0u32;
    let mut buf = [0u8, ..4096];

    loop
    {
        match stdin.read(&mut buf)
        {
            Err(_) => break,
            Ok(n) => { crc = zip::crc32::crc32(crc, buf.slice_to(n)); },
        }
    }

    println!("{:x}", crc);
}
