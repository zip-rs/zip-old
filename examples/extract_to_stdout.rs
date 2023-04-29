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

    // open up the zip archive and prepare an extra file handle to read the file directory
    let footer = zip::Footer::from_io(std::fs::File::open(&path)?)?;
    let mut reader = footer.with_disk(std::fs::File::open(&path)?);

    // also, allocate the structures we will use for decompression
    let mut datastore = zip::file::Store::default();

    // look for the directory in this disk, and start scanning it
    let files = footer
        .into_directory()?
        .seek_to_files::<zip::metadata::std::Full>()?;
    for file in files {
        // resolve the files within the open archive
        // NOTE: `in_disk` could be pointed at another file for multi-file archives
        let file = file?.in_disk(reader.as_mut())?;
        println!("{}", String::from_utf8_lossy(file.name()));

        // construct the decompression state and seek to the file contents
        let mut data = file
            .reader()?
            .seek_to_data()?
            .unlock_io()?
            .map(|d| Ok(d.into()))
            .unwrap_or_else(|d| {
                // If the file is locked, try to decrypt it with this password
                d.try_password(b"password")
            })?
            .build_io(&mut datastore, std::io::BufReader::new);
        // finally, read everything out of the archive!
        std::io::copy(&mut data, &mut std::io::stdout().lock())?;
    }

    Ok(())
}
