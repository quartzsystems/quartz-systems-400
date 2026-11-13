// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! ONC RPC calls (RFC 5531). The message header is `onc-rpc-client`'s
//! `rpc_msg`; the sockets, XIDs and retransmission are this client's, so a
//! scenario can send the same XID twice, a credential the server must
//! refuse, or bytes that are not arguments at all.

pub mod transport;

use std::fmt;
use std::io::Cursor;
use std::net::SocketAddr;
use std::time::Duration;

use onc_rpc_client::rpc::{
    accept_stat_data, auth_flavor, auth_unix, call_body, msg_body, opaque_auth, rejected_reply,
    reply_body, rpc_msg,
};
use onc_xdr::{Opaque, Pack, Unpack};

pub use transport::{Conn, Proto};

use crate::Error;

/// A credential.
#[derive(Clone, Debug)]
pub enum Cred {
    /// `AUTH_NONE`.
    None,
    /// `AUTH_SYS`.
    Sys {
        /// The machine name.
        machine: Vec<u8>,
        /// UID.
        uid: u32,
        /// GID.
        gid: u32,
        /// Supplementary GIDs.
        gids: Vec<u32>,
    },
    /// Another flavor with these body bytes (`RPCSEC_GSS` is 6).
    Raw(auth_flavor, Vec<u8>),
}

impl Cred {
    /// `AUTH_SYS` as `uid`/`gid` from machine `nfstest`.
    pub fn sys(uid: u32, gid: u32) -> Cred {
        Cred::Sys {
            machine: b"nfstest".to_vec(),
            uid,
            gid,
            gids: vec![gid],
        }
    }

    fn opaque(&self) -> opaque_auth<'static> {
        match self {
            Cred::None => opaque_auth::default(),
            Cred::Sys {
                machine,
                uid,
                gid,
                gids,
            } => opaque_auth::auth_unix(&auth_unix {
                stamp: 0x5153_3430,
                machinename: Opaque::owned(machine.clone()),
                uid: *uid,
                gid: *gid,
                gids: gids.clone(),
            }),
            Cred::Raw(f, body) => opaque_auth {
                flavor: *f,
                body: Opaque::owned(body.clone()),
            },
        }
    }
}

/// An RPC-level refusal (the call was not run).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// `PROG_UNAVAIL`.
    ProgUnavail,
    /// `PROG_MISMATCH` with the versions supported.
    ProgMismatch(u32, u32),
    /// `PROC_UNAVAIL`.
    ProcUnavail,
    /// `GARBAGE_ARGS`.
    GarbageArgs,
    /// `SYSTEM_ERR`.
    SystemErr,
    /// `MSG_DENIED` `RPC_MISMATCH`.
    RpcMismatch(u32, u32),
    /// `MSG_DENIED` `AUTH_ERROR` with the `auth_stat`.
    Auth(u32),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::ProgUnavail => write!(f, "PROG_UNAVAIL"),
            Refusal::ProgMismatch(l, h) => write!(f, "PROG_MISMATCH {l}..{h}"),
            Refusal::ProcUnavail => write!(f, "PROC_UNAVAIL"),
            Refusal::GarbageArgs => write!(f, "GARBAGE_ARGS"),
            Refusal::SystemErr => write!(f, "SYSTEM_ERR"),
            Refusal::RpcMismatch(l, h) => write!(f, "RPC_MISMATCH {l}..{h}"),
            Refusal::Auth(s) => write!(f, "AUTH_ERROR {s}"),
        }
    }
}

/// `AUTH_BADCRED`.
pub const AUTH_BADCRED: u32 = 1;
/// `AUTH_TOOWEAK`.
pub const AUTH_TOOWEAK: u32 = 5;

/// What one call came back with: the results' bytes or a refusal.
pub type Answer = Result<Vec<u8>, Refusal>;

/// An RPC client on one connection.
#[derive(Debug)]
pub struct Client {
    /// The socket.
    pub conn: Conn,
    /// The credential of each call.
    pub cred: Cred,
    /// UDP sends per call (the first and the retransmissions).
    pub tries: u32,
    xid: u32,
}

