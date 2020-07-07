//! Structs for creating a new zip archive

use crate::compression::CompressionMethod;
use crate::result::{ZipError, ZipResult};
use crate::spec;
use crate::types::{DateTime, System, ZipFileData, DEFAULT_VERSION};
use byteorder::{LittleEndian, WriteBytesExt};
use crc32fast::Hasher;
use std::default::Default;
use std::io;
use std::io::prelude::*;
use std::mem;

#[cfg(any(
    feature = "deflate",
    feature = "deflate-miniz",
    feature = "deflate-zlib"
))]
use flate2::write::DeflateEncoder;

#[cfg(feature = "bzip2")]
use bzip2::write::BzEncoder;

enum GenericZipWriter<W: Write>
{
    Closed,
    Storer(W),
    #[cfg(any(
        feature = "deflate",
        feature = "deflate-miniz",
        feature = "deflate-zlib"
    ))]
    Deflater(DeflateEncoder<W>),
    #[cfg(feature = "bzip2")]
    Bzip2(BzEncoder<W>),
}

/// Generator for ZIP files.
///
/// ```
/// fn doit() -> zip::result::ZipResult<()>
/// {
///     use std::io::Write;
///
///     // For this example we write to a buffer, but normally you should use a File
///     let mut buf: &mut [u8] = &mut [0u8; 65536];
///     let mut w = std::io::Cursor::new(buf);
///     let mut zip = zip::ZipWriter::new(w);
///
///     let options = zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Stored);
///     zip.start_file("hello_world.txt", options)?;
///     zip.write(b"Hello, World!")?;
///
///     // Optionally finish the zip. (this is also done on drop)
///     zip.finish()?;
///
///     Ok(())
/// }
///
/// println!("Result: {:?}", doit().unwrap());
/// ```
pub struct ZipWriter<W: Write>
{
    inner: GenericZipWriter<WriterWrapper<W>>,
    files: Vec<ZipFileData>,
    stats: ZipWriterStats,
    writing_to_file: bool,
    comment: String,
}

#[derive(Default)]
struct ZipWriterStats {
    hasher: Hasher,
    start: u64,
    bytes_written: u64,
}

/// Metadata for a file to be written
#[derive(Copy, Clone)]
pub struct FileOptions {
    compression_method: CompressionMethod,
    last_modified_time: DateTime,
    permissions: Option<u32>,
}

impl FileOptions {
    /// Construct a new FileOptions object
    pub fn default() -> FileOptions {
        FileOptions {
            #[cfg(any(
                feature = "deflate",
                feature = "deflate-miniz",
                feature = "deflate-zlib"
            ))]
            compression_method: CompressionMethod::Deflated,
            #[cfg(not(any(
                feature = "deflate",
                feature = "deflate-miniz",
                feature = "deflate-zlib"
            )))]
            compression_method: CompressionMethod::Stored,
            #[cfg(feature = "time")]
            last_modified_time: DateTime::from_time(time::now()).unwrap_or_default(),
            #[cfg(not(feature = "time"))]
            last_modified_time: DateTime::default(),
            permissions: None,
        }
    }

    /// Set the compression method for the new file
    ///
    /// The default is `CompressionMethod::Deflated`. If the deflate compression feature is
    /// disabled, `CompressionMethod::Stored` becomes the default.
    /// otherwise.
    pub fn compression_method(mut self, method: CompressionMethod) -> FileOptions {
        self.compression_method = method;
        self
    }

    /// Set the last modified time
    ///
    /// The default is the current timestamp if the 'time' feature is enabled, and 1980-01-01
    /// otherwise
    pub fn last_modified_time(mut self, mod_time: DateTime) -> FileOptions {
        self.last_modified_time = mod_time;
        self
    }

    /// Set the permissions for the new file.
    ///
    /// The format is represented with unix-style permissions.
    /// The default is `0o644`, which represents `rw-r--r--` for files,
    /// and `0o755`, which represents `rwxr-xr-x` for directories
    pub fn unix_permissions(mut self, mode: u32) -> FileOptions {
        self.permissions = Some(mode & 0o777);
        self
    }
}

