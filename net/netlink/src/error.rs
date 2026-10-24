// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The crate's error type.

/// `ENOENT`.
pub const ENOENT: i32 = 2;
/// `ENODEV`.
pub const ENODEV: i32 = 19;
/// `EEXIST`.
pub const EEXIST: i32 = 17;
/// `EOPNOTSUPP`.
pub const EOPNOTSUPP: i32 = 95;

/// What can go wrong talking to the kernel.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The socket itself failed.
    #[error("netlink socket: {0}")]
    Io(#[from] std::io::Error),
    /// The kernel answered with an error (positive errno).
    #[error("the kernel refused the request (errno {0})")]
    Kernel(i32),
    /// A reply did not parse.
    #[error("malformed netlink message: {0}")]
    Malformed(&'static str),
    /// A named object does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// A caller-supplied value cannot be encoded.
    #[error("invalid argument: {0}")]
    Invalid(String),
    /// Netlink exists only on Linux.
    #[error("netlink is available only on Linux")]
    Unsupported,
}

impl Error {
    /// The kernel errno, if the kernel refused.
    pub fn errno(&self) -> Option<i32> {
        match self {
            Error::Kernel(e) => Some(*e),
            Error::Io(e) => e.raw_os_error(),
            _ => None,
        }
    }

    /// The object already exists (`EEXIST`).
    pub fn is_exists(&self) -> bool {
        self.errno() == Some(EEXIST)
    }

    /// The object does not exist (`ENOENT`, `ENODEV` or [`Error::NotFound`]).
    pub fn is_not_found(&self) -> bool {
        matches!(self, Error::NotFound(_)) || matches!(self.errno(), Some(ENOENT | ENODEV))
    }

    /// The kernel lacks the feature (`EOPNOTSUPP`).
    pub fn is_not_supported(&self) -> bool {
        matches!(self, Error::Unsupported) || self.errno() == Some(EOPNOTSUPP)
    }
}

/// Result with [`Error`].
pub type Result<T> = std::result::Result<T, Error>;
