zip
===

[![Crates.io](https://img.shields.io/crates/v/zip.svg)](https://crates.io/crates/zip)


A Rusty API for manipulating ZIP archives

Usage
-----

```toml
[dependencies]
zip = "0.7"
```

MSRV
----

Our current Minimum Supported Rust Version is **1.34.0**. When adding features,
we will follow these guidelines:

- We will always support the latest four minor Rust versions. This gives you a 6
  month window to upgrade your compiler.
- Any change to the MSRV will be accompanied with a **minor** version bump
   - While the crate is pre-1.0, this will be a change to the PATCH version.
