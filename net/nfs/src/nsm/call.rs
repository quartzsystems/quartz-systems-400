// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Calls to a status monitor: what the lock manager asks of the local
//! one (`SM_MON`, `SM_UNMON`, `SM_UNMON_ALL`, `SM_STAT`) and what one
//! monitor tells another (`SM_NOTIFY`). Each opens a client of `addr`;
//! a caller with many calls keeps its own [`RpcClient`] and uses
//! `call_proc` with these types.

use std::net::SocketAddr;

use crate::rpc::{ClientConfig, Credential, Proto, RpcClient, RpcError};
use crate::types::nsm::SM_PROG::SM_VERS::{Args, Res};
use crate::types::nsm::{mon, mon_id, my_id, sm_name, sm_stat_res, stat_chge};

fn client(addr: SocketAddr, cfg: &ClientConfig) -> RpcClient {
    RpcClient::new(addr, Proto::Udp, cfg.clone())
}

fn bad() -> RpcError {
    RpcError::BadReply(crate::xdr::Error::Discriminant {
        ty: "procedure",
        value: 0,
    })
}

/// `SM_MON`: the state of the monitor and whether it agreed.
///
/// # Errors
///
/// The transport's.
pub fn sm_mon(addr: SocketAddr, cfg: &ClientConfig, m: &mon) -> Result<sm_stat_res, RpcError> {
    match client(addr, cfg).call_proc::<Args, Res>(&Credential::None, &Args::SM_MON(m.clone()))? {
        Res::SM_MON(r) => Ok(r),
        _ => Err(bad()),
    }
}

/// `SM_UNMON`: the monitor's state.
///
/// # Errors
///
/// The transport's.
pub fn sm_unmon(addr: SocketAddr, cfg: &ClientConfig, m: &mon_id) -> Result<i32, RpcError> {
    match client(addr, cfg).call_proc::<Args, Res>(&Credential::None, &Args::SM_UNMON(m.clone()))? {
        Res::SM_UNMON(r) => Ok(r.state),
        _ => Err(bad()),
    }
}

/// `SM_UNMON_ALL`: the monitor's state.
///
/// # Errors
///
/// The transport's.
pub fn sm_unmon_all(addr: SocketAddr, cfg: &ClientConfig, m: &my_id) -> Result<i32, RpcError> {
    match client(addr, cfg).call_proc::<Args, Res>(&Credential::None, &Args::SM_UNMON_ALL(m.clone()))? {
        Res::SM_UNMON_ALL(r) => Ok(r.state),
        _ => Err(bad()),
    }
}

/// `SM_STAT`.
///
/// # Errors
///
/// The transport's.
pub fn sm_stat(addr: SocketAddr, cfg: &ClientConfig, host: &[u8]) -> Result<sm_stat_res, RpcError> {
    let a = Args::SM_STAT(sm_name {
        mon_name: host.to_vec(),
    });
    match client(addr, cfg).call_proc::<Args, Res>(&Credential::None, &a)? {
        Res::SM_STAT(r) => Ok(r),
        _ => Err(bad()),
    }
}

/// `SM_NOTIFY`: `me` restarted and is now in `state`.
///
/// # Errors
///
/// The transport's.
pub fn sm_notify(addr: SocketAddr, cfg: &ClientConfig, me: &[u8], state: i32) -> Result<(), RpcError> {
    let a = Args::SM_NOTIFY(stat_chge {
        mon_name: me.to_vec(),
        state,
    });
    match client(addr, cfg).call_proc::<Args, Res>(&Credential::None, &a)? {
        Res::SM_NOTIFY => Ok(()),
        _ => Err(bad()),
    }
}
