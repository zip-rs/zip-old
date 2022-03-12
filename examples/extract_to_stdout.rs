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

    // open up the zip archive
    let mut footer = zip::Footer::read_from_io(std::fs::File::open(&path)?)?;

    // and prepare an extra file handle to read file contents
    let mut reader = footer
        .as_mut()
        .cloned()
        .with_disk(std::fs::File::open(&path)?);

    // also, allocate the structures we will need to use for decompression of all enabled methods
    let mut decompressor = zip::file::Decompressor::default();

    // look for the directory in this disk, and move to it
    let files = footer
        .into_directory()?
        .seek_to_files::<zip::metadata::Full>()?;
    for file in files {
        let file = file?;
        // print out the name of this file
        println!("{}", String::from_utf8_lossy(&file.name));

        // and subsequently look for it in the current disk
        // NOTE: `in_disk` could be pointed at another file for multi-file archives
        let file = file.in_disk(reader.as_mut().cloned())?;

        // construct the decompression state and seek to the file contents
        let mut data = file
            .into_data(&mut decompressor)?
            .seek_to_data(std::io::BufReader::new)?;
        // finally, read everything out of the archive!
        std::io::copy(&mut data, &mut std::io::stdout().lock())?;
    }

    Ok(())
}
