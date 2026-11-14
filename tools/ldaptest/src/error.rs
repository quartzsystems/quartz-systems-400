// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Why a scenario failed.

/// A scenario's failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The client library failed: a socket, a timeout, a malformed answer.
    #[error("ldap: {0}")]
    Ldap(#[from] ldap3::LdapError),
    /// A socket or file failed.
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    /// Bytes that do not decode as the expected type.
    #[error("decode: {0}")]
    Decode(String),
    /// TLS could not be set up.
    #[error("tls: {0}")]
    Tls(String),
    /// The server answered, but not as the scenario expects.
    #[error("{0}")]
    Check(String),
    /// The scenario cannot run against this target (printed as `skip`).
    #[error("{0}")]
    Skip(String),
    /// The command line was not usable.
    #[error("usage: {0}")]
    Usage(String),
}

/// A check that failed: `Err(Error::Check(..))`.
pub fn check(ok: bool, what: impl FnOnce() -> String) -> Result<(), Error> {
    if ok { Ok(()) } else { Err(Error::Check(what())) }
}

/// A skip: `Err(Error::Skip(..))`.
pub fn skip<T>(why: impl Into<String>) -> Result<T, Error> {
    Err(Error::Skip(why.into()))
}