impl Default for FileOptions {
    fn default() -> Self {
        Self::default()
    }
}

impl<W:Write> Write for ZipWriter<W>
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize>
    {
        if !self.writing_to_file { return Err(io::Error::new(io::ErrorKind::Other, "No file has been started")) }
       
        let write_result = self.inner.ref_mut()?.write(buf);
        if let Ok(count) = write_result {
            self.stats.update(&buf[0..count]);
        }
        write_result


        
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.ref_mut()?.flush()
    }
}

impl ZipWriterStats {
    fn update(&mut self, buf: &[u8]) {
        self.hasher.update(buf);
        self.bytes_written += buf.len() as u64;
    }
}

impl <W:Write> ZipWriter<W> {

    /// Initializes the ZipWriter.
    ///
    /// Before writing to this object, the start_file command should be called.
    pub fn new(inner: W) -> ZipWriter<W>
    where W: Seek
    {
        ZipWriter
        {
            inner: GenericZipWriter::Storer(WriterWrapper::new(inner)),
            files: Vec::new(),
            stats: Default::default(),
            writing_to_file: false,
            comment: String::new(),
        }
    }

    /// Initializes ZipWriter in streamming mode
    /// 
    /// It means that output stream may not be seekable (like stdout, pipe, socket ...).
    /// Zip format still supports this mode, local file header will contain 0 for file length and CRC32 
    /// and data descriptor record is added after file data containing actual CRC32 and file length
    pub fn new_streaming(inner: W) -> ZipWriter<W>
    {
        ZipWriter
        {
            inner: GenericZipWriter::Storer(WriterWrapper::new_unseekable(inner)),
            files: Vec::new(),
            stats: Default::default(),
            writing_to_file: false,
            comment: String::new(),
        }
    }

    /// Set ZIP archive comment. Defaults to an empty string if not set.
    pub fn set_comment<S>(&mut self, comment: S)
    where
        S: Into<String>,
    {
        self.comment = comment.into();
    }

    /// Start a new file for with the requested options.
    fn start_entry<S>(&mut self, name: S, options: FileOptions) -> ZipResult<()>
    where
        S: Into<String>,
    {
        self.finish_file()?;

        {
            let writer = self.inner.get_plain();
            let header_start = writer.current_position()?;

            let permissions = options.permissions.unwrap_or(0o100644);
            let file_name = name.into();
            let file_name_raw = file_name.clone().into_bytes();
            let mut file = ZipFileData {
                system: System::Unix,
                version_made_by: DEFAULT_VERSION,
                encrypted: false,
                compression_method: options.compression_method,
                last_modified_time: options.last_modified_time,
                crc32: 0,
                compressed_size: 0,
                uncompressed_size: 0,
                file_name,
                file_name_raw,
                file_comment: String::new(),
                header_start,
                data_start: 0,
                external_attributes: permissions << 16,
                streaming: ! writer.can_seek()
            };
            write_local_file_header(writer, &file)?;

            let header_end = writer.current_position()?;
            self.stats.start = header_end;
            file.data_start = header_end;

            self.stats.bytes_written = 0;
            self.stats.hasher = Hasher::new();

            self.files.push(file);
        }

        self.inner.switch_to(options.compression_method)?;

        Ok(())
    }

    fn finish_file(&mut self) -> ZipResult<()> {
        self.inner.switch_to(CompressionMethod::Stored)?;
        let writer = self.inner.get_plain();

        let file = match self.files.last_mut() {
            None => return Ok(()),
            Some(f) => f,
        };
        file.crc32 = self.stats.hasher.clone().finalize();
        file.uncompressed_size = self.stats.bytes_written;

        let file_end = writer.current_position()?;
        file.compressed_size = file_end - self.stats.start;

        if writer.can_seek() {
            update_local_file_header(writer, file)?;
            writer.seek_position(file_end)?;
        } else {
            add_data_descriptor_header(writer, file)?
        }

        self.writing_to_file = false;
        Ok(())
    }

