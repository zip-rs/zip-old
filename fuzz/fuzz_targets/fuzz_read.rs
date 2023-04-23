#![no_main]
use libfuzzer_sys::fuzz_target;

fn decompress_all(data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let reader = std::io::Cursor::new(data);
    let mut zip = zip_next::ZipArchive::new(reader)?;

    for i in 0..zip.len() {
        let mut file = zip.by_index(i)?;
        if file.size() < 1 << 20 {
            let _ = std::io::copy(&mut file, &mut std::io::sink());
        }
    }

    Ok(())
}

fuzz_target!(|data: &[u8]| {
    let _ = decompress_all(data);
});
