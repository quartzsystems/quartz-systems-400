// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The XDR description compiler (docs/NFS.md section 3): a description
//! in the XDR language is parsed into a model and written out as Rust
//! types that encode and decode themselves.

#![deny(missing_docs)]
#![forbid(unsafe_code)]
