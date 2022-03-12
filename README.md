# `zip`

> A library about ZIP files!

If you have some ZIP archives you need to mess about with in any way, this crate has you covered 😊

If you want to do literally anything else, get out of here. ZIP is terrible and you don't want to use it.

## Features

With the new 0.10 release, we (will be!) providing an API which is:
- `![no_std]`
- `![forbid(unsafe_code)]`
- able to inspect ZIP files without any dependencies

You can use [`Archive`] for in-memory modification of a preexisting archive,
[`files`] to view the contents of a zip without any allocations,
and even load archives split across multiple files using the [`file::File::in_disk`] API.