    /// Starts a file.
    pub fn start_file<S>(&mut self, name: S, mut options: FileOptions) -> ZipResult<()>
    where
        S: Into<String>,
    {
        if options.permissions.is_none() {
            options.permissions = Some(0o644);
        }
        *options.permissions.as_mut().unwrap() |= 0o100000;
        self.start_entry(name, options)?;
        self.writing_to_file = true;
        Ok(())
    }

    /// Starts a file, taking a Path as argument.
    ///
    /// This function ensures that the '/' path seperator is used. It also ignores all non 'Normal'
    /// Components, such as a starting '/' or '..' and '.'.
    pub fn start_file_from_path(
        &mut self,
        path: &std::path::Path,
        options: FileOptions,
    ) -> ZipResult<()> {
        self.start_file(path_to_string(path), options)
    }

    /// Add a directory entry.
    ///
    /// You can't write data to the file afterwards.
    pub fn add_directory<S>(&mut self, name: S, mut options: FileOptions) -> ZipResult<()>
    where
        S: Into<String>,
    {
        if options.permissions.is_none() {
            options.permissions = Some(0o755);
        }
        *options.permissions.as_mut().unwrap() |= 0o40000;
        options.compression_method = CompressionMethod::Stored;

        let name_as_string = name.into();
        // Append a slash to the filename if it does not end with it.
        let name_with_slash = match name_as_string.chars().last() {
            Some('/') | Some('\\') => name_as_string,
            _ => name_as_string + "/",
        };

        self.start_entry(name_with_slash, options)?;
        self.writing_to_file = false;
        Ok(())
    }

    /// Add a directory entry, taking a Path as argument.
    ///
    /// This function ensures that the '/' path seperator is used. It also ignores all non 'Normal'
    /// Components, such as a starting '/' or '..' and '.'.
    pub fn add_directory_from_path(
        &mut self,
        path: &std::path::Path,
        options: FileOptions,
    ) -> ZipResult<()> {
        self.add_directory(path_to_string(path), options)
    }

    fn finalize(&mut self) -> ZipResult<()> {
        self.finish_file()?;

        {
            let writer = self.inner.get_plain();

            let central_start = writer.current_position()?;
            for file in self.files.iter()
            {
                write_central_directory_header(writer, file)?;
            }
            let central_size = writer.current_position()? - central_start;

            let footer = spec::CentralDirectoryEnd {
                disk_number: 0,
                disk_with_central_directory: 0,
                number_of_files_on_this_disk: self.files.len() as u16,
                number_of_files: self.files.len() as u16,
                central_directory_size: central_size as u32,
                central_directory_offset: central_start as u32,
                zip_file_comment: self.comment.as_bytes().to_vec(),
            };

            footer.write(writer)?;
        }

        Ok(())
    }

    /// Finish the last file and write all other zip-structures
    ///
    /// This will return the writer, but one should normally not append any data to the end of the file.
    /// Note that the zipfile will also be finished on drop.
    pub fn finish(&mut self) -> ZipResult<W>
    {
        self.finalize()?;
        let inner = mem::replace(&mut self.inner, GenericZipWriter::Closed);
        Ok(inner.unwrap().unwrap())
    }
}

impl<W:Write> Drop for ZipWriter<W>
{
    fn drop(&mut self)
    {
        if !self.inner.is_closed()
        {
            if let Err(e) = self.finalize() {
                let _ = write!(&mut io::stderr(), "ZipWriter drop failed: {:?}", e);
            }
        }
    }
}

