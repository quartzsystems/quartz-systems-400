// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where a writer sends a transformed file: a [`Sink`]. Three kinds
//! over `std::net`: the raw port of a LAN printer ([`raw`], 9100, with
//! a PJL wrapper), an IPP/1.1 printer ([`ipp`], `Print-Job` over HTTP
//! or HTTPS) and an LPD ([`lpr`], RFC 1179, with IBM's `DESTOPT`
//! conventions and the IBM-to-IBM control file). HTTPS goes through a
//! [`TlsConnector`]; [`PlainConnector`] passes the TCP stream through
//! (tests, and until the TLS engine is in).

pub mod ipp;
pub mod lpr;
pub mod raw;

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use thiserror::Error;

use crate::job::PrintJob;
use crate::transform::Output;

/// What went wrong, as the writer reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinkErrorKind {
    /// The host name does not resolve.
    Resolve,
    /// The host refused the connection.
    Refused,
    /// No answer within the time allowed.
    Timeout,
    /// The connection could not be made for another reason.
    Connect,
    /// The connection broke while sending.
    Io,
    /// The far end answered, but not as the protocol says.
    Protocol,
    /// The far end turned the job down.
    Rejected,
    /// TLS could not be set up.
    Tls,
}

impl SinkErrorKind {
    /// The message identifier the writer sends for it. The `TCP3xxx`
    /// identifiers are `QTCPMSGF`'s; which of them IBM's writers send
    /// for each case is unconfirmed (`docs/NET_OPEN_QUESTIONS.md`).
    #[must_use]
    pub fn msgid(self) -> &'static str {
        match self {
            Self::Resolve => "TCP370A",
            Self::Refused => "TCP3427",
            Self::Timeout => "TCP3436",
            Self::Connect | Self::Tls => "TCP3409",
            Self::Io => "TCP3430",
            Self::Protocol | Self::Rejected => "TCP3701",
        }
    }

    /// Whether trying again later may work (the writer waits for a
    /// reply), or the file itself is at fault (it is held).
    #[must_use]
    pub fn retryable(self) -> bool {
        !matches!(self, Self::Rejected)
    }
}

/// A failed send.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
#[error("{}: {detail}", kind.msgid())]
pub struct SinkError {
    /// What kind of failure.
    pub kind: SinkErrorKind,
    /// What happened, for the job log.
    pub detail: String,
}

impl SinkError {
    /// A failure of `kind`.
    pub fn new(kind: SinkErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    /// From an I/O error while connected.
    #[must_use]
    pub fn io(e: &io::Error) -> Self {
        let kind = match e.kind() {
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => SinkErrorKind::Timeout,
            _ => SinkErrorKind::Io,
        };
        Self::new(kind, e.to_string())
    }
}

/// What a sink did with a file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Receipt {
    /// Bytes of print data sent.
    pub bytes: u64,
    /// The far end's job identifier, when it gave one (IPP `job-id`).
    pub job_id: Option<String>,
    /// A line for the job log.
    pub detail: String,
}

/// Somewhere a file can be sent.
pub trait Sink {
    /// Send `doc`, the transformed data of `job`.
    fn send(&mut self, job: &PrintJob, doc: &Output) -> Result<Receipt, SinkError>;
}

/// A connected byte stream: TCP, or TLS over it.
pub trait Stream: Read + Write + Send {
    /// Close the sending half: the far end sees end of file.
    fn finish(&mut self) -> io::Result<()>;
}

impl Stream for TcpStream {
    fn finish(&mut self) -> io::Result<()> {
        self.shutdown(std::net::Shutdown::Write)
    }
}

/// Turns a TCP connection into a TLS one. The TLS engine implements
/// it; the sinks only need the stream it gives back.
pub trait TlsConnector: Send + Sync {
    /// Start TLS on `tcp` to `host`, verifying it as the engine is
    /// configured to.
    fn connect(&self, host: &str, tcp: TcpStream) -> io::Result<Box<dyn Stream>>;
}

/// No TLS at all: the TCP stream as it is. For tests and for printers
/// reached over plain HTTP.
#[derive(Clone, Copy, Debug, Default)]
pub struct PlainConnector;

impl TlsConnector for PlainConnector {
    fn connect(&self, _host: &str, tcp: TcpStream) -> io::Result<Box<dyn Stream>> {
        Ok(Box::new(tcp))
    }
}

/// A host, a port and how long to wait.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// Host name or address.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// How long a connect, read or write may take.
    pub timeout: Duration,
}

/// The time allowed when none is given.
pub const TIMEOUT: Duration = Duration::from_secs(30);

impl Endpoint {
    /// `host:port` with the default timeout.
    #[must_use]
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            host: host.trim().trim_matches(['[', ']']).to_string(),
            port,
            timeout: TIMEOUT,
        }
    }

    fn addrs(&self) -> Result<Vec<SocketAddr>, SinkError> {
        let a: Vec<SocketAddr> = (self.host.as_str(), self.port)
            .to_socket_addrs()
            .map_err(|e| SinkError::new(SinkErrorKind::Resolve, format!("{}: {e}", self.host)))?
            .collect();
        if a.is_empty() {
            return Err(SinkError::new(SinkErrorKind::Resolve, self.host.clone()));
        }
        Ok(a)
    }

    /// Connect, trying every address the name resolves to.
    pub fn connect(&self) -> Result<TcpStream, SinkError> {
        let mut last = SinkError::new(SinkErrorKind::Connect, self.host.clone());
        for a in self.addrs()? {
            match TcpStream::connect_timeout(&a, self.timeout) {
                Ok(s) => {
                    s.set_read_timeout(Some(self.timeout)).map_err(|e| SinkError::io(&e))?;
                    s.set_write_timeout(Some(self.timeout)).map_err(|e| SinkError::io(&e))?;
                    let _ = s.set_nodelay(true);
                    return Ok(s);
                }
                Err(e) => {
                    let kind = match e.kind() {
                        io::ErrorKind::ConnectionRefused => SinkErrorKind::Refused,
                        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => {
                            SinkErrorKind::Timeout
                        }
                        _ => SinkErrorKind::Connect,
                    };
                    last = SinkError::new(kind, format!("{a}: {e}"));
                }
            }
        }
        Err(last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refused_and_unresolved() {
        // A port nobody listens on: bind, learn it, close.
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l);
        let e = Endpoint::new("127.0.0.1", port).connect().unwrap_err();
        assert_eq!(e.kind, SinkErrorKind::Refused);
        assert_eq!(e.kind.msgid(), "TCP3427");
        let e = Endpoint::new("no-such-host.invalid", 9100).connect().unwrap_err();
        assert_eq!(e.kind, SinkErrorKind::Resolve);
        assert!(e.to_string().starts_with("TCP370A: "));
        assert!(!SinkErrorKind::Rejected.retryable());
    }
}
