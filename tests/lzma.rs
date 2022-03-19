#![cfg(feature = "lzma")]

use std::io::{self, Read};
use zip::ZipArchive;

const CONTENT: &str = "Lorem ipsum dolor sit amet";

#[test]
fn lzma_file() {
    let mut v = Vec::new();
    v.extend_from_slice(include_bytes!("data/lzma_archive.zip"));
    let mut archive = ZipArchive::new(io::Cursor::new(v)).expect("couldn't open test zip file");

    let mut file = archive
        .by_name("ipsum.txt")
        .expect("couldn't find file in archive");

    let mut content = String::new();
    file.read_to_string(&mut content)
        .expect("couldn't read lzma file");
    assert_eq!(CONTENT, content);
}
