// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The connections a session drives: replies on the control
//! connection, data connections (passive or active, TLS with `PROT
//! P`), TLS on the control connection (`AUTH TLS`, `CCC`) and `ABOR`
//! while a transfer runs. [`tcp`] is the real one over sockets; [`mem`]
//! keeps everything in memory for tests and the transcript replay.

pub mod mem;
pub mod tcp;

use std::io::{self, Read, Write};
use std::net::SocketAddr;

use crate::reply::Reply;

/// A data connection.
pub trait DataConn: Read + Write {
    /// Close it (TLS's `close_notify` first).
    fn finish(&mut self) -> io::Result<()>;
}

/// The session's connections.
pub trait Net {
    /// Send a reply on the control connection.
    fn send(&mut self, r: &Reply) -> io::Result<()>;
    /// Listen for the next data connection: the address to tell the
    /// client (`PASV`, or `EPSV` with `extended`).
    fn passive(&mut self, extended: bool) -> io::Result<SocketAddr>;
    /// Connect the next data connection to `to` (`PORT`, `EPRT`).
    fn active(&mut self, to: SocketAddr) -> io::Result<()>;
    /// Open the data connection the last `PASV` or `PORT` set up.
    fn data(&mut self) -> io::Result<Box<dyn DataConn>>;
    /// Whether `AUTH TLS` can be done.
    fn tls_available(&self) -> bool;
    /// Do the TLS handshake on the control connection.
    fn start_tls(&mut self) -> io::Result<()>;
    /// `CCC`: back to clear text on the control connection.
    fn end_tls(&mut self) -> io::Result<()>;
    /// Whether the control connection is TLS.
    fn secure(&self) -> bool;
    /// `PROT P` (true) or `PROT C`.
    fn set_private(&mut self, private: bool);
    /// Whether the client sent `ABOR` while a transfer runs.
    fn aborted(&mut self) -> bool;
    /// The client's address.
    fn peer(&self) -> SocketAddr;
    /// The server's address on the control connection.
    fn local(&self) -> SocketAddr;
}
