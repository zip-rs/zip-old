# 0.10.0-alpha.1

#### FULL LIST OF CHANGES:

Features:

- load archives with `zip::Archive::open_at`, collecting the file metadata which can be accessed through iteration
- read, decompress, and optionally decrypt files with `zip::file::File::reader`