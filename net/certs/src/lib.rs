// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The certificate store: QS/400's equivalent of IBM's Digital
//! Certificate Manager, as a library.
//!
//! - [`store`]: a store file (`*SYSTEM`, the local CA's, any other) with
//!   server and client certificates and their keys, CAs and requests,
//!   sealed under a password; [`stash`] keeps that password sealed by
//!   the LIC partition's key.
//! - [`x509`]: certificate facts, self-signed and CA-issued certificates,
//!   requests; [`crypto`]: the keys (RSA 2048/3072/4096, ECDSA P-256 and
//!   P-384), AES-GCM, key derivation.
//! - [`ca`]: the local certificate authority.
//! - [`admin`]: the `QS*` commands as library functions, the expiry
//!   scan and the identity a server job gets for its application ID.
//! - [`apps`]: application IDs with their certificate, client
//!   authentication and CA trust settings.
//! - [`pem`] and [`pkcs12`]: import and export.
//! - [`roots`]: the shipped Mozilla root set.
//! - [`files`]: the file layer, implemented by the IFS; [`paths`]: where
//!   everything lives.
//!
//! See docs/CERTS.md.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod admin;
pub mod apps;
pub mod ca;
pub mod crypto;
mod error;
pub mod files;
pub mod paths;
pub mod pem;
pub mod pkcs12;
pub mod roots;
pub mod stash;
pub mod store;
pub mod x509;

pub use error::{CertError, Result};
