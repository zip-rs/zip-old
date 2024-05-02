use std::io;
use zip::ZipArchive;

#[test]
fn test_extended_timestamp() {
    let mut v = Vec::new();
    v.extend_from_slice(include_bytes!("../tests/data/extended_timestamp.zip"));
    let mut archive = ZipArchive::new(io::Cursor::new(v)).expect("couldn't open test zip file");

    for field in archive.by_name("test.txt").unwrap().extra_data_fields() {
        match field {
            zip::ExtraField::ExtendedTimestamp(ts) => {
                assert!(ts.ac_time().is_none());
                assert!(ts.cr_time().is_none());
                assert_eq!(*ts.mod_time().unwrap(), 1714635025);
            }
        }
    }
}
