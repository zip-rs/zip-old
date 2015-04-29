zip-rs
======

[![Build Status](https://travis-ci.org/mvdnes/zip-rs.svg?branch=master)](https://travis-ci.org/mvdnes/zip-rs)
[![Build Status](https://api.shippable.com/projects/553fdfb4edd7f2c052d66b59/badge?branchName=master)](https://app.shippable.com/projects/553fdfb4edd7f2c052d66b59/builds/latest)
[![Crates.io version](https://img.shields.io/crates/v/zip.svg)](https://crates.io/crates/zip)

[Documentation](http://mvdnes.github.io/rust-docs/zip-rs/zip/index.html)

Info
----

A zip library for rust wich supports reading and writing of simple ZIP files.

Supported compression formats:

* stored (i.e. none)
* deflate
* bzip2

Currently unsupported zip extensions:

* ZIP64
* Encryption
* Multi-disk
