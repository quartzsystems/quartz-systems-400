// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Call and reply messages (RFC 5531 section 9) over the generated
//! `rpc` types. The arguments and results follow the header as bytes.

use super::auth::none;
use crate::types::rpc::{
    accept_stat, accepted_reply, accepted_reply_data, auth_stat, call_body, mismatch_info,
    msg_type, opaque_auth, rejected_reply, reply_body, rpc_msg, rpc_msg_body,
};
use crate::xdr::{self, Decoder, Encoder, Xdr};

/// The RPC version this implementation speaks.
pub const RPC_VERSION: u32 = 2;

/// A call's header and its arguments still to decode.
#[derive(Debug, Clone)]
pub struct Call<'a> {
    /// The transaction ID.
    pub xid: u32,
    /// The header.
    pub body: call_body,
    /// The arguments.
    pub args: Decoder<'a>,
}

/// Why a message is not a call this server can take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// Too short or not a call: dropped without an answer.
    Drop,
    /// RPC version other than 2: answered `RPC_MISMATCH`.
    RpcVersion {
        /// The transaction ID.
        xid: u32,
    },
    /// A credential or verifier that does not decode: answered `AUTH_ERROR`.
    BadAuth {
        /// The transaction ID.
        xid: u32,
        /// `AUTH_BADCRED` or `AUTH_BADVERF`.
        stat: auth_stat,
    },
}

/// The call in `msg`.
pub fn parse_call(msg: &[u8]) -> Result<Call<'_>, CallError> {
    let mut d = Decoder::new(msg);
    let xid = d.get_u32().map_err(|_| CallError::Drop)?;
    match msg_type::decode(&mut d) {
        Ok(msg_type::CALL) => {}
        _ => return Err(CallError::Drop),
    }
    let rpcvers = d.get_u32().map_err(|_| CallError::Drop)?;
    if rpcvers != RPC_VERSION {
        return Err(CallError::RpcVersion { xid });
    }
    let mut word = || d.get_u32().map_err(|_| CallError::Drop);
    let (prog, vers, proc_) = (word()?, word()?, word()?);
    let cred = opaque_auth::decode(&mut d).map_err(|_| CallError::BadAuth {
        xid,
        stat: auth_stat::AUTH_BADCRED,
    })?;
    let verifier = opaque_auth::decode(&mut d).map_err(|_| CallError::BadAuth {
        xid,
        stat: auth_stat::AUTH_BADVERF,
    })?;
    Ok(Call {
        xid,
        body: call_body {
            rpcvers,
            prog,
            vers,
            proc: proc_,
            cred,
            verf: verifier,
        },
        args: d,
    })
}

/// A call message with the arguments `args` already encoded.
pub fn encode_call(xid: u32, body: &call_body, args: &[u8]) -> xdr::Result<Vec<u8>> {
    let msg = rpc_msg {
        xid,
        body: rpc_msg_body::CALL(body.clone()),
    };
    let mut e = Encoder::new();
    msg.encode(&mut e)?;
    e.put_raw(args);
    Ok(e.into_bytes())
}

fn reply(xid: u32, body: reply_body, results: &[u8]) -> Vec<u8> {
    let msg = rpc_msg {
        xid,
        body: rpc_msg_body::REPLY(body),
    };
    let mut e = Encoder::new();
    // A reply header has no unbounded item: encoding cannot fail.
    let _ = msg.encode(&mut e);
    e.put_raw(results);
    e.into_bytes()
}

fn accepted(xid: u32, data: accepted_reply_data, results: &[u8]) -> Vec<u8> {
    reply(
        xid,
        reply_body::MSG_ACCEPTED(accepted_reply {
            verf: none(),
            reply_data: data,
        }),
        results,
    )
}

/// `SUCCESS` with the results `results` already encoded.
pub fn reply_success(xid: u32, results: &[u8]) -> Vec<u8> {
    accepted(xid, accepted_reply_data::SUCCESS, results)
}

