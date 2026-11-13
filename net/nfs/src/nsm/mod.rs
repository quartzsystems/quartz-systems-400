// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The network status monitor, version 1 (XNFS chapter 11): `QNFSNSMD`.
//!
//! - `SM_STAT`: the state, `stat_succ`.
//! - `SM_MON`, `SM_UNMON`, `SM_UNMON_ALL`: from this host only (a
//!   caller elsewhere gets `stat_fail`, or its state with nothing
//!   changed); the monitored hosts are written at each change
//!   ([`record`]).
//! - `SM_SIMU_CRASH`: from this host only (`PROC_UNAVAIL` from
//!   another): the state up by two and every monitored host told.
//! - `SM_NOTIFY` from a host that restarted: each local program
//!   monitoring it is called back with its private bytes.
//!
//! At a start the state moves up by two and every host monitored
//! before is sent `SM_NOTIFY` ([`state::NsmState::notify_hosts`], run
//! by [`crate::server::run::nsmd`]). [`call`] has the client calls.

pub mod call;
pub mod record;
pub mod state;

use std::net::IpAddr;
use std::sync::Arc;

use self::record::MonRecord;
use self::state::{NsmState, Work};
use crate::rpc::{AcceptError, Answer, CallContext, Program, decode_args, encode_res};
use crate::types::nsm::{SM_PROG, res, sm_stat, sm_stat_res};
use crate::xdr::Decoder;

/// Whether a caller is on this host.
#[must_use]
pub fn local(peer: IpAddr) -> bool {
    match peer {
        IpAddr::V4(a) => a.is_loopback(),
        IpAddr::V6(a) => a.is_loopback() || a.to_ipv4_mapped().is_some_and(|v| v.is_loopback()),
    }
}

/// The monitor's program.
#[derive(Debug, Clone)]
pub struct Nsm {
    /// Its table.
    pub state: Arc<NsmState>,
}

impl Nsm {
    /// The program over `state`.
    pub fn new(state: Arc<NsmState>) -> Self {
        Self { state }
    }
}

impl Program for Nsm {
    fn program(&self) -> u32 {
        SM_PROG::NUMBER
    }
    fn versions(&self) -> &[u32] {
        &[1]
    }
    fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        use SM_PROG::SM_VERS::{Args, Res};
        let me = local(ctx.peer.ip());
        let st = &self.state;
        let res = match decode_args::<Args>(ctx, d)? {
            Args::SM_NULL => Res::SM_NULL,
            Args::SM_STAT(_) => Res::SM_STAT(sm_stat_res {
                res_stat: res::stat_succ,
                state: st.state_i32(),
            }),
            Args::SM_MON(m) => {
                let ok = me;
                if ok {
                    let id = &m.mon_id.my_id;
                    st.monitor(MonRecord {
                        mon_name: m.mon_id.mon_name.clone(),
                        my_name: id.my_name.clone(),
                        my_prog: id.my_prog.cast_unsigned(),
                        my_vers: id.my_vers.cast_unsigned(),
                        my_proc: id.my_proc.cast_unsigned(),
                        private: m.r#priv,
                    });
                }
                Res::SM_MON(sm_stat_res {
                    res_stat: if ok { res::stat_succ } else { res::stat_fail },
                    state: st.state_i32(),
                })
            }
            Args::SM_UNMON(m) => {
                if me {
                    let id = &m.my_id;
                    st.unmonitor(
                        &m.mon_name,
                        &id.my_name,
                        id.my_prog.cast_unsigned(),
                        id.my_vers.cast_unsigned(),
                        id.my_proc.cast_unsigned(),
                    );
                }
                Res::SM_UNMON(sm_stat {
                    state: st.state_i32(),
                })
            }
            Args::SM_UNMON_ALL(id) => {
                if me {
                    st.unmonitor_all(
                        &id.my_name,
                        id.my_prog.cast_unsigned(),
                        id.my_vers.cast_unsigned(),
                        id.my_proc.cast_unsigned(),
                    );
                }
                Res::SM_UNMON_ALL(sm_stat {
                    state: st.state_i32(),
                })
            }
            Args::SM_SIMU_CRASH => {
                if !me {
                    return Err(AcceptError::ProcUnavail);
                }
                let hosts = st.simu_crash();
                st.later(Work::Tell(hosts));
                Res::SM_SIMU_CRASH
            }
            Args::SM_NOTIFY(c) => {
                st.later(Work::Notified(c.mon_name, ctx.peer.ip(), c.state));
                Res::SM_NOTIFY
            }
        };
        encode_res(&res)
    }
}
