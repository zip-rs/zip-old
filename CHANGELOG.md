# Changelog

## [0.6.4]

### Changed

 - [#333](https://github.com/zip-rs/zip/pull/333): disabled the default features of the `time` dependency, and also `formatting` and `macros`, as they were enabled by mistake.
 - Deprecated [`DateTime::from_time`](https://docs.rs/zip/0.6/zip/struct.DateTime.html#method.from_time) in favor of [`DateTime::try_from`](https://docs.rs/zip/0.6/zip/struct.DateTime.html#impl-TryFrom-for-DateTime)
 
## [0.6.6]

### Changed

 - Updated dependency versions.

### Added

 - `shallow_copy_file` method: copy a file from within the ZipWriter

## [0.6.7]

### Added

- `deep_copy_file` method: more standards-compliant way to copy a file from within the ZipWriter
