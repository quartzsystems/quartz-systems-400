// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The calls the lock manager makes: `NLM_GRANTED` (or `GRANTED_MSG`)
//! to a client whose blocked lock is now held, the `_RES` messages that
//! answer the `_MSG` forms, and `SM_MON` to the local status monitor.
//! The client's lock manager is found through the locator at the
//! client's address, for the version it called with.

use std::net::{IpAddr, SocketAddr};

use super::conv;
use super::state::{Blocked, NlmState};
use crate::rpc::{Credential, Proto, RpcClient};
use crate::server::locate::Host;
use crate::types::nlm::{NLM_PROG, nlm_stats, nlm4_stats};
use crate::types::nsm::{SM_PROG, mon, mon_id, my_id, res};
use crate::xdr::{Decoder, Xdr};

/// `NLM_GRANTED_MSG`.
const GRANTED_MSG: u32 = 10;
/// `NLM_GRANTED`.
const GRANTED: u32 = 5;
/// `NLM_SM`'s version and its notify procedure, which the monitor calls back.
pub const SM_VERS: u32 = 2;
/// `NLM_SM_NOTIFY`.
pub const SM_NOTIFY: u32 = 1;

fn client(st: &NlmState, ip: IpAddr, vers: u32) -> Option<RpcClient> {
    let addr = st
        .cfg
        .locator
        .find(&Host::Addr(ip), NLM_PROG::NUMBER, vers, Proto::Udp)?;
    Some(RpcClient::new(addr, Proto::Udp, st.cfg.rpc.clone()))
}

/// Sends an `_RES` message with its results to the client at `peer`.
pub fn send_res(st: &NlmState, peer: SocketAddr, vers: u32, procedure: u32, results: &[u8]) {
    let Some(mut c) = client(st, peer.ip(), vers) else {
        tracing::warn!("no lock manager at {} for a reply", peer.ip());
        return;
    };
    if let Err(e) = c.call_raw(
        NLM_PROG::NUMBER,
        vers,
        procedure,
        &Credential::None,
        results,
    ) {
        tracing::warn!("reply message to {peer} failed: {e}");
    }
}

/// What became of a grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant {
    /// The client has it (or, sent as a message, will say).
    Taken,
    /// The client does not want it.
    Refused,
    /// The client did not answer.
    NoAnswer,
}

/// Tells the client its blocked lock is held.
pub fn send_granted(st: &NlmState, b: &Blocked) -> Grant {
    let Some(mut c) = client(st, b.peer.ip(), b.vers) else {
        return Grant::NoAnswer;
    };
    let cookie = st.cookie();
    let Ok(args) = conv::testargs_bytes(b.vers, &cookie, b.exclusive, &b.lock) else {
        return Grant::Refused;
    };
    let procedure = if b.msg { GRANTED_MSG } else { GRANTED };
    if b.msg {
        st.sent_grant(cookie.clone(), b.clone());
    }
    for _ in 0..st.cfg.grant_tries.max(1) {
        match c.call_raw(
            NLM_PROG::NUMBER,
            b.vers,
            procedure,
            &Credential::None,
            &args,
        ) {
            Ok(_) if b.msg => return Grant::Taken,
            Ok(r) => {
                return if granted(b.vers, &r) {
                    Grant::Taken
                } else {
                    Grant::Refused
                };
            }
            Err(e) if e.is_retryable() => {}
            Err(_) => break,
        }
    }
    if b.msg {
        st.grant_answered(&cookie);
    }
    Grant::NoAnswer
}

/// Whether `nlm_res`/`nlm4_res` bytes say granted.
fn granted(vers: u32, r: &[u8]) -> bool {
    let mut d = Decoder::new(r);
    if vers == 4 {
        crate::types::nlm::nlm4_res::decode(&mut d)
            .is_ok_and(|x| x.stat.stat == nlm4_stats::NLM4_GRANTED)
    } else {
        crate::types::nlm::nlm_res::decode(&mut d)
            .is_ok_and(|x| x.stat.stat == nlm_stats::nlm_granted)
    }
}

/// Asks the local status monitor to watch `host`, calling back this
/// lock manager's `NLM_SM_NOTIFY` when it restarts. Whether it agreed.
pub fn monitor(st: &NlmState, host: &[u8]) -> bool {
    let local = Host::Addr(IpAddr::from([127, 0, 0, 1]));
    let Some(addr) = st.cfg.locator.find(&local, SM_PROG::NUMBER, 1, Proto::Udp) else {
        tracing::warn!("no status monitor to watch a lock client");
        return false;
    };
    let m = mon {
        mon_id: mon_id {
            mon_name: host.to_vec(),
            my_id: my_id {
                my_name: b"127.0.0.1".to_vec(),
                my_prog: NLM_PROG::NUMBER.cast_signed(),
                my_vers: SM_VERS.cast_signed(),
                my_proc: SM_NOTIFY.cast_signed(),
            },
        },
        r#priv: [0; 16],
    };
    crate::nsm::call::sm_mon(addr, &st.cfg.rpc, &m).is_ok_and(|r| r.res_stat == res::stat_succ)
}
