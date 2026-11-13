// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `COMPOUND` (RFC 7530 section 15.2, RFC 8881 sections 2.10.6 and
//! 16.2): the header is decoded here, each operation only when its turn
//! comes, so an operation number the protocol lacks answers `OP_ILLEGAL`
//! with `NFS4ERR_OP_ILLEGAL` and one that does not decode answers
//! `NFS4ERR_BADXDR`; evaluation stops at the first error. Minor version
//! 2 and up is `NFS4ERR_MINOR_VERS_MISMATCH` with no results.
//!
//! Minor version 1: `SEQUENCE` first (its slot's reply kept for
//! exactly-once replies), else one of `EXCHANGE_ID`, `CREATE_SESSION`,
//! `DESTROY_SESSION`, `BIND_CONN_TO_SESSION`, `DESTROY_CLIENTID` alone
//! (`NFS4ERR_NOT_ONLY_OP`), anything else `NFS4ERR_OP_NOT_IN_SESSION`;
//! `SEQUENCE` later is `NFS4ERR_SEQUENCE_POS`. Limits: 4.0 answers
//! `NFS4ERR_RESOURCE` past [`MAX_OPS_40`] operations or a reply past
//! [`MAX_REPLY_40`]; 4.1 answers the session's `NFS4ERR_TOO_MANY_OPS`,
//! `NFS4ERR_REQ_TOO_BIG`, `NFS4ERR_REP_TOO_BIG` and
//! `NFS4ERR_REP_TOO_BIG_TO_CACHE`.

use std::sync::Arc;
use std::time::Instant;

use super::Fh;
use super::pseudo::Tree;
use super::server::Server;
use crate::rpc::back::BackChannel;
use crate::rpc::{AcceptError, Answer, CallContext};
use crate::server::vfs::Vfs;
use crate::types::nfs4::{nfs_argop4, nfs_opnum4, nfs_resop4, nfsstat4, stateid4};
use crate::xdr::{Decoder, Encoder, Xdr};

/// Operations in one 4.0 compound at most.
pub const MAX_OPS_40: u32 = 128;

/// The largest 4.0 reply (a record holds 1 MiB and 16 KiB).
pub const MAX_REPLY_40: usize = (1 << 20) + 12 * 1024;

/// What an operation answers.
#[derive(Debug)]
pub(crate) enum Res {
    /// Success, its results.
    Ok(nfs_resop4),
    /// An error with no results beyond the status.
    Err(nfsstat4),
    /// An error with results (`LOCK` denied, `SETCLIENTID` in use).
    Body(nfsstat4, nfs_resop4),
    /// The encoded result, kept from before (a 4.0 retransmission).
    Raw(nfsstat4, Vec<u8>),
}

impl From<nfsstat4> for Res {
    fn from(s: nfsstat4) -> Self {
        Self::Err(s)
    }
}

/// One compound's evaluation.
pub(crate) struct Cx<'a> {
    pub srv: &'a Arc<Server>,
    pub vfs: &'a dyn Vfs,
    pub ctx: &'a CallContext,
    pub back: Option<&'a Arc<BackChannel>>,
    pub minor: u32,
    pub cur: Fh,
    pub saved: Fh,
    pub cur_sid: Option<stateid4>,
    pub saved_sid: Option<stateid4>,
    /// 4.1: the session and slot of the `SEQUENCE`.
    pub session: Option<([u8; 16], u32)>,
    /// 4.1: the session's client.
    pub client: Option<u64>,
    /// The pseudo file system, made when first needed.
    pub tree: Option<Tree>,
    pub now: Instant,
    /// The ops in the compound and this one's index.
    pub nops: u32,
    pub index: u32,
    /// The request's bytes and digest (4.1 `SEQUENCE`).
    pub req_len: usize,
    pub digest: u64,
    /// `sa_cachethis`.
    pub cachethis: bool,
}

/// Operations that may stand alone, without `SEQUENCE` (4.1).
fn sessionless(op: u32) -> bool {
    use nfs_opnum4 as O;
    [
        O::OP_EXCHANGE_ID,
        O::OP_CREATE_SESSION,
        O::OP_DESTROY_SESSION,
        O::OP_BIND_CONN_TO_SESSION,
        O::OP_DESTROY_CLIENTID,
    ]
    .iter()
    .any(|o| num(*o) == op)
}

