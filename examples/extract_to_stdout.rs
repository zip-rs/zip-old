use std::io;

pub fn main() -> io::Result<()> {
    let path = std::env::args().nth(1).expect("Usage: zip-extract <path>");
    
    let mut decompressor = Default::default();
    for file in &zip::Archive::open_at(path)? {
        // TODO: Rework the API to allow
        //   A) extractor.bufread(file)?.copy_to(stdout);
        //   B) file.extract_to(stdout)?;
        //   C) file.bufread()?.copy_to(stdout); // note that this requires an owned Read<'extractor>
        let mut reader = file
            .reader()?
            .remove_encryption_io()??
            .build_with_buffering(&mut decompressor, std::io::BufReader::new);
        io::copy(&mut reader,
                 &mut std::io::stdout())?;
    }

    Ok(())
}