// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client transport: [`RpcClient`] over TCP (connected on first
//! use and again after a failure; replies matched by XID, others
//! dropped) or UDP (retransmitted with the same XID, the timeout
//! doubling up to a ceiling). Blocking. A caller's interrupt flag ends
//! a wait with [`RpcError::Interrupted`] (the NFS client's `intr`).
//!
//! [`RpcClient::attempt`] is one send and one wait: a retry policy
//! (`soft`, `hard`) is the caller's, built on it and on
//! [`RpcError::is_retryable`]. [`RpcClient::call_raw`] does what one
//! call does without a policy: on UDP the retransmissions of
//! [`ClientConfig::retransmits`]; on TCP one attempt, sent again once
//! on a new connection if a kept connection had broken.

mod error;
mod port;

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub use error::RpcError;

use super::auth::{Credential, none};
use super::msg::{self, RPC_VERSION, ReplyError};
use super::record::{MAX_RECORD, RecordBuf, TooLarge, frame};
use crate::types::rpc::call_body;
use crate::xdr::{self, Decoder, Encoder, ProcBody, Xdr};

/// How often a wait looks at the interrupt flag.
const SLICE: Duration = Duration::from_millis(100);

/// TCP or UDP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Proto {
    /// TCP.
    Tcp,
    /// UDP.
    Udp,
}

/// How a client calls.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// The wait for one attempt (UDP: the first; it doubles).
    pub timeout: Duration,
    /// UDP: retransmissions after the first send.
    pub retransmits: u32,
    /// UDP: the ceiling of the doubling timeout.
    pub max_timeout: Duration,
    /// Send from a reserved port when one can be had.
    pub reserved_port: bool,
    /// The largest reply record over TCP.
    pub max_record: usize,
    /// Set by the caller to end a wait.
    pub interrupt: Option<Arc<AtomicBool>>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(5),
            retransmits: 3,
            max_timeout: Duration::from_secs(60),
            reserved_port: true,
            max_record: MAX_RECORD,
            interrupt: None,
        }
    }
}

enum Conn {
    Tcp(TcpStream, RecordBuf),
    Udp(UdpSocket),
}

/// An RPC client of one server address.
pub struct RpcClient {
    addr: SocketAddr,
    proto: Proto,
    cfg: ClientConfig,
    conn: Option<Conn>,
    xid: u32,
}

impl std::fmt::Debug for RpcClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RpcClient")
            .field("addr", &self.addr)
            .field("proto", &self.proto)
            .field("connected", &self.conn.is_some())
            .finish_non_exhaustive()
    }
}

fn first_xid() -> u32 {
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    t.subsec_nanos()
        ^ (u32::try_from(t.as_secs() & 0xffff_ffff).unwrap_or(0) << 12)
        ^ (std::process::id() << 20)
}

impl RpcClient {
    /// A client of `addr` (nothing is opened until the first call).
    pub fn new(addr: SocketAddr, proto: Proto, cfg: ClientConfig) -> Self {
        Self {
            addr,
            proto,
            cfg,
            conn: None,
            xid: first_xid(),
        }
    }

    /// The server's address.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// TCP or UDP.
    pub fn proto(&self) -> Proto {
        self.proto
    }

    /// The configuration, to change.
    pub fn config_mut(&mut self) -> &mut ClientConfig {
        &mut self.cfg
    }

