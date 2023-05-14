# Self-Replace: A Utility For Self Replacing Executables

[![Crates.io](https://img.shields.io/crates/d/self-replace.svg)](https://crates.io/crates/self-replace)
[![License](https://img.shields.io/github/license/mitsuhiko/self-replace)](https://github.com/mitsuhiko/self-replace/blob/main/LICENSE)
[![rustc 1.61.0](https://img.shields.io/badge/rust-1.61%2B-orange.svg)](https://img.shields.io/badge/rust-1.61%2B-orange.svg)
[![Documentation](https://docs.rs/similar/badge.svg)](https://docs.rs/similar)

`self-replace` is a crate that allows binaries to replace themselves with newer
versions or to uninstall themselves.  On Unix systems this is a simple feat, but
on Windows a few hacks are needed which is why this crate exists.

This is a useful operation when working with single-executable utilties that want to implement a form of self updating or self uninstallation.

## Uninstallation

```rust
// uninstall
self_replace::self_delete()?;
```

## Updates

```rust
use std::fs;

let new_binary = "/path/to/new/binary";
self_replace::self_delete(&new_binary)?;
fs::remove_file(&new_binary)?;
```

## License and Links

* [Documentation](https://docs.rs/self-replace/)
* [Issue Tracker](https://github.com/mitsuhiko/self-replace/issues)
* [Examples](https://github.com/mitsuhiko/self-replace/tree/main/examples)
* License: [Apache-2.0](https://github.com/mitsuhiko/self-replace/blob/main/LICENSE)
