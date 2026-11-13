// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The server side: a [`Program`] answers its procedures; the
//! [`Dispatcher`] holds the programs, checks the header and the
//! credential, answers what no program can (`PROG_UNAVAIL`,
//! `PROG_MISMATCH`, `RPC_MISMATCH`, `AUTH_ERROR`) and consults the
//! duplicate request cache. [`serve_stream`] runs one TCP connection,
//! one call at a time, its replies in order; [`serve_datagram`] one UDP
//! datagram. Both are bytes in and bytes out underneath
//! ([`Dispatcher::handle`]), so tests and fuzzers drive them in memory.

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::sync::Arc;
use std::time::Instant;

use super::auth::Credential;
use super::drc::{DuplicateCache, Key, Lookup};
use super::msg::{self, AcceptError, CallError};
use super::record::{frame, read_record};
use crate::types::rpc::auth_stat;
use crate::xdr::{Decoder, Encoder, ProcBody};

/// How a call arrived.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    /// TCP.
    Tcp,
    /// UDP.
    Udp,
}

/// What a procedure knows of its call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallContext {
    /// The caller's address and port.
    pub peer: SocketAddr,
    /// TCP or UDP.
    pub transport: Transport,
    /// The caller's credential.
    pub cred: Credential,
    /// The transaction ID.
    pub xid: u32,
    /// The program.
    pub prog: u32,
    /// The version (one the program has).
    pub vers: u32,
    /// The procedure.
    pub procedure: u32,
}

/// What a procedure answers: the results encoded, or no reply at all.
pub type Answer = Result<Option<Vec<u8>>, AcceptError>;

/// An RPC program.
pub trait Program: Send + Sync {
    /// The program number.
    fn program(&self) -> u32;
    /// The versions it has, lowest first.
    fn versions(&self) -> &[u32];
    /// Runs a procedure of a version it has. `Ok(None)` sends no reply.
    fn call(&self, ctx: &CallContext, args: &mut Decoder<'_>) -> Answer;
}

/// A program one thread serves: [`Program`] without `Send` and `Sync`,
/// for state that cannot cross threads (a daemon thread's own link to
/// the file system). A connection's thread hands its own to
/// [`Dispatcher::handle_with`].
pub trait LocalProgram {
    /// The program number.
    fn program(&self) -> u32;
    /// The versions it has, lowest first.
    fn versions(&self) -> &[u32];
    /// Runs a procedure of a version it has. `Ok(None)` sends no reply.
    fn call(&self, ctx: &CallContext, args: &mut Decoder<'_>) -> Answer;
}

/// The program a call is for: the thread's or the dispatcher's.
enum Found<'a> {
    Local(&'a dyn LocalProgram),
    Shared(&'a dyn Program),
}

impl Found<'_> {
    fn versions(&self) -> &[u32] {
        match self {
            Self::Local(p) => p.versions(),
            Self::Shared(p) => p.versions(),
        }
    }
    fn call(&self, ctx: &CallContext, args: &mut Decoder<'_>) -> Answer {
        match self {
            Self::Local(p) => p.call(ctx, args),
            Self::Shared(p) => p.call(ctx, args),
        }
    }
}

/// The arguments of `ctx`'s procedure from a version's generated
/// `Args`: `PROC_UNAVAIL` for a procedure it lacks, `GARBAGE_ARGS` when
/// they do not decode or bytes are left over.
pub fn decode_args<A: ProcBody>(ctx: &CallContext, d: &mut Decoder<'_>) -> Result<A, AcceptError> {
    match A::decode_body(ctx.procedure, d) {
        Ok(Some(a)) if d.is_empty() => Ok(a),
        Ok(None) => Err(AcceptError::ProcUnavail),
        Ok(Some(_)) | Err(_) => Err(AcceptError::GarbageArgs),
    }
}

/// A version's generated `Res` as an answer (`SYSTEM_ERR` if it breaks a bound).
pub fn encode_res<R: ProcBody>(r: &R) -> Answer {
    let mut e = Encoder::new();
    r.encode_body(&mut e).map_err(|_| AcceptError::SystemErr)?;
    Ok(Some(e.into_bytes()))
}

/// The programs of one server, and its duplicate request cache.
#[derive(Default, Clone)]
pub struct Dispatcher {
    programs: Vec<Arc<dyn Program>>,
    cache: Option<Arc<DuplicateCache>>,
}

impl std::fmt::Debug for Dispatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let p: Vec<u32> = self.programs.iter().map(|p| p.program()).collect();
        f.debug_struct("Dispatcher")
            .field("programs", &p)
            .finish_non_exhaustive()
    }
}

impl Dispatcher {
    /// A dispatcher without programs.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a program.
    pub fn add(&mut self, p: Arc<dyn Program>) {
        self.programs.push(p);
    }

