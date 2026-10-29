// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TLS engine: rustls with the pure-Rust graviola provider, fed by
//! the certificate store and the system values, with the `GSKit` and
//! `SSL_` API semantics IBM i programs use.
//!
//! - [`policy`]: `QSSLPCL`, `QSSLCSL`, `QSSLCSLCTL`; [`suites`]: IBM's
//!   cipher names and codes.
//! - [`config`]: server and client configurations from the store, a
//!   label or an application ID.
//! - [`stream`]: blocking TLS over any `Read + Write`; [`engine`]: the
//!   non-blocking state machine servers drive.
//! - [`gsk`]: `gsk_*` semantics; [`ssl`]: the older `SSL_*` semantics.
//!   Both are plain Rust the ILE binding layer wraps (docs/CERTS.md).

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod config;
pub mod engine;
mod error;
pub mod gsk;
pub mod policy;
pub mod ssl;
pub mod stream;
pub mod suites;

pub use error::{Result, TlsError};
pub use rustls;