fn fnv64(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, x| {
        (h ^ u64::from(*x)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// The encoded result of op `op` failing with `s`.
fn error_bytes(op: u32, s: nfsstat4) -> Vec<u8> {
    let mut e = Encoder::new();
    e.put_u32(op);
    let _ = s.encode(&mut e);
    if op == num(nfs_opnum4::OP_SETATTR) {
        e.put_u32(0);
    }
    e.into_bytes()
}

/// A result's bytes and status.
pub(crate) fn encode(op: u32, r: Res) -> (nfsstat4, Vec<u8>) {
    match r {
        Res::Ok(v) => match crate::xdr::to_bytes(&v) {
            Ok(b) => (nfsstat4::NFS4_OK, b),
            Err(_) => (
                nfsstat4::NFS4ERR_SERVERFAULT,
                error_bytes(op, nfsstat4::NFS4ERR_SERVERFAULT),
            ),
        },
        Res::Body(s, v) => match crate::xdr::to_bytes(&v) {
            Ok(b) => (s, b),
            Err(_) => (
                nfsstat4::NFS4ERR_SERVERFAULT,
                error_bytes(op, nfsstat4::NFS4ERR_SERVERFAULT),
            ),
        },
        Res::Err(s) => (s, error_bytes(op, s)),
        Res::Raw(s, b) => (s, b),
    }
}

/// Whether op number `op` exists at minor version `minor`.
fn known(op: u32, minor: u32) -> bool {
    let top = if minor == 0 {
        num(nfs_opnum4::OP_RELEASE_LOCKOWNER)
    } else {
        num(nfs_opnum4::OP_RECLAIM_COMPLETE)
    };
    (3..=top).contains(&op)
}

/// The whole reply.
fn reply(status: nfsstat4, tag: &[u8], results: &[Vec<u8>]) -> Answer {
    let mut e = Encoder::new();
    let _ = status.encode(&mut e);
    e.put_opaque(tag, u32::MAX)
        .map_err(|_| AcceptError::SystemErr)?;
    e.put_u32(u32::try_from(results.len()).unwrap_or(0));
    for r in results {
        e.put_raw(r);
    }
    Ok(Some(e.into_bytes()))
}

/// One `COMPOUND`.
pub(crate) fn compound(
    srv: &Arc<Server>,
    vfs: &dyn Vfs,
    ctx: &CallContext,
    back: Option<&Arc<BackChannel>>,
    d: &mut Decoder<'_>,
) -> Answer {
    let whole = d.clone().rest();
    let tag = d
        .get_opaque(u32::MAX)
        .map_err(|_| AcceptError::GarbageArgs)?;
    let minor = d.get_u32().map_err(|_| AcceptError::GarbageArgs)?;
    let nops = d.get_u32().map_err(|_| AcceptError::GarbageArgs)?;
    if minor > 1 {
        return reply(nfsstat4::NFS4ERR_MINOR_VERS_MISMATCH, &tag, &[]);
    }
    let mut cx = Cx {
        srv,
        vfs,
        ctx,
        back,
        minor,
        cur: Fh::None,
        saved: Fh::None,
        cur_sid: None,
        saved_sid: None,
        session: None,
        client: None,
        tree: None,
        now: srv.now(),
        nops,
        index: 0,
        req_len: whole.len(),
        digest: fnv64(whole),
        cachethis: false,
    };
    let mut results: Vec<Vec<u8>> = Vec::new();
    let mut size = 12 + tag.len().div_ceil(4) * 4;
    let mut status = nfsstat4::NFS4_OK;
    if minor == 0 && nops > MAX_OPS_40 {
        let op = d.clone().get_u32().unwrap_or(num(nfs_opnum4::OP_ILLEGAL));
        let op = if known(op, 0) {
            op
        } else {
            num(nfs_opnum4::OP_ILLEGAL)
        };
        results.push(error_bytes(op, nfsstat4::NFS4ERR_RESOURCE));
        return reply(nfsstat4::NFS4ERR_RESOURCE, &tag, &results);
    }
    for i in 0..nops {
        cx.index = i;
        let Ok(op) = d.clone().get_u32() else {
            // Fewer operations than the count says: the slot is free again.
            cx.finish(None);
            return Err(AcceptError::GarbageArgs);
        };
        let (st, bytes) = if known(op, minor) {
            match nfs_argop4::decode(d) {
                Err(_) => (
                    nfsstat4::NFS4ERR_BADXDR,
                    error_bytes(op, nfsstat4::NFS4ERR_BADXDR),
                ),
                Ok(arg) => {
                    if let Some(s) = position(&cx, op, i, nops) {
                        (s, error_bytes(op, s))
                    } else {
                        let r = cx.op(arg);
                        if let Res::Raw(_, ref b) = r
                            && op == num(nfs_opnum4::OP_SEQUENCE)
                            && cx.session.is_none()
                        {
                            // A retransmission on its slot: the reply kept.
                            return Ok(Some(b.clone()));
                        }
                        encode(op, r)
                    }
                }
            }
        } else {
            let ill = num(nfs_opnum4::OP_ILLEGAL);
            (
                nfsstat4::NFS4ERR_OP_ILLEGAL,
                error_bytes(ill, nfsstat4::NFS4ERR_OP_ILLEGAL),
            )
        };
        let limit = cx.reply_limit();
        let (st, bytes) = if size + bytes.len() > limit.0 {
            let s = if minor == 0 {
                nfsstat4::NFS4ERR_RESOURCE
            } else if limit.1 {
                nfsstat4::NFS4ERR_REP_TOO_BIG_TO_CACHE
            } else {
                nfsstat4::NFS4ERR_REP_TOO_BIG
            };
            (s, error_bytes(op_of(&bytes), s))
        } else {
            (st, bytes)
        };
        size += bytes.len();
        results.push(bytes);
        status = st;
        if st != nfsstat4::NFS4_OK {
            break;
        }
    }
    let out = reply(status, &tag, &results);
    cx.finish(out.as_ref().ok().and_then(Option::as_ref));
    out
}

/// The op number at the head of an encoded result.
fn op_of(b: &[u8]) -> u32 {
    b.get(..4).map_or(num(nfs_opnum4::OP_ILLEGAL), |x| {
        u32::from_be_bytes([x[0], x[1], x[2], x[3]])
    })
}

/// The 4.1 rules of where an operation may be.
fn position(cx: &Cx<'_>, op: u32, i: u32, n: u32) -> Option<nfsstat4> {
    use nfs_opnum4 as O;
    if cx.minor == 0 {
        return None;
    }
    let seq = num(O::OP_SEQUENCE);
    if i == 0 {
        if op == seq || op == num(O::OP_ILLEGAL) {
            return None;
        }
        if sessionless(op) {
            return (n > 1).then_some(nfsstat4::NFS4ERR_NOT_ONLY_OP);
        }
        return Some(nfsstat4::NFS4ERR_OP_NOT_IN_SESSION);
    }
    if op == seq {
        return Some(nfsstat4::NFS4ERR_SEQUENCE_POS);
    }
    if op == num(O::OP_BIND_CONN_TO_SESSION) {
        return Some(nfsstat4::NFS4ERR_NOT_ONLY_OP);
    }
    None
}

impl Cx<'_> {
    /// The most bytes the reply may have, and whether that is because
    /// it is to be kept.
    fn reply_limit(&self) -> (usize, bool) {
        if self.minor == 0 {
            return (MAX_REPLY_40, false);
        }
        let Some((sid, _)) = self.session else {
            return (MAX_REPLY_40, false);
        };
        let t = self.srv.t();
        match t.sessions.get(&sid) {
            Some(s) if self.cachethis => (
                usize::try_from(s.fore.ca_maxresponsesize_cached).unwrap_or(usize::MAX),
                true,
            ),
            Some(s) => (
                usize::try_from(s.fore.ca_maxresponsesize).unwrap_or(usize::MAX),
                false,
            ),
            None => (MAX_REPLY_40, false),
        }
    }
}

/// An operation's number.
fn num(o: nfs_opnum4) -> u32 {
    o.value().cast_unsigned()
}
