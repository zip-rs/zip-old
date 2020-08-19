extern crate zip;

use std::fs;
use std::io;

use zip::ZipArchive;

// This tests extracting the contents of a zip file
#[test]
#[ignore]
fn extract() {
    let mut v = Vec::new();
    v.extend_from_slice(include_bytes!("../tests/data/files_and_dirs.zip"));
    let mut _archive = ZipArchive::new(io::Cursor::new(v)).expect("couldn't open test zip file");

    // archive
    //     .extract("test_directory")
    //     .expect("extract failed");

    // Cleanup
    fs::remove_dir_all("test_directory").expect("failed to remove extracted files");
}
