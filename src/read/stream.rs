use std::fs;
use std::io::{self, Read};
use std::path::Path;

use super::{
    central_header_to_zip_file_inner, read_zipfile_from_stream, spec, ZipError, ZipFile,
    ZipFileData, ZipResult,
};

use byteorder::{LittleEndian, ReadBytesExt};

/// Stream decoder for zip.
#[derive(Debug)]
pub struct ZipStreamReader<R>(R);

impl<R> ZipStreamReader<R> {
    /// Create a new ZipStreamReader
    pub fn new(reader: R) -> Self {
        Self(reader)
    }
}

impl<R: Read> ZipStreamReader<R> {
    fn parse_central_directory(&mut self) -> ZipResult<Option<ZipStreamFileMetadata>> {
        // Give archive_offset and central_header_start dummy value 0, since
        // they are not used in the output.
        let archive_offset = 0;
        let central_header_start = 0;

        // Parse central header
        let signature = self.0.read_u32::<LittleEndian>()?;
        if signature != spec::CENTRAL_DIRECTORY_HEADER_SIGNATURE {
            Ok(None)
        } else {
            central_header_to_zip_file_inner(&mut self.0, archive_offset, central_header_start)
                .map(ZipStreamFileMetadata)
                .map(Some)
        }
    }

    /// Iteraate over the stream and extract all file and their
    /// metadata.
    pub fn visit<V: ZipStreamVisitor>(mut self, visitor: &mut V) -> ZipResult<()> {
        while let Some(mut file) = read_zipfile_from_stream(&mut self.0)? {
            visitor.visit_file(&mut file)?;
        }

        while let Some(metadata) = self.parse_central_directory()? {
            visitor.visit_additional_metadata(&metadata)?;
        }

        Ok(())
    }

    /// Extract a Zip archive into a directory, overwriting files if they
    /// already exist. Paths are sanitized with [`ZipFile::enclosed_name`].
    ///
    /// Extraction is not atomic; If an error is encountered, some of the files
    /// may be left on disk.
    pub fn extract<P: AsRef<Path>>(self, directory: P) -> ZipResult<()> {
        struct Extracter<'a>(&'a Path);
        impl ZipStreamVisitor for Extracter<'_> {
            fn visit_file(&mut self, file: &mut ZipFile<'_>) -> ZipResult<()> {
                let filepath = file
                    .enclosed_name()
                    .ok_or(ZipError::InvalidArchive("Invalid file path"))?;

                let outpath = self.0.join(filepath);

                if file.name().ends_with('/') {
                    fs::create_dir_all(&outpath)?;
                } else {
                    if let Some(p) = outpath.parent() {
                        fs::create_dir_all(&p)?;
                    }
                    let mut outfile = fs::File::create(&outpath)?;
                    io::copy(file, &mut outfile)?;
                }

                Ok(())
            }

            fn visit_additional_metadata(
                &mut self,
                metadata: &ZipStreamFileMetadata,
            ) -> ZipResult<()> {
                #[cfg(unix)]
                {
                    let filepath = metadata
                        .enclosed_name()
                        .ok_or(ZipError::InvalidArchive("Invalid file path"))?;

                    let outpath = self.0.join(filepath);

                    use std::os::unix::fs::PermissionsExt;
                    if let Some(mode) = metadata.unix_mode() {
                        fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
                    }
                }

                Ok(())
            }
        }

        self.visit(&mut Extracter(directory.as_ref()))
    }
}

/// Visitor for ZipStreamReader
pub trait ZipStreamVisitor {
    ///  * `file` - contains the content of the file and most of the metadata,
    ///    except:
    ///     - `comment`: set to an empty string
    ///     - `data_start`: set to 0
    ///     - `external_attributes`: `unix_mode()`: will return None
    fn visit_file(&mut self, file: &mut ZipFile<'_>) -> ZipResult<()>;

    /// This function is guranteed to be called after all `visit_file`s.
    ///
    ///  * `metadata` - Provides missing metadata in `visit_file`.
    fn visit_additional_metadata(&mut self, metadata: &ZipStreamFileMetadata) -> ZipResult<()>;
}

/// Additional metadata for the file.
#[derive(Debug)]
pub struct ZipStreamFileMetadata(ZipFileData);

impl ZipStreamFileMetadata {
    /// Get the name of the file
    ///
    /// # Warnings
    ///
    /// It is dangerous to use this name directly when extracting an archive.
    /// It may contain an absolute path (`/etc/shadow`), or break out of the
    /// current directory (`../runtime`). Carelessly writing to these paths
    /// allows an attacker to craft a ZIP archive that will overwrite critical
    /// files.
    ///
    /// You can use the [`ZipFile::enclosed_name`] method to validate the name
    /// as a safe path.
    pub fn name(&self) -> &str {
        &self.0.file_name
    }

    /// Get the name of the file, in the raw (internal) byte representation.
    ///
    /// The encoding of this data is currently undefined.
    pub fn name_raw(&self) -> &[u8] {
        &self.0.file_name_raw
    }

    /// Rewrite the path, ignoring any path components with special meaning.
    ///
    /// - Absolute paths are made relative
    /// - [`ParentDir`]s are ignored
    /// - Truncates the filename at a NULL byte
    ///
    /// This is appropriate if you need to be able to extract *something* from
    /// any archive, but will easily misrepresent trivial paths like
    /// `foo/../bar` as `foo/bar` (instead of `bar`). Because of this,
    /// [`ZipFile::enclosed_name`] is the better option in most scenarios.
    ///
    /// [`ParentDir`]: `Component::ParentDir`
    pub fn mangled_name(&self) -> ::std::path::PathBuf {
        self.0.file_name_sanitized()
    }

    /// Ensure the file path is safe to use as a [`Path`].
    ///
    /// - It can't contain NULL bytes
    /// - It can't resolve to a path outside the current directory
    ///   > `foo/../bar` is fine, `foo/../../bar` is not.
    /// - It can't be an absolute path
    ///
    /// This will read well-formed ZIP files correctly, and is resistant
    /// to path-based exploits. It is recommended over
    /// [`ZipFile::mangled_name`].
    pub fn enclosed_name(&self) -> Option<&Path> {
        self.0.enclosed_name()
    }

    /// Get the comment of the file
    pub fn comment(&self) -> &str {
        &self.0.file_comment
    }

    /// Get the starting offset of the data of the compressed file
    pub fn data_start(&self) -> u64 {
        self.0.data_start.load()
    }

    /// Get unix mode for the file
    pub fn unix_mode(&self) -> Option<u32> {
        self.0.unix_mode()
    }
}
