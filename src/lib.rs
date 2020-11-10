//! ZIP [`Archive`]s
//! store files in a cross-platform format
//! to be saved and shared.
//!
//! The ZIP format was published by PKWARE in 1989, and its age shows.
//! It wasn't designed for the modern internet,
//! with no global timestamps
//! or reliable  streaming support.
//! However, over time it has crept into every corner of computing.
//!
//! It's implemented for
//! [just](https://docs.python.org/3/library/zipfile.htm)
//! [about](https://docs.microsoft.com/en-us/dotnet/api/system.io.compression.zipfil)
//! [every](https://docs.oracle.com/javase/8/docs/api/index.html?java/util/zip/package-summary.htm)
//! [environment](https://github.com/richgel999/miniz).
//! It's been extended into
//! JAR, Open Office XML,
//! and [EPUB](https://docs.rs/epub/).
//! Due to consumer OS support for ZIP files,
//! it's also perfect for  file distribution:
//! chances are you've downloaded one yourself.
//!
//! # Purpose
//!
//! This library focuses on
//! providing an ergonomic API for manipulating ZIP archives.
//! It makes it easy to create archives,
//! and is flexible enough to implement a more complex format.
//!
// 1. Ergonomics
//! - Ease of use comes first.
//!   The library will
//!   sacrifice some performance for the sake of an intuitive interface.
// 2. Correctness
//! - Archives will be accurately represented by the API.
//!   If the meaning of any metadata is unclear,
//!   the user will need to resolve it.
// 3. Speed
//! - Parsing should be competitive with other popular implementations.
//!   [See the benchmarks](#)
// 4. Permissiveness
//! - Invalid archives may be read if validation is not required,
//!   allowing more ZIP-like files to be used.
//!
//! # The Format
//!
//! ## Compression
//!
//! File compression is a way of reducing the size of your files
//! by removing redundant data.
//! ZIP uses a set of "lossless" compression algorithms
//! to make archives as small as possible.
//!
//! > Note: Compression algorithms are able to
//!         be configured and optimised for your files
//!         to use even less space
//!
//! ZIP supports many compression formats,
//! and this library is not compatible with all of them yet.
//! Luckily, most ZIP files share the same few formats.
//!
// TODO: List pros and cons of available formats
//!
//! ## Encryption
//!
//! It is also possible to encrypt the contents of a ZIP file.
//! Using this feature,
//! you can use a passphrase
//! to keep private information safe.
//!
// TODO: Add link to explanation
//! > **Be careful** when using this feature,
//!   especially to provide privacy to your users
//!     - ZIP encryption only protects a portion of the archive.
//!
// FIXME: Move these details to the implementation?
//! ### Traditional PKWARE Encryption
//!
//! Sometimes known as "ZipCrypto",
//! this has been [found to be vulnerable][Known Plaintext Attack].
//! Unfortunately, it is the most widely supported spec anyway,
//! and you'll find many archives using it.
// TODO: Explain this library's encryption features - we support decryption!
//!
//! ### AE-x
//!
//! Taking advantage of the well-known AES
//! (that's the Advanced Encryption Standard)
//! cipher,
//! this method can reliably secure files
//! so long as your recipient can read it.
//! For now it has very spotty support,
//! and is notably missing from Windows Explorer.
//!
//! ## Extensions
//!
//! Files in a ZIP archive
//! each have a header
//! containing a list of extensible fields
//! which contain third party metadata.
//! They don't have a defined format,
//! meaning this library cannot parse them
//! and leaves the task to the user.
//!
// TODO: Provide an example of implementing an extension
//!
//! # Examples
//!
//! ```no_run
//! # use std::io::File;
//! # use zip::Archive;
//! let mut archive = Archive::create(File::create("new.zip")?);
//! archive.insert("My Day.txt")?.write_all("It was a long one")?;
//! archive.finish()?;
//! ```
//!
//! [Known Plaintext Attack]: https://link.springer.com/content/pdf/10.1007%2F3-540-60590-8_12.pdf
use std::io;

pub mod metadata;

pub struct Archive<Data, Metadata = metadata::Name> {
    inner: Data,
    // TODO: decide on metadata storage. Do we want random access?
    metadata: Vec<Metadata>,
}

impl<D: io::Read + io::Seek, M> Archive<D, M> {
    // Builds an Archive from the Central Directory of a given file.
    pub fn open(source: D) -> io::Result<Self> {
        todo!()
    }
}

impl<D: io::Write, M> Archive<D, M> {
    // Initializes an empty archive
    pub fn create(sink: D) -> Self {
        Self {
            inner: sink,
            metadata: Vec::new(),
        }
    }
    // Creates a new file within the archive
    pub fn insert(&mut self, metadata: M) -> io::Result<File<&mut D, &mut M>> {
        self.metadata.push(metadata);
        let metadata = self.metadata.last_mut().unwrap();
        todo!("write `metadata` as a local header");
        Ok(File {
            inner: &mut self.inner,
            metadata,
        })
    }
    // Finalizes the archive, leaving the cursor at the end of the ZIP data
    pub fn into_inner(self) -> D {
        todo!()
    }
}

impl<D, M> Archive<D, M> {
    // Get a mutable reference to a file within the archive.
    pub fn get(
        &mut self,
        predicate: impl FnMut(File<&mut D, &mut M>) -> bool,
    ) -> Option<File<&mut D, &mut M>> {
        todo!()
    }
    /// # Warnings
    ///
    // TODO: Improve explanation
    /// The [`io`] traits
    /// are implemented for shared references to std types,
    /// which means you may read from
    /// the [`File`]s yielded by this iterator.
    ///
    /// Be careful with these references though
    /// - each one refers to the same object,
    /// and share its position.
    /// You can only read from
    /// the most recently yielded [`File`].
    ///
    /// ```
    /// // TODO: Example
    /// ```
    pub fn iter(&self) -> Iter<D, M> {
        Iter {
            inner: &self.inner,
            metadata: self.metadata.iter(),
        }
    }
}

// TODO: Figure out how users access the metadata. `Deref`? `file.meta()`?
//       Will modifications to the metadata be reflected in the local header?
pub struct File<Data, Metadata = ()> {
    inner: Data,
    metadata: Metadata,
}

impl<Data: io::Write, M> io::Write for File<Data, M> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        todo!()
    }
    fn flush(&mut self) -> io::Result<()> {
        todo!()
    }
}
impl<Data: io::Read, M> io::Read for File<Data, M> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        todo!()
    }
}

impl<'a, D, M> IntoIterator for &'a Archive<D, M> {
    type Item = File<&'a D, &'a M>;
    type IntoIter = Iter<'a, D, M>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// An iterator over
/// files within an [`Archive`]
///
/// # Warnings
///
/// See [`Archive::iter`]
pub struct Iter<'a, Data, Metadata> {
    inner: &'a Data,
    metadata: core::slice::Iter<'a, Metadata>,
}

impl<'a, D, M> Iterator for Iter<'a, D, M> {
    type Item = File<&'a D, &'a M>;
    fn next(&mut self) -> Option<Self::Item> {
        self.metadata.next().map(|metadata| File {
            inner: self.inner,
            metadata,
        })
    }
}
