// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Error type.

use std::io;

use thiserror::Error;

/// Anything the certificate store can fail with.
#[derive(Debug, Error)]
pub enum CertError {
    /// The file layer failed.
    #[error("certificate store I/O: {0}")]
    Io(#[from] io::Error),
    /// A store, stash or definitions file does not exist.
    #[error("{0} does not exist")]
    NotFound(String),
    /// A store already exists where one was to be created.
    #[error("{0} already exists")]
    Exists(String),
    /// The password does not open the store, or a PKCS#12 file.
    #[error("the password is not correct")]
    BadPassword,
    /// The file is not a store in our format, or is damaged.
    #[error("the store is damaged or not a Quartz store: {0}")]
    Corrupt(String),
    /// A label is already in use in the store.
    #[error("label {0} is already in use")]
    LabelExists(String),
    /// No entry has this label.
    #[error("no certificate has label {0}")]
    LabelNotFound(String),
    /// The entry has no private key.
    #[error("certificate {0} has no private key")]
    NoPrivateKey(String),
    /// A certificate, request or key failed to parse.
    #[error("malformed data: {0}")]
    Malformed(String),
    /// A key or signature operation failed.
    #[error("cryptographic operation failed: {0}")]
    Crypto(String),
    /// The input uses something this store does not support.
    #[error("not supported: {0}")]
    Unsupported(String),
    /// A request was not valid.
    #[error("not valid: {0}")]
    Invalid(String),
    /// The application ID is not registered.
    #[error("application ID {0} is not registered")]
    NotRegistered(String),
    /// The application ID is already registered.
    #[error("application ID {0} is already registered")]
    AlreadyRegistered(String),
    /// A sealed secret did not unseal.
    #[error("the sealed password does not unseal with this system's key")]
    Unseal,
}

impl From<der::Error> for CertError {
    fn from(e: der::Error) -> Self {
        CertError::Malformed(e.to_string())
    }
}

impl From<graviola::Error> for CertError {
    fn from(e: graviola::Error) -> Self {
        CertError::Crypto(format!("{e:?}"))
    }
}

/// Short alias.
pub type Result<T> = std::result::Result<T, CertError>;