    /// The local address, once connected.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        match self.conn.as_ref()? {
            Conn::Tcp(s, _) => s.local_addr().ok(),
            Conn::Udp(u) => u.local_addr().ok(),
        }
    }

    /// Opens the connection (or socket) now.
    pub fn connect(&mut self) -> Result<(), RpcError> {
        if self.conn.is_none() {
            self.conn = Some(match self.proto {
                Proto::Tcp => Conn::Tcp(
                    port::tcp(self.addr, self.cfg.reserved_port, self.cfg.timeout)
                        .map_err(RpcError::Connect)?,
                    RecordBuf::new(self.cfg.max_record),
                ),
                Proto::Udp => Conn::Udp(
                    port::udp(self.addr, self.cfg.reserved_port).map_err(RpcError::Connect)?,
                ),
            });
        }
        Ok(())
    }

    /// Closes the connection; the next call opens a new one.
    pub fn disconnect(&mut self) {
        self.conn = None;
    }

    /// A transaction ID not used before by this client.
    pub fn next_xid(&mut self) -> u32 {
        self.xid = self.xid.wrapping_add(1);
        self.xid
    }

    fn interrupted(&self) -> bool {
        self.cfg
            .interrupt
            .as_ref()
            .is_some_and(|f| f.load(Ordering::Relaxed))
    }

    /// One send of the call message `msg` and one wait of `timeout` for
    /// the reply with `xid`: the reply message.
    pub fn attempt(
        &mut self,
        xid: u32,
        msg: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>, RpcError> {
        self.connect()?;
        let r = match self.proto {
            Proto::Tcp => self.attempt_tcp(xid, msg, timeout),
            Proto::Udp => self.attempt_udp(xid, msg, timeout),
        };
        if matches!(r, Err(RpcError::ConnectionLost(_) | RpcError::TooLarge(_))) {
            self.disconnect();
        }
        r
    }

    fn attempt_tcp(
        &mut self,
        xid: u32,
        msg: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>, RpcError> {
        let deadline = Instant::now() + timeout;
        let interrupted = self.interrupted();
        let Some(Conn::Tcp(s, buf)) = self.conn.as_mut() else {
            return Err(RpcError::ConnectionLost(io::ErrorKind::NotConnected.into()));
        };
        s.set_write_timeout(Some(timeout.max(Duration::from_millis(1))))
            .map_err(RpcError::Io)?;
        s.write_all(&frame(msg)).map_err(RpcError::ConnectionLost)?;
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            while let Some(rec) = buf
                .next_record()
                .map_err(|TooLarge(n)| RpcError::TooLarge(n))?
            {
                if msg::xid_of(&rec) == Some(xid) {
                    return Ok(rec);
                }
            }
            if interrupted
                || self
                    .cfg
                    .interrupt
                    .as_ref()
                    .is_some_and(|f| f.load(Ordering::Relaxed))
            {
                return Err(RpcError::Interrupted);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(RpcError::Timeout);
            }
            s.set_read_timeout(Some(left.min(SLICE)))
                .map_err(RpcError::Io)?;
            match s.read(&mut chunk) {
                Ok(0) => {
                    return Err(RpcError::ConnectionLost(
                        io::ErrorKind::UnexpectedEof.into(),
                    ));
                }
                Ok(n) => buf.push(chunk.get(..n).unwrap_or_default()),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) => {}
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(RpcError::ConnectionLost(e)),
            }
        }
    }

    fn attempt_udp(
        &mut self,
        xid: u32,
        msg: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>, RpcError> {
        let deadline = Instant::now() + timeout;
        let flag = self.cfg.interrupt.clone();
        let Some(Conn::Udp(u)) = self.conn.as_ref() else {
            return Err(RpcError::ConnectionLost(io::ErrorKind::NotConnected.into()));
        };
        // A refusal from an earlier datagram is no answer: keep waiting.
        let soft = |e: &io::Error| {
            matches!(
                e.kind(),
                io::ErrorKind::ConnectionRefused
                    | io::ErrorKind::ConnectionReset
                    | io::ErrorKind::WouldBlock
                    | io::ErrorKind::TimedOut
                    | io::ErrorKind::Interrupted
            )
        };
        match u.send(msg) {
            Ok(_) => {}
            Err(e) if soft(&e) => {}
            Err(e) => return Err(RpcError::Io(e)),
        }
        let mut buf = vec![0u8; 65_536];
        loop {
            if flag.as_ref().is_some_and(|f| f.load(Ordering::Relaxed)) {
                return Err(RpcError::Interrupted);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(RpcError::Timeout);
            }
            u.set_read_timeout(Some(left.min(SLICE)))
                .map_err(RpcError::Io)?;
            match u.recv(&mut buf) {
                Ok(n) => {
                    let m = buf.get(..n).unwrap_or_default();
                    if msg::xid_of(m) == Some(xid) {
                        return Ok(m.to_vec());
                    }
                }
                Err(e) if soft(&e) => {}
                Err(e) => return Err(RpcError::Io(e)),
            }
        }
    }

    /// The call message of a procedure, with its XID.
    pub fn message(
        &mut self,
        prog: u32,
        vers: u32,
        procedure: u32,
        cred: &Credential,
        args: &[u8],
    ) -> Result<(u32, Vec<u8>), RpcError> {
        let xid = self.next_xid();
        let body = call_body {
            rpcvers: RPC_VERSION,
            prog,
            vers,
            proc: procedure,
            cred: cred.to_opaque().map_err(RpcError::Args)?,
            verf: none(),
        };
        let m = msg::encode_call(xid, &body, args).map_err(RpcError::Args)?;
        Ok((xid, m))
    }

    /// Calls a procedure with its arguments encoded: its results encoded.
    pub fn call_raw(
        &mut self,
        prog: u32,
        vers: u32,
        procedure: u32,
        cred: &Credential,
        args: &[u8],
    ) -> Result<Vec<u8>, RpcError> {
        let (xid, m) = self.message(prog, vers, procedure, cred, args)?;
        let reply = match self.proto {
            Proto::Udp => {
                let mut t = self.cfg.timeout;
                let mut tries = 0;
                loop {
                    match self.attempt(xid, &m, t) {
                        Err(RpcError::Timeout) if tries < self.cfg.retransmits => {
                            tries += 1;
                            t = (t * 2).min(self.cfg.max_timeout);
                        }
                        r => break r,
                    }
                }
            }
            Proto::Tcp => {
                let kept = self.conn.is_some();
                match self.attempt(xid, &m, self.cfg.timeout) {
                    Err(RpcError::ConnectionLost(_)) if kept => {
                        self.attempt(xid, &m, self.cfg.timeout)
                    }
                    r => r,
                }
            }
        }?;
        results(&reply)
    }

    /// Calls a procedure: `A` encoded, `R` decoded.
    pub fn call<A: Xdr, R: Xdr>(
        &mut self,
        prog: u32,
        vers: u32,
        procedure: u32,
        cred: &Credential,
        args: &A,
    ) -> Result<R, RpcError> {
        let a = xdr::to_bytes(args).map_err(RpcError::Args)?;
        let r = self.call_raw(prog, vers, procedure, cred, &a)?;
        xdr::from_bytes(&r).map_err(RpcError::BadReply)
    }

    /// Calls the procedure `args` belongs to, from a version's generated
    /// `Args`, decoding the version's `Res`.
    pub fn call_proc<A: ProcBody, R: ProcBody>(
        &mut self,
        cred: &Credential,
        args: &A,
    ) -> Result<R, RpcError> {
        let mut enc = Encoder::new();
        args.encode_body(&mut enc).map_err(RpcError::Args)?;
        let procedure = args.procedure();
        let results = self.call_raw(A::PROGRAM, A::VERSION, procedure, cred, enc.as_bytes())?;
        let mut dec = Decoder::new(&results);
        let v = R::decode_body(procedure, &mut dec)
            .map_err(RpcError::BadReply)?
            .ok_or(RpcError::BadReply(xdr::Error::Discriminant {
                ty: "procedure",
                value: i64::from(procedure),
            }))?;
        dec.finish().map_err(RpcError::BadReply)?;
        Ok(v)
    }
}

/// The results of a reply message, or why there are none.
pub fn results(reply: &[u8]) -> Result<Vec<u8>, RpcError> {
    let r = msg::parse_reply(reply).map_err(RpcError::BadReply)?;
    match r.outcome {
        Ok(mut d) => Ok(d.rest().to_vec()),
        Err(ReplyError::Refused(a)) => Err(RpcError::Refused(a)),
        Err(ReplyError::Denied(d)) => Err(RpcError::Denied(d)),
    }
}
