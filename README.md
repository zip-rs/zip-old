**Unfortunately, due to a lack of time and loss of interest, this project will no longer be actively maintained.**

zip-rs
======

[![Build Status](https://travis-ci.org/mvdnes/zip-rs.svg?branch=master)](https://travis-ci.org/mvdnes/zip-rs)
[![Build status](https://ci.appveyor.com/api/projects/status/gsnpqcodg19iu253/branch/master?svg=true)](https://ci.appveyor.com/project/mvdnes/zip-rs/branch/master)
[![Crates.io version](https://img.shields.io/crates/v/zip.svg)](https://crates.io/crates/zip)

[Documentation](http://mvdnes.github.io/rust-docs/zip-rs/zip/index.html)


Info
----

A zip library for rust which supports reading and writing of simple ZIP files.

Supported compression formats:

* stored (i.e. none)
* deflate
* bzip2

Currently unsupported zip extensions:

* Encryption
* Multi-disk

Usage
-----

With all default features:

```toml
[dependencies]
zip = "0.5"
```

Without the default features:

```toml
[dependencies]
zip = { version = "0.5", default-features = false }
```

The features available are:

* `deflate`: Enables the deflate compression algorithm, which is the default for zipfiles
* `bzip2`: Enables the BZip2 compression algorithm.
* `time`: Enables features using the [time](https://github.com/rust-lang-deprecated/time) crate.

All of these are enabled by default.

Examples
--------

See the [examples directory](examples) for:
   * How to write a file to a zip.
   * how to write a directory of files to a zip (using [walkdir](https://github.com/BurntSushi/walkdir)).
   * How to extract a zip file.
   * How to extract a single file from a zip.
   * How to read a zip from the standard input.
