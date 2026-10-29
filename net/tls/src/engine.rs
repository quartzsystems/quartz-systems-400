// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The non-blocking engine: a server (or client) feeds it bytes from
//! the wire, takes bytes to send, and reads and writes plaintext. No
//! socket is involved; the caller owns the transport.

use std::io::{self, Read, Write};
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, Connection, ServerConfig, ServerConnection};

use crate::error::{Result, TlsError};
use crate::policy::Protocol;
use crate::suites::{self, IbmSuite};

/// What a finished handshake settled on.
#[derive(Clone, Debug)]
pub struct Session {
    /// The protocol.
    pub protocol: Protocol,
    /// The suite's IANA code.
    pub suite_code: u16,
    /// The suite, when it is one IBM names.
    pub suite: Option<&'static IbmSuite>,
    /// The peer's certificate chain, DER, leaf first (empty when none).
    pub peer_certs: Vec<Vec<u8>>,
    /// The server name the client asked for (servers only).
    pub server_name: Option<String>,
    /// The ALPN protocol agreed, if any.
    pub alpn: Option<Vec<u8>>,
}

/// A TLS connection without a transport.
pub struct Engine {
    conn: Connection,
}

impl Engine {
    /// A server-side engine.
    pub fn server(config: Arc<ServerConfig>) -> Result<Self> {
        Ok(Self {
            conn: Connection::Server(ServerConnection::new(config)?),
        })
    }

    /// A client-side engine for `name`.
    pub fn client(config: Arc<ClientConfig>, name: ServerName<'static>) -> Result<Self> {
        Ok(Self {
            conn: Connection::Client(ClientConnection::new(config, name)?),
        })
    }

    /// Bytes from the wire. Returns how many were taken (the engine may
    /// stop short when its buffer is full); call [`Engine::process`] next.
    pub fn feed(&mut self, bytes: &[u8]) -> Result<usize> {
        let mut cur = io::Cursor::new(bytes);
        Ok(self.conn.read_tls(&mut cur)?)
    }

    /// Process what was fed. An alert from the peer or a bad record is
    /// an error; after it the engine has an alert to send ([`Engine::take_output`]).
    pub fn process(&mut self) -> Result<()> {
        self.conn.process_new_packets()?;
        Ok(())
    }

    /// Bytes to send to the peer, appended to `out`. Returns how many.
    pub fn take_output(&mut self, out: &mut Vec<u8>) -> Result<usize> {
        let mut n = 0;
        while self.conn.wants_write() {
            n += self.conn.write_tls(out)?;
        }
        Ok(n)
    }

    /// Plaintext the peer sent. `Ok(0)` after a clean close; `Closed`
    /// when the peer went away without one; `WouldBlock` when nothing
    /// is available yet.
    pub fn read_plain(&mut self, buf: &mut [u8]) -> Result<usize> {
        match self.conn.reader().read(buf) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => Err(TlsError::Io(e)),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Err(TlsError::Closed),
            Err(e) => Err(TlsError::Io(e)),
        }
    }

    /// Plaintext to send. Buffered until taken.
    pub fn write_plain(&mut self, data: &[u8]) -> Result<usize> {
        Ok(self.conn.writer().write(data)?)
    }

    /// The engine wants more wire bytes.
    pub fn wants_read(&self) -> bool {
        self.conn.wants_read()
    }

    /// The engine has wire bytes to send.
    pub fn wants_write(&self) -> bool {
        self.conn.wants_write()
    }

    /// The handshake is not finished.
    pub fn is_handshaking(&self) -> bool {
        self.conn.is_handshaking()
    }

    /// Queue a `close_notify`; take the output and send it.
    pub fn close(&mut self) {
        self.conn.send_close_notify();
    }

    /// The session, once the handshake is done.
    pub fn session(&self) -> Option<Session> {
        if self.conn.is_handshaking() {
            return None;
        }
        let suite = self.conn.negotiated_cipher_suite()?;
        let suite_code = u16::from(suite.suite());
        let protocol = Protocol::of_rustls(self.conn.protocol_version()?)?;
        let peer_certs = self
            .conn
            .peer_certificates()
            .map(|c| c.iter().map(|d| d.as_ref().to_vec()).collect())
            .unwrap_or_default();
        let server_name = match &self.conn {
            Connection::Server(s) => s.server_name().map(str::to_string),
            Connection::Client(_) => None,
        };
        Some(Session {
            protocol,
            suite_code,
            suite: suites::of_rustls(suite),
            peer_certs,
            server_name,
            alpn: self.conn.alpn_protocol().map(<[u8]>::to_vec),
        })
    }

    /// The rustls connection, for the stream adapter.
    pub(crate) fn connection(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Drive the handshake over a blocking transport.
    pub fn complete_io<T: Read + Write>(&mut self, io: &mut T) -> Result<()> {
        while self.conn.is_handshaking() || self.conn.wants_write() {
            self.conn.complete_io(io).map_err(unwrap_io)?;
            if !self.conn.is_handshaking() && !self.conn.wants_write() {
                break;
            }
        }
        Ok(())
    }
}

/// rustls reports its own errors from `complete_io` inside an
/// `io::Error`; take them back out so callers see the TLS reason.
fn unwrap_io(e: io::Error) -> TlsError {
    match e
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<rustls::Error>())
    {
        Some(r) => TlsError::Rustls(r.clone()),
        None => TlsError::Io(e),
    }
}

/// Two engines talking in memory until neither has anything to say.
/// Returns an error from whichever side failed first.
pub fn pump(a: &mut Engine, b: &mut Engine) -> Result<()> {
    let mut buf = Vec::new();
    for _ in 0..64 {
        let mut moved = false;
        buf.clear();
        a.take_output(&mut buf)?;
        if !buf.is_empty() {
            moved = true;
            let n = b.feed(&buf)?;
            debug_assert_eq!(n, buf.len());
            b.process()?;
        }
        buf.clear();
        b.take_output(&mut buf)?;
        if !buf.is_empty() {
            moved = true;
            let n = a.feed(&buf)?;
            debug_assert_eq!(n, buf.len());
            a.process()?;
        }
        if !moved {
            return Ok(());
        }
    }
    Ok(())
}