impl<W: Write> GenericZipWriter<W>
{
    fn is_closed(&self) -> bool
    {
        match *self
        {
            GenericZipWriter::Closed => true,
            _ => false,
        }
    }


    fn switch_to(&mut self, compression: CompressionMethod) -> ZipResult<()>
    {
        match self.current_compression() {
            Some(method) if method == compression => return Ok(()),
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "ZipWriter was already closed",
                )
                .into())
            }
            _ => {}
        }

        let bare = match mem::replace(self, GenericZipWriter::Closed) {
            GenericZipWriter::Storer(w) => w,
            #[cfg(any(
                feature = "deflate",
                feature = "deflate-miniz",
                feature = "deflate-zlib"
            ))]
            GenericZipWriter::Deflater(w) => w.finish()?,
            #[cfg(feature = "bzip2")]
            GenericZipWriter::Bzip2(w) => w.finish()?,
            GenericZipWriter::Closed => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "ZipWriter was already closed",
                )
                .into())
            }
        };

        *self = {
            #[allow(deprecated)]
            match compression {
                CompressionMethod::Stored => GenericZipWriter::Storer(bare),
                #[cfg(any(
                    feature = "deflate",
                    feature = "deflate-miniz",
                    feature = "deflate-zlib"
                ))]
                CompressionMethod::Deflated => GenericZipWriter::Deflater(DeflateEncoder::new(
                    bare,
                    flate2::Compression::default(),
                )),
                #[cfg(feature = "bzip2")]
                CompressionMethod::Bzip2 => {
                    GenericZipWriter::Bzip2(BzEncoder::new(bare, bzip2::Compression::Default))
                }
                CompressionMethod::Unsupported(..) => {
                    return Err(ZipError::UnsupportedArchive("Unsupported compression"))
                }
            }
        };

        Ok(())
    }

    fn ref_mut(&mut self) -> io::Result<&mut dyn Write> {
        match *self {
            GenericZipWriter::Storer(ref mut w) => Ok(w as &mut dyn Write),
            #[cfg(any(
                feature = "deflate",
                feature = "deflate-miniz",
                feature = "deflate-zlib"
            ))]
            GenericZipWriter::Deflater(ref mut w) => Ok(w as &mut dyn Write),
            #[cfg(feature = "bzip2")]
            GenericZipWriter::Bzip2(ref mut w) => Ok(w as &mut dyn Write),
            GenericZipWriter::Closed => Err(io::Error::new(io::ErrorKind::BrokenPipe, "ZipWriter was already closed")),
        }
    }

    fn get_plain(&mut self) -> &mut W {
        match *self {
            GenericZipWriter::Storer(ref mut w) => w,
            _ => panic!("Should have switched to stored beforehand"),
        }
    }

    fn current_compression(&self) -> Option<CompressionMethod> {
        match *self {
            GenericZipWriter::Storer(..) => Some(CompressionMethod::Stored),
            #[cfg(any(
                feature = "deflate",
                feature = "deflate-miniz",
                feature = "deflate-zlib"
            ))]
            GenericZipWriter::Deflater(..) => Some(CompressionMethod::Deflated),
            #[cfg(feature = "bzip2")]
            GenericZipWriter::Bzip2(..) => Some(CompressionMethod::Bzip2),
            GenericZipWriter::Closed => None,
        }
    }

    fn unwrap(self) -> W {
        match self {
            GenericZipWriter::Storer(w) => w,
            _ => panic!("Should have switched to stored beforehand"),
        }
    }
}

fn calc_generic_flags(file: &ZipFileData) -> u16 {
    let mut flag = if !file.file_name.is_ascii() { 1u16 << 11 } else { 0 };
    if file.streaming { flag |= 0x08 }
    flag
}

