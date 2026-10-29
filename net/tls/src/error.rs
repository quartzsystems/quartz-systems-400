// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Error type.

use std::io;

use thiserror::Error;

/// Anything the TLS engine can fail with.
#[derive(Debug, Error)]
pub enum TlsError {
    /// The certificate store.
    #[error(transparent)]
    Certs(#[from] certs::CertError),
    /// rustls refused a configuration or a record.
    #[error("TLS: {0}")]
    Rustls(#[from] rustls::Error),
    /// The transport failed.
    #[error("TLS transport: {0}")]
    Io(#[from] io::Error),
    /// The application has no certificate assigned.
    #[error("application {0} has no certificate assigned")]
    NoCertificate(String),
    /// The policy leaves no protocol or cipher suite.
    #[error("the system values leave no usable {0}")]
    NothingEnabled(&'static str),
    /// Client authentication was asked for but the store trusts no CA.
    #[error("client authentication needs at least one trusted certificate authority")]
    NoTrustedCas,
    /// The entry cannot serve as an identity.
    #[error("{0} is not a certificate with a private key")]
    NotAnIdentity(String),
    /// The server name is not valid.
    #[error("server name {0} is not valid")]
    ServerName(String),
    /// The certificate is outside its validity period.
    #[error("certificate {0} is expired or not yet valid")]
    Expired(String),
    /// The peer closed the connection.
    #[error("the connection was closed by the peer")]
    Closed,
}

/// Short alias.
pub type Result<T> = std::result::Result<T, TlsError>;
