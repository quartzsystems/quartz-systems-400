// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Secure Boot signing for QS/400 (`docs/INSTALL.md`, "Secure Boot"):
//! Authenticode signatures on the UKIs with the QS/400 db key, checked
//! as the firmware checks them, and the key's certificate as DER and as
//! an EFI signature list for enrolment.
//!
//! - [`pe`]: the PE32+ layout, the Authenticode digest, the
//!   certificate table.
//! - [`sign`]: the PKCS#7 signature and its check.
//! - [`keys`]: the db key (release or development), new keys, the
//!   signature list.

#![forbid(unsafe_code)]

pub mod der;
pub mod keys;
pub mod pe;
pub mod sign;

pub use keys::DbKey;
pub use sign::sign;
pub use sign::verify::verify;

/// What can go wrong.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The image is not a PE32+ the signer can take.
    #[error("PE image: {0}")]
    Pe(&'static str),
    /// The key or certificate.
    #[error("db key: {0}")]
    Key(String),
    /// A signature that does not check.
    #[error("signature: {0}")]
    Signature(String),
}
