// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Why a scenario failed.

use crate::rpc::Refusal;

/// A scenario's failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A socket failed or nothing came back in time.
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    /// Bytes that do not decode as the expected type.
    #[error("decode: {0}")]
    Decode(String),
    /// The server refused the call at the RPC level.
    #[error("rpc: {0}")]
    Refused(Refusal),
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

impl From<onc_xdr::Error> for Error {
    fn from(e: onc_xdr::Error) -> Self {
        Error::Decode(e.to_string())
    }
}

/// A check that failed: `Err(Error::Check(..))`.
pub fn check(ok: bool, what: impl FnOnce() -> String) -> Result<(), Error> {
    if ok {
        Ok(())
    } else {
        Err(Error::Check(what()))
    }
}

/// `Err(Error::Check(..))` with the text.
pub fn fail<T>(what: impl Into<String>) -> Result<T, Error> {
    Err(Error::Check(what.into()))
}
