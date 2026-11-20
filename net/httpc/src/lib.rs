// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An HTTP/1.1 client: one request per connection, `http://` or
//! `https://`, bodies by `Content-Length`, chunked or to the close,
//! redirects followed. What a TLS connection trusts is the caller's
//! choice ([`Trust`]): roots it gives, one certificate pinned by its
//! SHA-256, or a configuration of its own. A body is read whole
//! ([`send`]) or streamed to a writer under a size limit and resumed
//! by range ([`download`], [`download_file`]).

mod client;
mod download;
mod trust;
mod url;
mod wire;

pub use client::{Request, Response, send};
pub use download::{Fetched, download, download_file};
pub use rustls;
pub use trust::Trust;
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
    /// TLS failed: the configuration, the handshake or the server's
    /// certificate.
    #[error("TLS: {0}")]
    Tls(String),
    /// The server's answer is not HTTP.
    #[error("the answer is not HTTP: {0}")]
    Protocol(String),
    /// Too many redirects.
    #[error("more than {0} redirects")]
    Redirects(u32),
    /// A download answered with a status other than 200 or 206.
    #[error("the server answered {0}")]
    Status(u16),
    /// A download is longer than its limit.
    #[error("the body is longer than the {0} bytes allowed")]
    TooLarge(u64),
    /// A resumed download was answered from the start: the server
    /// ignored the range. Nothing was written.
    #[error("the server ignored the range; the download must start again")]
    RangeIgnored,
}

/// A result.
pub type Result<T> = std::result::Result<T, Error>;
