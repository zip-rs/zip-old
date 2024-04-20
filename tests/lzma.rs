#![cfg(feature = "lzma")]

use std::io::{self, Read};
use zip::ZipArchive;

#[test]
fn decompress_lzma() {
    let mut v = Vec::new();
    v.extend_from_slice(include_bytes!("data/lzma.zip"));
    let mut archive = ZipArchive::new(io::Cursor::new(v)).expect("couldn't open test zip file");

    let mut file = archive
        .by_name("hello.txt")
        .expect("couldn't find file in archive");
    assert_eq!("hello.txt", file.name());

    let mut content = Vec::new();
    file.read_to_end(&mut content)
        .expect("couldn't read encrypted and compressed file");
    assert_eq!("Hello world\n", String::from_utf8(content).unwrap());
}
