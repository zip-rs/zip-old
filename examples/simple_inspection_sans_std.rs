fn main() -> Result<(), Error> {
    let disk = zip::DirectoryLocator::from_buf(include_bytes!("example.zip"))?;
    for file in disk.into_directory()?.iter() {
        println!("{}", core::str::from_utf8(file?.name())?);
    }
    Ok(())
}


#[derive(Debug)]
struct Error(Box<dyn core::fmt::Debug>);
impl<T: 'static + Clone + core::fmt::Debug> From<T> for Error {
    fn from(v: T) -> Self {
        Self(Box::new(v))
    }
}