/// An accepted call that did not run.
pub fn reply_refused(xid: u32, why: AcceptError) -> Vec<u8> {
    let data = match why {
        AcceptError::ProgUnavail => accepted_reply_data::default(accept_stat::PROG_UNAVAIL),
        AcceptError::ProgMismatch { low, high } => {
            accepted_reply_data::PROG_MISMATCH(mismatch_info { low, high })
        }
        AcceptError::ProcUnavail => accepted_reply_data::default(accept_stat::PROC_UNAVAIL),
        AcceptError::GarbageArgs => accepted_reply_data::default(accept_stat::GARBAGE_ARGS),
        AcceptError::SystemErr => accepted_reply_data::default(accept_stat::SYSTEM_ERR),
    };
    accepted(xid, data, &[])
}

/// `MSG_DENIED`/`RPC_MISMATCH` 2 to 2.
pub fn reply_rpc_mismatch(xid: u32) -> Vec<u8> {
    let r = rejected_reply::RPC_MISMATCH(mismatch_info {
        low: RPC_VERSION,
        high: RPC_VERSION,
    });
    reply(xid, reply_body::MSG_DENIED(r), &[])
}

/// `MSG_DENIED`/`AUTH_ERROR` with `stat`.
pub fn reply_auth_error(xid: u32, stat: auth_stat) -> Vec<u8> {
    reply(
        xid,
        reply_body::MSG_DENIED(rejected_reply::AUTH_ERROR(stat)),
        &[],
    )
}

/// Why an accepted call did not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum AcceptError {
    /// No such program here.
    #[error("program unavailable")]
    ProgUnavail,
    /// Not a version the program has.
    #[error("program version mismatch: {low} to {high}")]
    ProgMismatch {
        /// The lowest version.
        low: u32,
        /// The highest version.
        high: u32,
    },
    /// No such procedure in the version.
    #[error("procedure unavailable")]
    ProcUnavail,
    /// The arguments do not decode.
    #[error("the arguments do not decode")]
    GarbageArgs,
    /// The server failed.
    #[error("system error")]
    SystemErr,
}

/// Why a call was denied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Denied {
    /// The server speaks other RPC versions.
    #[error("RPC version mismatch: {low} to {high}")]
    RpcMismatch {
        /// The lowest version.
        low: u32,
        /// The highest version.
        high: u32,
    },
    /// The credential or verifier is refused.
    #[error("authentication error {0:?}")]
    Auth(auth_stat),
}

/// A reply: its transaction ID and the results or why there are none.
#[derive(Debug, Clone)]
pub struct Reply<'a> {
    /// The transaction ID.
    pub xid: u32,
    /// The results to decode, or why the call did not run.
    pub outcome: Result<Decoder<'a>, ReplyError>,
}

/// A reply without results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReplyError {
    /// Accepted but not run.
    #[error("{0}")]
    Refused(AcceptError),
    /// Denied.
    #[error("{0}")]
    Denied(Denied),
}

/// The reply in `msg`.
pub fn parse_reply(msg: &[u8]) -> xdr::Result<Reply<'_>> {
    let mut d = Decoder::new(msg);
    let m = rpc_msg::decode(&mut d)?;
    let rpc_msg_body::REPLY(body) = m.body else {
        return Err(xdr::Error::Discriminant {
            ty: "msg_type",
            value: 0,
        });
    };
    let outcome = match body {
        reply_body::MSG_ACCEPTED(a) => match a.reply_data {
            accepted_reply_data::SUCCESS => Ok(d),
            accepted_reply_data::PROG_MISMATCH(m) => Err(AcceptError::ProgMismatch {
                low: m.low,
                high: m.high,
            }),
            accepted_reply_data::default(s) => Err(match s {
                accept_stat::PROG_UNAVAIL => AcceptError::ProgUnavail,
                accept_stat::PROC_UNAVAIL => AcceptError::ProcUnavail,
                accept_stat::GARBAGE_ARGS => AcceptError::GarbageArgs,
                _ => AcceptError::SystemErr,
            }),
        }
        .map_err(ReplyError::Refused),
        reply_body::MSG_DENIED(r) => Err(ReplyError::Denied(match r {
            rejected_reply::RPC_MISMATCH(m) => Denied::RpcMismatch {
                low: m.low,
                high: m.high,
            },
            rejected_reply::AUTH_ERROR(s) => Denied::Auth(s),
        })),
    };
    Ok(Reply {
        xid: m.xid,
        outcome,
    })
}

/// The transaction ID of a message, if it has one.
pub fn xid_of(msg: &[u8]) -> Option<u32> {
    msg.get(..4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}