fn write_local_file_header<T: Write>(writer: &mut T, file: &ZipFileData) -> ZipResult<()>
{
    // local file header signature
    writer.write_u32::<LittleEndian>(spec::LOCAL_FILE_HEADER_SIGNATURE)?;
    // version needed to extract
    writer.write_u16::<LittleEndian>(file.version_needed())?;
    // general purpose bit flag
    writer.write_u16::<LittleEndian>(calc_generic_flags(file))?;
    // Compression method
    #[allow(deprecated)]
    writer.write_u16::<LittleEndian>(file.compression_method.to_u16())?;
    // last mod file time and last mod file date
    writer.write_u16::<LittleEndian>(file.last_modified_time.timepart())?;
    writer.write_u16::<LittleEndian>(file.last_modified_time.datepart())?;
    // crc-32
    writer.write_u32::<LittleEndian>(file.crc32)?;
    // compressed size
    writer.write_u32::<LittleEndian>(file.compressed_size as u32)?;
    // uncompressed size
    writer.write_u32::<LittleEndian>(file.uncompressed_size as u32)?;
    // file name length
    writer.write_u16::<LittleEndian>(file.file_name.as_bytes().len() as u16)?;
    // extra field length
    let extra_field = build_extra_field(file)?;
    writer.write_u16::<LittleEndian>(extra_field.len() as u16)?;
    // file name
    writer.write_all(file.file_name.as_bytes())?;
    // extra field
    writer.write_all(&extra_field)?;

    Ok(())
}

fn update_local_file_header<W:Write>(writer: &mut WriterWrapper<W>, file: &ZipFileData) -> ZipResult<()>
{
    const CRC32_OFFSET : u64 = 14;
    writer.seek_position(file.header_start + CRC32_OFFSET)?;
    writer.write_u32::<LittleEndian>(file.crc32)?;
    writer.write_u32::<LittleEndian>(file.compressed_size as u32)?;
    writer.write_u32::<LittleEndian>(file.uncompressed_size as u32)?;
    Ok(())
}

fn write_central_directory_header<T: Write>(writer: &mut T, file: &ZipFileData) -> ZipResult<()> {
    // central file header signature
    writer.write_u32::<LittleEndian>(spec::CENTRAL_DIRECTORY_HEADER_SIGNATURE)?;
    // version made by
    let version_made_by = (file.system as u16) << 8 | (file.version_made_by as u16);
    writer.write_u16::<LittleEndian>(version_made_by)?;
    // version needed to extract
    writer.write_u16::<LittleEndian>(file.version_needed())?;
    // general puprose bit flag
    writer.write_u16::<LittleEndian>(calc_generic_flags(file))?;
    // compression method
    #[allow(deprecated)]
    writer.write_u16::<LittleEndian>(file.compression_method.to_u16())?;
    // last mod file time + date
    writer.write_u16::<LittleEndian>(file.last_modified_time.timepart())?;
    writer.write_u16::<LittleEndian>(file.last_modified_time.datepart())?;
    // crc-32
    writer.write_u32::<LittleEndian>(file.crc32)?;
    // compressed size
    writer.write_u32::<LittleEndian>(file.compressed_size as u32)?;
    // uncompressed size
    writer.write_u32::<LittleEndian>(file.uncompressed_size as u32)?;
    // file name length
    writer.write_u16::<LittleEndian>(file.file_name.as_bytes().len() as u16)?;
    // extra field length
    let extra_field = build_extra_field(file)?;
    writer.write_u16::<LittleEndian>(extra_field.len() as u16)?;
    // file comment length
    writer.write_u16::<LittleEndian>(0)?;
    // disk number start
    writer.write_u16::<LittleEndian>(0)?;
    // internal file attribytes
    writer.write_u16::<LittleEndian>(0)?;
    // external file attributes
    writer.write_u32::<LittleEndian>(file.external_attributes)?;
    // relative offset of local header
    writer.write_u32::<LittleEndian>(file.header_start as u32)?;
    // file name
    writer.write_all(file.file_name.as_bytes())?;
    // extra field
    writer.write_all(&extra_field)?;
    // file comment
    // <none>

    Ok(())
}

