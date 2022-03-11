# `zip`

> A library about ZIP files!

If you have some ZIP archives you need to mess about with in any way, this crate has you covered 😊

If you want to do literally anything else, get out of here. ZIP is terrible and you don't want to use it.

## Features

With the new 0.10 release, we (will be!) providing an API which is:
- `![no_std]`
- `![forbid(unsafe_code)]`
- able to inspect ZIP files without any dependencies

You can use [`zip::Archive`] for in-memory modification of a preexisting archive,
[`zip::files`] to view the contents of a zip without any allocations,
and even load archives split across multiple files using the [`zip::File::in_disk`] API.

## Heads up:

The new API does come with a bit of a drawback at the moment, though: the [`Persisted`] type
is splattered across the API, and uttlerly ruins the documentation of the crate.

TODO: Review what it'd be like to remove `Persisted`, and duplicated its API across the rest of the types.