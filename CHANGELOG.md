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

### Fixed

 - `deep_copy_file` could set incorrect Unix permissions.
 - `deep_copy_file` could handle files incorrectly if their compressed size was u32::MAX bytes or less but their
   uncompressed size was not.
 - Documented that `deep_copy_file` does not copy a directory's contents.
 
### Changed

 - Improved performance of `deep_copy_file`: it no longer searches for the filename twice.