impl Client {
    /// A client of `addr`.
    pub fn new(addr: SocketAddr, proto: Proto, timeout: Duration, cred: Cred) -> Client {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.subsec_nanos());
        Client {
            conn: Conn::new(addr, proto, timeout),
            cred,
            tries: 3,
            xid: seed ^ (std::process::id() << 16),
        }
    }

    /// A fresh XID.
    pub fn next_xid(&mut self) -> u32 {
        self.xid = self.xid.wrapping_add(1);
        self.xid
    }

    /// The call message: header with this client's credential, then `args`.
    pub fn message(&self, xid: u32, prog: u32, vers: u32, proc: u32, args: &[u8]) -> Vec<u8> {
        message(xid, 2, prog, vers, proc, &self.cred, args)
    }

    /// Sends `msg` (XID `xid`) and waits for its reply, retransmitting over UDP.
    pub fn exchange(&mut self, xid: u32, msg: &[u8]) -> Result<Answer, Error> {
        let tries = if self.conn.proto == Proto::Udp {
            self.tries.max(1)
        } else {
            1
        };
        let mut last = None;
        for _ in 0..tries {
            self.conn.send(msg)?;
            match self.wait(xid) {
                Ok(a) => return Ok(a),
                Err(Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    last = Some(e);
                }
                Err(e) => return Err(e),
            }
        }
        Err(Error::Io(last.unwrap_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::TimedOut, "no reply")
        })))
    }

    /// Waits for the reply to `xid`, dropping others.
    pub fn wait(&mut self, xid: u32) -> Result<Answer, Error> {
        loop {
            let bytes = self.conn.recv()?;
            if let Some(a) = decode_reply(&bytes, xid)? {
                return Ok(a);
            }
        }
    }

    /// One call with argument bytes: the results' bytes or a refusal.
    pub fn call_bytes(
        &mut self,
        prog: u32,
        vers: u32,
        proc: u32,
        args: &[u8],
    ) -> Result<Answer, Error> {
        let xid = self.next_xid();
        let msg = self.message(xid, prog, vers, proc, args);
        self.exchange(xid, &msg)
    }

    /// One call: a refusal is an error.
    pub fn call_raw(
        &mut self,
        prog: u32,
        vers: u32,
        proc: u32,
        args: &[u8],
    ) -> Result<Vec<u8>, Error> {
        self.call_bytes(prog, vers, proc, args)?
            .map_err(Error::Refused)
    }

    /// One call of typed arguments and results.
    pub fn call<A: Pack, R: Unpack>(
        &mut self,
        prog: u32,
        vers: u32,
        proc: u32,
        args: &A,
    ) -> Result<R, Error> {
        let bytes = self.call_raw(prog, vers, proc, &pack(args)?)?;
        unpack_all(&bytes)
    }
}

/// A call message with RPC version `rpcvers`.
pub fn message(
    xid: u32,
    rpcvers: u32,
    prog: u32,
    vers: u32,
    proc: u32,
    cred: &Cred,
    args: &[u8],
) -> Vec<u8> {
    let m = rpc_msg {
        xid,
        body: msg_body::CALL(call_body {
            rpcvers,
            prog,
            vers,
            proc,
            cred: cred.opaque(),
            verf: opaque_auth::default(),
        }),
    };
    let mut out = Vec::with_capacity(m.packed_size() + args.len());
    m.pack(&mut out).expect("a Vec takes any write");
    out.extend_from_slice(args);
    out
}

/// The reply in `bytes` when it answers `xid`: `None` for another XID.
pub fn decode_reply(bytes: &[u8], xid: u32) -> Result<Option<Answer>, Error> {
    let mut c = Cursor::new(bytes);
    let (m, _) = rpc_msg::unpack(&mut c)?;
    if m.xid != xid {
        return Ok(None);
    }
    let at = usize::try_from(c.position()).expect("within the slice");
    let a = match m.body {
        msg_body::REPLY(reply_body::MSG_ACCEPTED(acc)) => match acc.reply_data {
            accept_stat_data::SUCCESS => Ok(bytes[at..].to_vec()),
            accept_stat_data::PROG_UNAVAIL => Err(Refusal::ProgUnavail),
            accept_stat_data::PROG_MISMATCH { low, high } => Err(Refusal::ProgMismatch(low, high)),
            accept_stat_data::PROC_UNAVAIL => Err(Refusal::ProcUnavail),
            accept_stat_data::GARBAGE_ARGS => Err(Refusal::GarbageArgs),
            _ => Err(Refusal::SystemErr),
        },
        msg_body::REPLY(reply_body::MSG_DENIED(rejected_reply::RPC_MISMATCH { low, high })) => {
            Err(Refusal::RpcMismatch(low, high))
        }
        msg_body::REPLY(reply_body::MSG_DENIED(rejected_reply::AUTH_ERROR(s))) => {
            Err(Refusal::Auth(s as u32))
        }
        _ => return Err(Error::Decode("a call where a reply was expected".into())),
    };
    Ok(Some(a))
}

/// `v` packed.
pub fn pack<A: Pack>(v: &A) -> Result<Vec<u8>, Error> {
    let mut out = Vec::with_capacity(v.packed_size());
    v.pack(&mut out)?;
    Ok(out)
}

/// `bytes` unpacked, every byte used.
pub fn unpack_all<R: Unpack>(bytes: &[u8]) -> Result<R, Error> {
    let mut c = Cursor::new(bytes);
    let (r, _) = R::unpack(&mut c)?;
    if c.position() as usize != bytes.len() {
        return Err(Error::Decode(format!(
            "{} bytes left after the results",
            bytes.len() - c.position() as usize
        )));
    }
    Ok(r)
}
