// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Error type.

use std::io;

use thiserror::Error;

/// Anything making or reading certificates and keys can fail with.
#[derive(Debug, Error)]
pub enum Error {
    /// Reading or writing failed.
    #[error("certificate store I/O: {0}")]
    Io(#[from] io::Error),
    /// The password does not open a PKCS#12 file or an encrypted key.
    #[error("the password is not correct")]
    BadPassword,
    /// A certificate, request or key failed to parse.
    #[error("malformed data: {0}")]
    Malformed(String),
    /// A key or signature operation failed.
    #[error("cryptographic operation failed: {0}")]
    Crypto(String),
    /// The input uses something this crate does not support.
    #[error("not supported: {0}")]
    Unsupported(String),
    /// A request was not valid.
    #[error("not valid: {0}")]
    Invalid(String),
}

impl From<der::Error> for Error {
    fn from(e: der::Error) -> Self {
        Error::Malformed(e.to_string())
    }
}

impl From<graviola::Error> for Error {
    fn from(e: graviola::Error) -> Self {
        Error::Crypto(format!("{e:?}"))
    }
}

/// Short alias.
pub type Result<T> = std::result::Result<T, Error>;
