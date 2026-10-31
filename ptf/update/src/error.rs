// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Update service errors.

use crate::msg;

/// What went wrong talking to the update service.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The connection.
    #[error("update service: {0}")]
    Io(#[from] std::io::Error),
    /// TLS.
    #[error("update service TLS: {0}")]
    Tls(#[from] tls::TlsError),
    /// An answer other than 200.
    #[error("update service answered {status}: {text}")]
    Status {
        /// HTTP status.
        status: u16,
        /// The answer's text.
        text: String,
    },
    /// A message that does not parse.
    #[error("update service protocol: {0}")]
    Protocol(String),
    /// A signature or digest that does not verify.
    #[error("not verified: {0}")]
    Verify(String),
    /// A URL that is not one.
    #[error("not a service URL: {0}")]
    Url(String),
    /// A PTF stream.
    #[error(transparent)]
    Ptf(#[from] ptf::Error),
}

impl Error {
    /// The message identifier a command sends for it.
    #[must_use]
    pub fn msgid(&self) -> &'static str {
        match self {
            Self::Verify(_) => msg::VERIFY,
            Self::Url(_) => "CPD0084",
            _ => msg::CONNECT,
        }
    }
}

/// Update service result.
pub type Result<T> = std::result::Result<T, Error>;
