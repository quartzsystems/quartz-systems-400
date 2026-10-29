// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The blocking adapter: TLS over any `Read + Write` (a socket later).

use std::io::{self, Read, Write};
use std::sync::Arc;

use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ServerConfig};

use crate::engine::{Engine, Session};
use crate::error::Result;

/// A TLS stream over `T`.
pub struct TlsStream<T: Read + Write> {
    engine: Engine,
    io: T,
}

impl<T: Read + Write> TlsStream<T> {
    /// Accept on `io` as a server and complete the handshake.
    pub fn accept(config: Arc<ServerConfig>, io: T) -> Result<Self> {
        let mut s = Self {
            engine: Engine::server(config)?,
            io,
        };
        s.engine.complete_io(&mut s.io)?;
        Ok(s)
    }

    /// Connect over `io` as a client to `name` and complete the handshake.
    pub fn connect(config: Arc<ClientConfig>, name: ServerName<'static>, io: T) -> Result<Self> {
        let mut s = Self {
            engine: Engine::client(config, name)?,
            io,
        };
        s.engine.complete_io(&mut s.io)?;
        Ok(s)
    }

    /// The session.
    pub fn session(&self) -> Option<Session> {
        self.engine.session()
    }

    /// Send `close_notify` and flush. The transport stays open.
    pub fn close(&mut self) -> Result<()> {
        self.engine.close();
        self.engine.complete_io(&mut self.io)?;
        Ok(())
    }

    /// The transport.
    pub fn get_ref(&self) -> &T {
        &self.io
    }

    /// The transport, mutable.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.io
    }

    /// Take the parts apart.
    pub fn into_parts(self) -> (Engine, T) {
        (self.engine, self.io)
    }
}

macro_rules! with_stream {
    ($self:ident, $buf:ident, $op:ident) => {
        match $self.engine.connection() {
            rustls::Connection::Server(c) => rustls::Stream::new(c, &mut $self.io).$op($buf),
            rustls::Connection::Client(c) => rustls::Stream::new(c, &mut $self.io).$op($buf),
        }
    };
}

impl<T: Read + Write> Read for TlsStream<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        with_stream!(self, buf, read)
    }
}

impl<T: Read + Write> Write for TlsStream<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        with_stream!(self, buf, write)
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.engine.connection() {
            rustls::Connection::Server(c) => rustls::Stream::new(c, &mut self.io).flush(),
            rustls::Connection::Client(c) => rustls::Stream::new(c, &mut self.io).flush(),
        }
    }
}
