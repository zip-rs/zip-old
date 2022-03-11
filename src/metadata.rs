use std::io;
// TODO: Provide useful alternatives:
//       - a `Name` that only saves the name of the file
//       - something that decodes some well-known extra fields - maybe in another crate?
pub struct Full {
    pub name: Vec<u8>,
}
impl<'a, 'b, D: io::Seek + io::Read> TryFrom<(&'a zip_format::DirectoryEntry, &'b mut D)> for Full {
    type Error = std::io::Error;
    fn try_from((entry, disk): (&zip_format::DirectoryEntry, &mut D)) -> io::Result<Self> {
        use io::Read;
        let mut name = vec![];
        disk.take(entry.name_len.get() as u64)
            .read_to_end(&mut name)?;
        disk.seek(std::io::SeekFrom::Current(
            entry.metadata_len.get() as i64 + entry.comment_len.get() as i64,
        ))?;
        Ok(Self { name })
    }
}
