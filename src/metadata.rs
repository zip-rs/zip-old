use std::io;
use io::Read;

pub trait Metadata<D>: Sized {
    fn from_header(header: &zip_format::Header, disk: &mut D) -> io::Result<Self>;
    fn from_entry(entry: &zip_format::DirectoryEntry, disk: &mut D) -> io::Result<Self>;
}
// TODO: Provide useful alternatives:
//       - a `Name` that only saves the name of the file
//       - something that decodes some well-known extra fields - maybe in another crate?
pub struct Full {
    pub name: Vec<u8>,
}
impl<D: io::Seek + Read> Metadata<D> for Full {
    fn from_entry(entry: &zip_format::DirectoryEntry, disk: &mut D) -> io::Result<Self> {
        let mut name = vec![];
        disk.take(entry.name_len.get() as u64)
            .read_to_end(&mut name)?;
        disk.seek(std::io::SeekFrom::Current(
            entry.metadata_len.get() as i64 + entry.comment_len.get() as i64,
        ))?;
        Ok(Self { name })
    }
    fn from_header(header: &zip_format::Header, disk: &mut D) -> io::Result<Self> {
        let mut name = vec![];
        disk.take(header.name_len.get() as u64)
            .read_to_end(&mut name)?;
        disk.seek(std::io::SeekFrom::Current(
            header.metadata_len.get() as i64,
        ))?;
        Ok(Self { name })
    }
}
