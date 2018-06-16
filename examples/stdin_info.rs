extern crate zip;

use std::io;

fn main() {
    std::process::exit(real_main());
}

fn real_main() -> i32 {
    let stdin = io::stdin();
    let mut stdin_handle = stdin.lock();

    loop {
        match zip::read::read_zipfile_from_stream(&mut stdin_handle) {
            Ok(None) => break,
            Ok(Some(file)) => {
                println!("{}: {} bytes ({} bytes packed)", file.name(), file.size(), file.compressed_size());
            },
            Err(e) => {
                println!("Error encountered while reading zip: {:?}", e);
                return 1;
            },
        }
    }
    return 0;
}
