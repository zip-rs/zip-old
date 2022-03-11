use std::io;

/// TODO: slim down the API so that this file can be written somewhat like:
/// ```
/// let archive = "archive.zip";
/// let reader = io::BufReader::new(fs::File::open(archive)?);
/// let out = std::io::stdout().lock();
/// for file in zip::files(fs::File::open(archive))? {
///     out.write(&file.name)?;
///     file?.with_store(&mut reader).copy_to(out)?;
/// }
/// ```
pub fn main() -> io::Result<()> {
    let path = std::env::args().nth(1).expect("Usage: zip-extract <path>");
    
    // BUF: Throughout this example, the BUF: comments show the few calls you need to change to
    //      switch the zip parser to in-memory mode.
    // BUF: let buf = std::fs::read(&path)?;
    
    // open up the zip archive
    // BUF: let mut footer = zip::Footer::read_from_buf(&buf)?;
    let mut footer = zip::Footer::read_from_io(std::fs::File::open(&path)?)?;

    // and prepare an extra file handle to read file contents
    let mut reader = footer
        // BUF: .map_disk(std::io::Cursor::new);
        .as_mut()
        .cloned()
        .with_disk(std::fs::File::open(&path)?);

    // also, allocate the structures we will need to use for decompression of all enabled methods
    let mut decompressor = zip::file::Decompressor::default();

    // look for the directory in this disk, and move to it
    let files = footer
        .into_directory()?
        // BUF: .map_disk(std::io::Cursor::new)
        .seek_to_files::<zip::metadata::Full>()?;
    for file in files {
        let file = file?;
        // print out the name of this file
        println!("{}", String::from_utf8_lossy(&file.name));

        // and subsequently look for it in the current disk
        // NOTE: `in_disk` could be pointed at another file for multi-file archives
        // BUF: let file = file.in_disk(reader.clone())?;
        let file = file.in_disk(reader.as_mut().cloned())?;
        
        
        // finally, read everything out of the archive!
        let mut data = file
            // construct the decompression state
            .into_data(&mut decompressor)?
            // and, if successful, seek to the file contents
            // BUF: .seek_to_data(|b| b)?;
            .seek_to_data(std::io::BufReader::new)?;
        std::io::copy(&mut data, &mut std::io::stdout().lock())?;
    }

    Ok(())
}