    /// Uses `cache` for the procedures it keeps.
    pub fn set_cache(&mut self, cache: Arc<DuplicateCache>) {
        self.cache = Some(cache);
    }

    /// The reply to the message `msg` from `peer`, or `None` for no reply.
    pub fn handle(&self, msg: &[u8], peer: SocketAddr, transport: Transport) -> Option<Vec<u8>> {
        self.handle_with(msg, peer, transport, &[])
    }

    /// [`Self::handle`] with the calling thread's own programs too,
    /// looked at before the dispatcher's.
    pub fn handle_with(
        &self,
        msg: &[u8],
        peer: SocketAddr,
        transport: Transport,
        local: &[&dyn LocalProgram],
    ) -> Option<Vec<u8>> {
        let call = match msg::parse_call(msg) {
            Ok(c) => c,
            Err(CallError::Drop) => return None,
            Err(CallError::RpcVersion { xid }) => return Some(msg::reply_rpc_mismatch(xid)),
            Err(CallError::BadAuth { xid, stat }) => return Some(msg::reply_auth_error(xid, stat)),
        };
        let xid = call.xid;
        let cred = match Credential::from_opaque(&call.body.cred) {
            Ok(c) => c,
            Err(stat) => return Some(msg::reply_auth_error(xid, stat)),
        };
        // AUTH_NONE and AUTH_SYS callers send an AUTH_NONE verifier.
        if call.body.verf.flavor != crate::types::rpc::AUTH_NONE {
            return Some(msg::reply_auth_error(xid, auth_stat::AUTH_BADVERF));
        }
        let b = &call.body;
        let found = match local.iter().find(|p| p.program() == b.prog) {
            Some(l) => Some(Found::Local(*l)),
            None => self
                .programs
                .iter()
                .find(|p| p.program() == b.prog)
                .map(|p| Found::Shared(&**p)),
        };
        let Some(p) = found else {
            return Some(msg::reply_refused(xid, AcceptError::ProgUnavail));
        };
        let vs = p.versions();
        if !vs.contains(&b.vers) {
            let low = vs.iter().copied().min().unwrap_or(0);
            let high = vs.iter().copied().max().unwrap_or(0);
            return Some(msg::reply_refused(
                xid,
                AcceptError::ProgMismatch { low, high },
            ));
        }
        let ctx = CallContext {
            peer,
            transport,
            cred,
            xid,
            prog: b.prog,
            vers: b.vers,
            procedure: b.proc,
        };
        let key = Key {
            peer,
            xid,
            prog: b.prog,
            vers: b.vers,
            procedure: b.proc,
        };
        let cache = self.cache.as_ref();
        match cache.map_or(Lookup::NotCached, |c| c.begin(key, Instant::now())) {
            Lookup::Done(r) => return Some(r.as_ref().clone()),
            Lookup::InProgress => return None,
            Lookup::New | Lookup::NotCached => {}
        }
        let mut args = call.args;
        let out = match p.call(&ctx, &mut args) {
            Ok(Some(results)) => Some(msg::reply_success(xid, &results)),
            Ok(None) => None,
            Err(why) => Some(msg::reply_refused(xid, why)),
        };
        if let Some(c) = cache
            && c.keeps(key.prog, key.vers, key.procedure)
        {
            c.finish(key, out.as_deref());
        }
        out
    }
}

/// Serves one connection's records from `r`, replying on `w`, until
/// `r` ends. A record over `max` ends it with an error.
pub fn serve_stream<R: Read, W: Write>(
    r: &mut R,
    w: &mut W,
    d: &Dispatcher,
    peer: SocketAddr,
    max: usize,
) -> io::Result<()> {
    while let Some(m) = read_record(r, max)? {
        if let Some(reply) = d.handle(&m, peer, Transport::Tcp) {
            w.write_all(&frame(&reply))?;
            w.flush()?;
        }
    }
    Ok(())
}

/// Serves one accepted TCP connection until the peer closes it.
pub fn serve_tcp(stream: &TcpStream, d: &Dispatcher, max: usize) -> io::Result<()> {
    let peer = stream.peer_addr()?;
    serve_stream(&mut &*stream, &mut &*stream, d, peer, max)
}

/// Receives one datagram on `sock` and answers it.
pub fn serve_datagram(sock: &UdpSocket, d: &Dispatcher) -> io::Result<()> {
    let mut buf = vec![0u8; 65_536];
    let (n, peer) = sock.recv_from(&mut buf)?;
    let m = buf.get(..n).unwrap_or_default();
    if let Some(reply) = d.handle(m, peer, Transport::Udp) {
        sock.send_to(&reply, peer)?;
    }
    Ok(())
}
