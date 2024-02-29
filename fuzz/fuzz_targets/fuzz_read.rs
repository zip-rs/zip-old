#![no_main]
use libfuzzer_sys::fuzz_target;
use std::io::Read;

const MAX_BYTES_TO_READ: u64 = 1 << 24;

fn decompress_all(data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let reader = std::io::Cursor::new(data);
    let mut zip = zip_next::ZipArchive::new(reader)?;

    for i in 0..zip.len() {
        let file = zip.by_index(i)?;
        let expected_bytes = file.size().min(MAX_BYTES_TO_READ);
        let result = std::io::copy(&mut file.take(MAX_BYTES_TO_READ), &mut std::io::sink());
        if let Ok(bytes) = result {
            assert!(bytes <= expected_bytes)
        }
    }

    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let _ = decompress_all(data);
});
