// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Calls from the server to its client on the client's own TCP
//! connection (the `NFSv4.1` backchannel, RFC 8881 section 2.10.3.1).
//! The connection's thread writes its replies through [`BackChannel`]
//! too, so records never interleave; the replies to the server's calls
//! arrive among the client's calls and are handed back by
//! [`BackChannel::deliver`]. A call is made from another thread than
//! the connection's (that one is busy reading).

use std::collections::HashMap;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use super::auth::{Credential, none};
use super::client::{RpcError, results};
use super::msg::encode_call;
use super::record::frame;
use crate::types::rpc::call_body;

/// One connection's way back to its client.
pub struct BackChannel {
    out: Mutex<Box<dyn Write + Send>>,
    pending: Mutex<HashMap<u32, mpsc::Sender<Vec<u8>>>>,
    xid: AtomicU32,
    open: AtomicBool,
    peer: SocketAddr,
}

impl std::fmt::Debug for BackChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackChannel")
            .field("peer", &self.peer)
            .field("open", &self.is_open())
            .finish_non_exhaustive()
    }
}

/// Whether `msg` is a reply (its message type is 1).
#[must_use]
pub fn is_reply(msg: &[u8]) -> bool {
    msg.get(4..8) == Some(&[0, 0, 0, 1])
}

impl BackChannel {
    /// The channel over `out`, the connection to `peer`.
    pub fn new(out: Box<dyn Write + Send>, peer: SocketAddr) -> Self {
        Self {
            out: Mutex::new(out),
            pending: Mutex::new(HashMap::new()),
            xid: AtomicU32::new(0x5153_0000 ^ u32::from(peer.port())),
            open: AtomicBool::new(true),
            peer,
        }
    }

    /// The client's address.
    pub fn peer(&self) -> SocketAddr {
        self.peer
    }

    /// Whether the connection is still up.
    pub fn is_open(&self) -> bool {
        self.open.load(Ordering::Relaxed)
    }

    /// Writes one record.
    ///
    /// # Errors
    ///
    /// The socket's; the channel is then closed.
    pub fn send_record(&self, msg: &[u8]) -> io::Result<()> {
        let mut w = self.out.lock().unwrap_or_else(PoisonError::into_inner);
        let r = w.write_all(&frame(msg)).and_then(|()| w.flush());
        if r.is_err() {
            self.close();
        }
        r
    }

    /// Hands a reply that arrived on the connection to its call: whether
    /// one waited for it.
    pub fn deliver(&self, msg: &[u8]) -> bool {
        let Some(x) = msg.get(..4) else { return false };
        let xid = u32::from_be_bytes([x[0], x[1], x[2], x[3]]);
        let tx = self
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&xid);
        tx.is_some_and(|t| t.send(msg.to_vec()).is_ok())
    }

    /// The connection ended: waiting calls fail.
    pub fn close(&self) {
        self.open.store(false, Ordering::Relaxed);
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }

    /// Calls `prog` `vers` `procedure` with `args` encoded: the results.
    ///
    /// # Errors
    ///
    /// `ConnectionLost` when the connection is down or goes, `Timeout`,
    /// and the reply's refusals.
    pub fn call(
        &self,
        prog: u32,
        vers: u32,
        procedure: u32,
        cred: &Credential,
        args: &[u8],
        timeout: Duration,
    ) -> Result<Vec<u8>, RpcError> {
        if !self.is_open() {
            return Err(RpcError::ConnectionLost(io::ErrorKind::NotConnected.into()));
        }
        let xid = self.xid.fetch_add(1, Ordering::Relaxed);
        let body = call_body {
            rpcvers: 2,
            prog,
            vers,
            proc: procedure,
            cred: cred.to_opaque().map_err(RpcError::Args)?,
            verf: none(),
        };
        let m = encode_call(xid, &body, args).map_err(RpcError::Args)?;
        let (tx, rx) = mpsc::channel();
        self.pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(xid, tx);
        if let Err(e) = self.send_record(&m) {
            return Err(RpcError::ConnectionLost(e));
        }
        match rx.recv_timeout(timeout) {
            Ok(reply) => results(&reply),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.pending
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&xid);
                Err(RpcError::Timeout)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(RpcError::ConnectionLost(
                io::ErrorKind::ConnectionAborted.into(),
            )),
        }
    }
}
