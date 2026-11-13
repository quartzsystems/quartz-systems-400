// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Callbacks (RFC 7530 section 10.2, RFC 8881 section 20): the 4.0
//! callback program at the address `SETCLIENTID` gave ([`CbConnector`],
//! [`TcpConnector`] over [`crate::rpc::RpcClient`]), probed with
//! `CB_NULL`; 4.1's backchannel on the session's own connection
//! ([`crate::rpc::back::BackChannel`]) with `CB_SEQUENCE`. Both send
//! `CB_COMPOUND(CB_RECALL)`.

use std::time::Duration;

use crate::rpc::{ClientConfig, Credential, Proto, RpcClient, RpcError};
use crate::rpcb::uaddr::from_uaddr;
use crate::types::nfs4::{
    CB_COMPOUND4args, CB_COMPOUND4res, CB_RECALL4args, CB_SEQUENCE4args, nfs_cb_argop4, nfs_fh4,
    nfsstat4, sessionid4, stateid4, utf8string,
};
use crate::xdr::{self};

/// The callback program's version.
pub const CB_VERSION: u32 = 1;

/// A connection to a 4.0 client's callback program.
pub trait CbChannel: Send {
    /// Calls procedure `procedure` of `prog` with `args` encoded: the results.
    ///
    /// # Errors
    ///
    /// The transport's.
    fn call(&mut self, prog: u32, procedure: u32, args: &[u8]) -> Result<Vec<u8>, RpcError>;
}

/// Makes connections to 4.0 callback addresses.
pub trait CbConnector: Send + Sync {
    /// A channel to `uaddr` over `netid` (`tcp`, `tcp6`, `udp`, `udp6`),
    /// or none when it is not an address this server reaches.
    fn connect(&self, netid: &str, uaddr: &str) -> Option<Box<dyn CbChannel>>;
}

/// [`CbConnector`] over [`RpcClient`], calling as `AUTH_NONE`.
#[derive(Debug, Clone)]
pub struct TcpConnector {
    /// How calls are made.
    pub cfg: ClientConfig,
}

impl Default for TcpConnector {
    fn default() -> Self {
        Self {
            cfg: ClientConfig {
                timeout: Duration::from_secs(5),
                retransmits: 1,
                ..ClientConfig::default()
            },
        }
    }
}

struct Rpc(RpcClient);

impl CbChannel for Rpc {
    fn call(&mut self, prog: u32, procedure: u32, args: &[u8]) -> Result<Vec<u8>, RpcError> {
        self.0
            .call_raw(prog, CB_VERSION, procedure, &Credential::None, args)
    }
}

impl CbConnector for TcpConnector {
    fn connect(&self, netid: &str, uaddr: &str) -> Option<Box<dyn CbChannel>> {
        let proto = match netid {
            "tcp" | "tcp6" => Proto::Tcp,
            "udp" | "udp6" => Proto::Udp,
            _ => return None,
        };
        let addr = from_uaddr(uaddr)?;
        Some(Box::new(Rpc(RpcClient::new(addr, proto, self.cfg.clone()))))
    }
}

/// A connector that reaches nobody (no 4.0 callbacks: no delegations).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCallbacks;

impl CbConnector for NoCallbacks {
    fn connect(&self, _: &str, _: &str) -> Option<Box<dyn CbChannel>> {
        None
    }
}

/// `CB_COMPOUND` arguments recalling delegation `stateid` of `fh`; with
/// `seq` (session, sequence ID) the 4.1 form.
///
/// # Errors
///
/// Encoding's (never with a valid handle).
pub fn recall_args(
    ident: u32,
    stateid: stateid4,
    fh: &[u8],
    seq: Option<(sessionid4, u32)>,
) -> xdr::Result<Vec<u8>> {
    let mut ops = Vec::new();
    let minor = if let Some((sid, seqid)) = seq {
        ops.push(nfs_cb_argop4::OP_CB_SEQUENCE(CB_SEQUENCE4args {
            csa_sessionid: sid,
            csa_sequenceid: seqid,
            csa_slotid: 0,
            csa_highest_slotid: 0,
            csa_cachethis: false,
            csa_referring_call_lists: Vec::new(),
        }));
        1
    } else {
        0
    };
    ops.push(nfs_cb_argop4::OP_CB_RECALL(CB_RECALL4args {
        stateid,
        truncate: false,
        fh: nfs_fh4(fh.to_vec()),
    }));
    xdr::to_bytes(&CB_COMPOUND4args {
        tag: utf8string(Vec::new()),
        minorversion: minor,
        callback_ident: ident,
        argarray: ops,
    })
}

/// The status of a `CB_COMPOUND` reply.
///
/// # Errors
///
/// `BadReply` when it does not decode.
pub fn reply_status(results: &[u8]) -> Result<nfsstat4, RpcError> {
    xdr::from_bytes::<CB_COMPOUND4res>(results)
        .map(|r| r.status)
        .map_err(RpcError::BadReply)
}
