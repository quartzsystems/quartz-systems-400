// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Calls to one server program under the mount's retry policy. A
//! [`Channel`] keeps a pool of connections (each worker thread of the
//! IFS calls at once; replies are matched by XID per connection). One
//! call keeps its XID through every retransmission: an attempt waits
//! `timeo`, doubling to 60 seconds; a refused or lost connection waits
//! out the attempt and connects again. After `retrans`
//! retransmissions (a major timeout) a `soft` call fails `TimedOut`; a
//! `hard` one reports the server not responding (once for the mount)
//! and goes on. `intr`: [`Status::interrupt`] ends the calls waiting.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ifs::Errno;
use ifs::remote::{RResult, RemoteError};

use super::env::Hooks;
use super::settings::Policy;
use crate::rpc::client::results;
use crate::rpc::{AcceptError, ClientConfig, Credential, Denied, Proto, RpcClient, RpcError};
use crate::xdr::{Decoder, Encoder, ProcBody};

/// A mount's view of its server, shared by its channels: whether it
/// answers, the hooks, and the flags of the calls in progress.
pub struct Status {
    hooks: Hooks,
    intr: bool,
    down: AtomicBool,
    stopped: AtomicBool,
    next: AtomicU64,
    calls: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

impl std::fmt::Debug for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Status")
            .field("down", &self.down.load(Ordering::Relaxed))
            .field("intr", &self.intr)
            .finish_non_exhaustive()
    }
}

impl Status {
    /// A server that answers.
    #[must_use]
    pub fn new(hooks: Hooks, intr: bool) -> Arc<Self> {
        Arc::new(Self {
            hooks,
            intr,
            down: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
            next: AtomicU64::new(1),
            calls: Mutex::new(HashMap::new()),
        })
    }

    /// Whether the server is answering.
    pub fn responding(&self) -> bool {
        !self.down.load(Ordering::Relaxed)
    }

    fn not_responding(&self) {
        if !self.down.swap(true, Ordering::Relaxed) {
            tracing::warn!("NFS server not responding, still trying");
            if let Some(h) = &self.hooks.not_responding {
                h();
            }
        }
    }

    fn answered(&self) {
        if self.down.swap(false, Ordering::Relaxed) {
            tracing::info!("NFS server responding again");
            if let Some(h) = &self.hooks.responding {
                h();
            }
        }
    }

    /// `intr`: every call waiting now ends `Interrupted`. Without
    /// `intr` nothing happens. Whether a call was waiting.
    pub fn interrupt(&self) -> bool {
        if !self.intr {
            return false;
        }
        self.wake_all()
    }

    fn wake_all(&self) -> bool {
        let c = self.calls.lock().unwrap_or_else(PoisonError::into_inner);
        for f in c.values() {
            f.store(true, Ordering::Relaxed);
        }
        !c.is_empty()
    }

    /// The mount ends: calls waiting end and new ones are refused.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Relaxed);
        self.wake_all();
    }

    fn enter(&self) -> (u64, Arc<AtomicBool>) {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let f = Arc::new(AtomicBool::new(false));
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, Arc::clone(&f));
        (id, f)
    }

    fn leave(&self, id: u64) {
        self.calls
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&id);
    }
}

/// Connections to one program at one address.
pub struct Channel {
    addr: SocketAddr,
    proto: Proto,
    policy: Policy,
    status: Arc<Status>,
    reserved: bool,
    cap: usize,
    pool: Mutex<(Vec<RpcClient>, usize)>,
    cv: Condvar,
}

impl std::fmt::Debug for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Channel")
            .field("addr", &self.addr)
            .field("proto", &self.proto)
            .finish_non_exhaustive()
    }
}

/// An RPC error as the IFS sees it (the retryable ones are handled
/// before this).
#[must_use]
pub fn rpc_errno(e: &RpcError) -> RemoteError {
    match e {
        RpcError::Interrupted => RemoteError::Interrupted,
        RpcError::Timeout => RemoteError::TimedOut,
        RpcError::Connect(_) => RemoteError::Errno(Errno::ECONNREFUSED),
        RpcError::ConnectionLost(_) => RemoteError::Errno(Errno::ECONNRESET),
        RpcError::Refused(AcceptError::ProgUnavail | AcceptError::ProgMismatch { .. }) => {
            RemoteError::Errno(Errno::EPROTONOSUPPORT)
        }
        RpcError::Refused(AcceptError::ProcUnavail) => RemoteError::Errno(Errno::ENOTSUP),
        RpcError::Denied(Denied::Auth(_)) => RemoteError::Errno(Errno::EACCES),
        RpcError::Args(_) => RemoteError::Errno(Errno::EINVAL),
        _ => RemoteError::Errno(Errno::EIO),
    }
}

fn retryable(e: &RpcError) -> bool {
    e.is_retryable() || matches!(e, RpcError::Io(_))
}

impl Channel {
    /// Calls to `addr` over `proto` under `policy`, at most `cap`
    /// connections at once.
    #[must_use]
    pub fn new(
        addr: SocketAddr,
        proto: Proto,
        policy: Policy,
        status: Arc<Status>,
        reserved: bool,
        cap: usize,
    ) -> Self {
        Self {
            addr,
            proto,
            policy,
            status,
            reserved,
            cap: cap.max(1),
            pool: Mutex::new((Vec::new(), 0)),
            cv: Condvar::new(),
        }
    }

