// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connection under a client: a socket, in the clear or under TLS.

use std::io::{self, Read, Write};
use std::net::TcpStream;

use tls::stream::TlsStream;

/// A client's connection.
pub enum Stream {
    /// In the clear.
    Plain(TcpStream),
    /// Under TLS (LDAPS or after `StartTLS`).
    Tls(Box<TlsStream<TcpStream>>),
}

impl Stream {
    /// The socket, for its timeouts.
    pub fn tcp(&self) -> &TcpStream {
        match self {
            Self::Plain(s) => s,
            Self::Tls(t) => t.get_ref(),
        }
    }
}

impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(buf),
            Self::Tls(t) => t.read(buf),
        }
    }
}

impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(s) => s.write(buf),
            Self::Tls(t) => t.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Tls(t) => t.flush(),
        }
    }
}
