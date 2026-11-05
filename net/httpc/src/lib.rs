// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An HTTP/1.1 client: one request per connection, `http://` or
//! `https://` with the shipped roots (or any certificate when the caller
//! tolerates it), bodies by `Content-Length`, chunked or to the close,
//! redirects followed. The `QSYS2.HTTP_*` functions use it.

mod client;
mod url;
mod wire;

pub use client::{Request, Response, send};
pub use url::Url;

/// Why a request failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The URL is not one the client reads.
    #[error("the URL {0} is not valid")]
    Url(String),
    /// The connection failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// TLS failed.
    #[error("TLS: {0}")]
    Tls(#[from] tls::TlsError),
    /// The server's answer is not HTTP.
    #[error("the answer is not HTTP: {0}")]
    Protocol(String),
    /// Too many redirects.
    #[error("more than {0} redirects")]
    Redirects(u32),
}

/// A result.
pub type Result<T> = std::result::Result<T, Error>;