fn add_data_descriptor_header<T: Write>(writer: &mut T, file: &ZipFileData) -> ZipResult<()>
{
    // data_descriptor header signature
    writer.write_u32::<LittleEndian>(spec::DATA_DESCRIPTOR_SIGNATURE)?;
    // crc-32
    writer.write_u32::<LittleEndian>(file.crc32)?;
    // compressed size
    writer.write_u32::<LittleEndian>(file.compressed_size as u32)?;
    // uncompressed size
    writer.write_u32::<LittleEndian>(file.uncompressed_size as u32)?;

    Ok(())
}

fn build_extra_field(_file: &ZipFileData) -> ZipResult<Vec<u8>>
{
    let writer = Vec::new();
    // Future work
    Ok(writer)
}

fn path_to_string(path: &std::path::Path) -> String {
    let mut path_str = String::new();
    for component in path.components() {
        if let std::path::Component::Normal(os_str) = component {
            if !path_str.is_empty() {
                path_str.push('/');
            }
            path_str.push_str(&*os_str.to_string_lossy());
        }
    }
    path_str
}

// Here are types for making ZipWriter work with non-seekable streams 

type SeekFn<W> =  for<'a> fn(&'a mut W, io::SeekFrom)->io::Result<u64>;

enum WriterFlavour<W> {
    CanSeek{
        seek: SeekFn<W>
    },
    CannotSeek {
        pos: u64
    }
}

struct WriterWrapper<W> {
    inner: W,
    flavour: WriterFlavour<W>
}


impl <W:Write> WriterWrapper<W> {
    pub fn new_unseekable(inner: W) -> Self {
        WriterWrapper{
            inner,
            flavour: WriterFlavour::CannotSeek{ pos: 0}
        }
    }

    pub fn new(inner: W) -> Self 
    where W:Seek
    {
        WriterWrapper{
            inner,
            flavour: WriterFlavour::CanSeek{ seek: Seek::seek}
        }
    }

    fn can_seek(&self) -> bool {
        match self.flavour {
            WriterFlavour::CannotSeek{..} => false,
            WriterFlavour::CanSeek{..} => true
        }
    }

    fn current_position(&mut self) -> io::Result<u64> {
        match self.flavour {
            WriterFlavour::CannotSeek{pos} => Ok(pos),
            WriterFlavour::CanSeek{seek} => seek(&mut self.inner, io::SeekFrom::Current(0))
        }
    }

    fn seek_position(&mut self, pos: u64) -> io::Result<u64> {
        match self.flavour {
            WriterFlavour::CannotSeek{..} => Err(io::Error::new(io::ErrorKind::Other, "Unsupported operation")),
            WriterFlavour::CanSeek{seek} => seek(&mut self.inner, io::SeekFrom::Start(pos))
        }
        
    }

    fn unwrap(self) -> W {
        self.inner
    }
}

impl <W:Write> Write for WriterWrapper<W> {

    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        if let WriterFlavour::CannotSeek{ref mut pos} = &mut self.flavour  {
            *pos += n as u64
        };
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
mod test {
    use super::{FileOptions, ZipWriter};
    use crate::compression::CompressionMethod;
    use crate::types::DateTime;
    use std::io;
    use std::io::Write;
    use byteorder::{LittleEndian, ReadBytesExt};

    #[test]
    fn write_empty_zip() {
        let mut writer = ZipWriter::new(io::Cursor::new(Vec::new()));
        writer.set_comment("ZIP");
        let result = writer.finish().unwrap();
        assert_eq!(result.get_ref().len(), 25);
        assert_eq!(
            *result.get_ref(),
            [80, 75, 5, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 0, 90, 73, 80]
        );
    }

