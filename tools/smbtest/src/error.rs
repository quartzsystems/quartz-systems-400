// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Why a scenario failed.

/// A scenario's failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The client crate reported an error (a status, a transport or crypto failure).
    #[error("{}", describe(.0))]
    Smb(#[from] smb::Error),
    /// A local I/O error.
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
    /// The server answered, but not as the scenario expects.
    #[error("{0}")]
    Check(String),
    /// Nothing came within the time allowed.
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    /// The client crate cannot do what the scenario needs against this server
    /// (printed as a `skip` line).
    #[error("{0}")]
    Inexpressible(&'static str),
    /// The command line was not usable.
    #[error("usage: {0}")]
    Usage(String),
}

impl Error {
    /// The NT status the server answered with, when the failure is one.
    pub fn status(&self) -> Option<u32> {
        match self {
            Error::Smb(e) => status_of(e),
            _ => None,
        }
    }
}

/// The NT status inside a client crate error.
pub fn status_of(e: &smb::Error) -> Option<u32> {
    match e {
        smb::Error::UnexpectedMessageStatus(s) | smb::Error::ReceivedErrorMessage(s, _) => Some(*s),
        _ => None,
    }
}

fn describe(e: &smb::Error) -> String {
    match status_of(e) {
        Some(s) => format!("status 0x{s:08X} ({e})"),
        None => e.to_string(),
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
