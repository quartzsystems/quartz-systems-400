// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Certificates without a store, for any product (docs/BUILDKIT.md
//! section 17):
//!
//! - [`crypto`]: the private keys (ECDSA P-256 and P-384, RSA 2048 to
//!   4096), randomness, hashes, PBKDF2 and the PKCS#12 KDF.
//! - [`x509`]: what a certificate holds ([`x509::CertInfo`]: subject,
//!   issuer, names, dates, fingerprints), self-signed and issued
//!   certificates, requests ([`x509::build`]).
//! - [`server`]: a key and a self-signed server certificate for the
//!   names and validity the caller gives, in one call.
//! - [`bundle`]: a PEM chain and key read, matched and ordered.
//! - [`pem`] and [`pkcs12`]: PEM blocks; PKCS#12 files read (with their
//!   password) and written.
//!
//! Pure Rust: graviola for the keys, the `RustCrypto` crates for DER,
//! X.509 and PKCS#12's ciphers.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod bundle;
pub mod crypto;
mod error;
pub mod pem;
pub mod pkcs12;
pub mod server;
pub mod x509;

pub use error::{Error, Result};