    #[test]
    fn write_zip_dir() {
        let mut writer = ZipWriter::new(io::Cursor::new(Vec::new()));
        writer
            .add_directory(
                "test",
                FileOptions::default().last_modified_time(
                    DateTime::from_date_and_time(2018, 8, 15, 20, 45, 6).unwrap(),
                ),
            )
            .unwrap();
        assert!(writer
            .write(b"writing to a directory is not allowed, and will not write any data")
            .is_err());
        let result = writer.finish().unwrap();
        assert_eq!(result.get_ref().len(), 108);
        assert_eq!(
            *result.get_ref(),
            &[
                80u8, 75, 3, 4, 20, 0, 0, 0, 0, 0, 163, 165, 15, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 5, 0, 0, 0, 116, 101, 115, 116, 47, 80, 75, 1, 2, 46, 3, 20, 0, 0, 0, 0, 0,
                163, 165, 15, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 237, 65, 0, 0, 0, 0, 116, 101, 115, 116, 47, 80, 75, 5, 6, 0, 0, 0, 0, 1, 0,
                1, 0, 51, 0, 0, 0, 35, 0, 0, 0, 0, 0,
            ] as &[u8]
        );
    }

    #[test]
    fn write_mimetype_zip() {
        let mut writer = ZipWriter::new(io::Cursor::new(Vec::new()));
        let options = FileOptions {
            compression_method: CompressionMethod::Stored,
            last_modified_time: DateTime::default(),
            permissions: Some(33188),
        };
        writer.start_file("mimetype", options).unwrap();
        writer
            .write(b"application/vnd.oasis.opendocument.text")
            .unwrap();
        let result = writer.finish().unwrap();

        assert_eq!(result.get_ref().len(), 153);
        let mut v = Vec::new();
        v.extend_from_slice(include_bytes!("../tests/data/mimetype.zip"));
        assert_eq!(result.get_ref(), &v);
    }

    #[test]
    fn path_to_string() {
        let mut path = std::path::PathBuf::new();
        #[cfg(windows)]
        path.push(r"C:\");
        #[cfg(unix)]
        path.push("/");
        path.push("windows");
        path.push("..");
        path.push(".");
        path.push("system32");
        let path_str = super::path_to_string(&path);
        assert_eq!(path_str, "windows/system32");
    }

        #[test]
    fn write_to_noseekable_stream() {
        let w = Vec::new();
        let mut zip = ZipWriter::new_streaming(w);
        let stored_opts = FileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("my_test", stored_opts).unwrap();
        zip.write("Sedm lumpu slohlo pumpu".as_bytes()).unwrap();
        zip.start_file("my_second_test", stored_opts).unwrap();
        zip.write("Usak suak susak kulisak je na vesak".as_bytes()).unwrap();
        let res = zip.finish().unwrap();
        assert!(res.len()>100);
        let read_u32 = |idx| {
             (&res[idx..]).read_u32::<LittleEndian>().unwrap()
        };
        let test_flag = |idx| {
            (&res[idx..]).read_u16::<LittleEndian>().unwrap() & 0x8 > 0
        };
        // has flag 3
        assert!(test_flag(6));
        // local file header lengths are empty
        assert_eq!(0, read_u32(18));
        assert_eq!(0, read_u32(22));
        // but there is data descriptor after file 
        let dd_start = 30+7+23;
        assert_eq!(crate::spec::DATA_DESCRIPTOR_SIGNATURE, read_u32(dd_start));
        assert!(read_u32(dd_start+4) != 0);
        assert_eq!(23, read_u32(dd_start+8));
        assert_eq!(23, read_u32(dd_start+12));

        // and correct data are also in files dictionary
        let fd_start = 171;
        assert!(test_flag(fd_start+8));
        assert_eq!(23, read_u32(fd_start+20));
        assert_eq!(23, read_u32(fd_start+24));
        // let mut f = std::fs::File::create("/tmp/my_zip_test.zip").unwrap();
        // f.write_all(&res).unwrap();

}
}
