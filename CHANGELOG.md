# Changelog

## [0.6.4]

### Changed

 - [#333](https://github.com/zip-rs/zip/pull/333): disabled the default features of the `time` dependency, and also `formatting` and `macros`, as they were enabled by mistake.
 - Deprecated [`DateTime::from_time`](https://docs.rs/zip/0.6/zip/struct.DateTime.html#method.from_time) in favor of [`DateTime::try_from`](https://docs.rs/zip/0.6/zip/struct.DateTime.html#impl-TryFrom-for-DateTime)

## [0.6.5]

### Added

 - `shallow_copy_file` method: copy a file from within the ZipWriter
 
## [0.6.6]

### Fixed

 - Unused flag `#![feature(read_buf)]` was breaking compatibility with stable compiler.

### Changed

 - Updated dependency versions.

## [0.6.7]

### Added

 - `deep_copy_file` method: more standards-compliant way to copy a file from within the ZipWriter

## [0.6.8]

### Added

 - Detects duplicate filenames.

### Fixed

 - `deep_copy_file` could set incorrect Unix permissions.
 - `deep_copy_file` could handle files incorrectly if their compressed size was u32::MAX bytes or less but their
   uncompressed size was not.
 - Documented that `deep_copy_file` does not copy a directory's contents.
 
### Changed

 - Improved performance of `deep_copy_file` by using a HashMap and eliminating a redundant search.

## [0.6.9]

### Fixed

 - Fixed an issue that prevented `ZipWriter` from implementing `Send`.

## [0.6.10]

### Changed

 - Updated dependency versions.

## [0.6.11]

### Fixed

 - Fixed a bug that could cause later writes to fail after a `deep_copy_file` call.

## [0.6.12]

### Fixed

 - Fixed a Clippy warning that was missed during the last release.

## [0.6.13]

### Fixed

 - Fixed a possible bug in deep_copy_file.

## [0.7.0]

### Fixed

 - Calling `start_file` with invalid parameters no longer closes the `ZipWriter`.
 - Attempting to write a 4GiB file without calling `FileOptions::large_file(true)` now removes the file from the archive
   but does not close the `ZipWriter`.
 - Attempting to write a file with an unrepresentable or invalid last-modified date will instead add it with a date of
   1980-01-01 00:00:00.

### Added

 - Method `is_writing_file` - indicates whether a file is open for writing.

## [0.7.0.1]

### Changed

 - Bumped the version number in order to upload an updated README to crates.io.