    /// The server's address.
    #[must_use]
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The transport.
    #[must_use]
    pub fn proto(&self) -> Proto {
        self.proto
    }

    /// The same program's channel with another policy (the mount's own
    /// calls are soft).
    #[must_use]
    pub fn with_policy(&self, policy: Policy) -> Self {
        Self::new(
            self.addr,
            self.proto,
            policy,
            Arc::clone(&self.status),
            self.reserved,
            self.cap,
        )
    }

    fn take(&self) -> RpcClient {
        let mut p = self.pool.lock().unwrap_or_else(PoisonError::into_inner);
        loop {
            if let Some(c) = p.0.pop() {
                return c;
            }
            if p.1 < self.cap {
                p.1 += 1;
                let cfg = ClientConfig {
                    timeout: self.policy.timeo,
                    retransmits: 0,
                    max_timeout: super::settings::MAX_WAIT,
                    reserved_port: self.reserved,
                    ..ClientConfig::default()
                };
                return RpcClient::new(self.addr, self.proto, cfg);
            }
            p = self.cv.wait(p).unwrap_or_else(PoisonError::into_inner);
        }
    }

    fn give(&self, c: RpcClient) {
        self.pool
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .0
            .push(c);
        self.cv.notify_one();
    }

    /// Close every idle connection (the next call opens a new one).
    pub fn drop_connections(&self) {
        let mut p = self.pool.lock().unwrap_or_else(PoisonError::into_inner);
        for c in &mut p.0 {
            c.disconnect();
        }
    }

    /// Calls `procedure` of `prog` `vers` with encoded arguments: the
    /// encoded results.
    ///
    /// # Errors
    ///
    /// `TimedOut` (soft), `Interrupted` (intr), or what the transport
    /// could not get past.
    pub fn call_raw(
        &self,
        prog: u32,
        vers: u32,
        procedure: u32,
        cred: &Credential,
        args: &[u8],
    ) -> RResult<Vec<u8>> {
        if self.status.stopped.load(Ordering::Relaxed) {
            return Err(RemoteError::Interrupted);
        }
        let (id, flag) = self.status.enter();
        let mut c = self.take();
        c.config_mut().interrupt = Some(Arc::clone(&flag));
        let r = self.retry(&mut c, &flag, prog, vers, procedure, cred, args);
        c.config_mut().interrupt = None;
        self.give(c);
        self.status.leave(id);
        r
    }

    #[allow(clippy::too_many_arguments)]
    fn retry(
        &self,
        c: &mut RpcClient,
        flag: &AtomicBool,
        prog: u32,
        vers: u32,
        procedure: u32,
        cred: &Credential,
        args: &[u8],
    ) -> RResult<Vec<u8>> {
        let (xid, msg) = c
            .message(prog, vers, procedure, cred, args)
            .map_err(|e| rpc_errno(&e))?;
        let mut n = 0u32;
        loop {
            let wait = self.policy.wait(n);
            let start = Instant::now();
            match c.attempt(xid, &msg, wait) {
                Ok(reply) => {
                    self.status.answered();
                    return results(&reply).map_err(|e| rpc_errno(&e));
                }
                Err(RpcError::Interrupted) => return Err(RemoteError::Interrupted),
                Err(e) if retryable(&e) => {
                    if !matches!(e, RpcError::Timeout) {
                        // Refused or lost at once: the attempt still
                        // takes its time before the next.
                        c.disconnect();
                        if pause(start + wait, flag) {
                            return Err(RemoteError::Interrupted);
                        }
                    }
                    n += 1;
                    if n > self.policy.retrans {
                        if self.policy.soft {
                            return Err(RemoteError::TimedOut);
                        }
                        self.status.not_responding();
                    }
                }
                Err(e) => return Err(rpc_errno(&e)),
            }
        }
    }

    /// Calls the procedure `args` belongs to, decoding the version's
    /// results.
    ///
    /// # Errors
    ///
    /// As [`Self::call_raw`]; results that do not decode are `EIO`.
    pub fn call<A: ProcBody, R: ProcBody>(&self, cred: &Credential, args: &A) -> RResult<R> {
        let mut enc = Encoder::new();
        args.encode_body(&mut enc)
            .map_err(|_| RemoteError::Errno(Errno::EINVAL))?;
        let procedure = args.procedure();
        let res = self.call_raw(A::PROGRAM, A::VERSION, procedure, cred, enc.as_bytes())?;
        decode_res::<R>(procedure, &res)
    }
}

/// A version's results of `procedure`, all bytes used.
///
/// # Errors
///
/// `EIO` when they do not decode.
pub fn decode_res<R: ProcBody>(procedure: u32, res: &[u8]) -> RResult<R> {
    let bad = || RemoteError::Errno(Errno::EIO);
    let mut d = Decoder::new(res);
    let v = R::decode_body(procedure, &mut d)
        .map_err(|_| bad())?
        .ok_or_else(bad)?;
    d.finish().map_err(|_| bad())?;
    Ok(v)
}

/// Wait until `until` unless `flag` is set first (true then).
fn pause(until: Instant, flag: &AtomicBool) -> bool {
    loop {
        if flag.load(Ordering::Relaxed) {
            return true;
        }
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return false;
        }
        std::thread::sleep(left.min(Duration::from_millis(50)));
    }
}
