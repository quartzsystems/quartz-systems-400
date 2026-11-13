// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The network lock manager (XNFS chapters 10 and 14): `QNFSNLMD`.
//! Version 4 in full, versions 1 and 3 as its 32-bit forms; version 2
//! is `NLM_SM`, the status monitor's callback (from this host only).
//!
//! A lock owner (caller name, `svid`, owner handle) is an IFS
//! sub-owner ([`state`]), so remote owners conflict with each other and
//! with local jobs. A blocking lock that cannot be had waits in the IFS
//! (`lock_wait`): the reply is `NLM4_BLOCKED`, and when the job's poll
//! names the token ([`grant::grant_loop`]) the client is called back
//! with `NLM_GRANTED` (`GRANTED_MSG` when it asked with `LOCK_MSG`); a
//! client that does not take it, or does not answer, has it given back.
//! The `_MSG` forms are answered with no results and their `_RES`
//! message sent to the client's lock manager. `SHARE` and `UNSHARE` are
//! opens with share reservations; `FREE_ALL`, `NLM_SM_NOTIFY` and
//! [`state::NlmState::release_client`] (`RLSIFSLCK RMTLOCNAME`) drop
//! everything a host holds. [`call`] has the calls it makes.

pub mod call;
pub mod conv;
pub mod grant;
mod proc;
pub mod state;

use std::sync::Arc;

use self::proc::Core;
use self::state::{NlmState, Reply, Work};
use crate::rpc::{AcceptError, Answer, CallContext, LocalProgram, decode_args, encode_res};
use crate::server::{Gate, Vfs};
use crate::types::nlm::NLM_PROG;
use crate::xdr::{self, Decoder, Xdr};

pub use proc::status_of;

/// The lock manager for one thread.
pub struct Nlm<V: Vfs> {
    /// The thread's file system.
    pub vfs: V,
    /// The exports.
    pub gate: Gate,
    /// What the threads share.
    pub state: Arc<NlmState>,
}

impl<V: Vfs> std::fmt::Debug for Nlm<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Nlm").finish_non_exhaustive()
    }
}

impl<V: Vfs> Nlm<V> {
    /// The program over `vfs`.
    pub fn new(vfs: V, gate: Gate, state: Arc<NlmState>) -> Self {
        Self { vfs, gate, state }
    }

    fn core<'a>(&'a self, ctx: &'a CallContext) -> Core<'a, V> {
        Core {
            vfs: &self.vfs,
            gate: &self.gate,
            st: &self.state,
            ctx,
        }
    }

    /// Sends `r` as the `_RES` message `procedure` after the reply.
    fn later<T: Xdr>(&self, ctx: &CallContext, procedure: u32, r: &T) {
        if let Ok(b) = xdr::to_bytes(r) {
            self.state
                .later(Work::Res(ctx.peer, ctx.vers, procedure, b));
        }
    }

    fn result(&self, ctx: &CallContext, procedure: u32, r: &Reply) -> Result<(), AcceptError> {
        let c = self.state.client().ok_or(AcceptError::ProcUnavail)?;
        c.result(ctx.peer, procedure, r);
        Ok(())
    }

    fn v4(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        use NLM_PROG::NLM4_VERS::{Args, Res};
        let c = self.core(ctx);
        let res = match decode_args::<Args>(ctx, d)? {
            Args::NLMPROC4_NULL => Res::NLMPROC4_NULL,
            Args::NLMPROC4_TEST(a) => Res::NLMPROC4_TEST(c.test(&a)),
            Args::NLMPROC4_LOCK(a) => Res::NLMPROC4_LOCK(c.lock(&a, true, false)),
            Args::NLMPROC4_CANCEL(a) => Res::NLMPROC4_CANCEL(c.cancel(&a)),
            Args::NLMPROC4_UNLOCK(a) => Res::NLMPROC4_UNLOCK(c.unlock(&a)),
            Args::NLMPROC4_GRANTED(a) => Res::NLMPROC4_GRANTED(c.granted(&a)),
            Args::NLMPROC4_TEST_MSG(a) => {
                self.later(ctx, 11, &c.test(&a));
                Res::NLMPROC4_TEST_MSG
            }
            Args::NLMPROC4_LOCK_MSG(a) => {
                self.later(ctx, 12, &c.lock(&a, true, true));
                Res::NLMPROC4_LOCK_MSG
            }
            Args::NLMPROC4_CANCEL_MSG(a) => {
                self.later(ctx, 13, &c.cancel(&a));
                Res::NLMPROC4_CANCEL_MSG
            }
            Args::NLMPROC4_UNLOCK_MSG(a) => {
                self.later(ctx, 14, &c.unlock(&a));
                Res::NLMPROC4_UNLOCK_MSG
            }
            Args::NLMPROC4_GRANTED_MSG(a) => {
                self.later(ctx, 15, &c.granted(&a));
                Res::NLMPROC4_GRANTED_MSG
            }
            Args::NLMPROC4_TEST_RES(a) => {
                self.result(ctx, 11, &Reply::Test(a))?;
                Res::NLMPROC4_TEST_RES
            }
            Args::NLMPROC4_LOCK_RES(a) => {
                self.result(ctx, 12, &Reply::Res(a))?;
                Res::NLMPROC4_LOCK_RES
            }
            Args::NLMPROC4_CANCEL_RES(a) => {
                self.result(ctx, 13, &Reply::Res(a))?;
                Res::NLMPROC4_CANCEL_RES
            }
            Args::NLMPROC4_UNLOCK_RES(a) => {
                self.result(ctx, 14, &Reply::Res(a))?;
                Res::NLMPROC4_UNLOCK_RES
            }
            Args::NLMPROC4_GRANTED_RES(a) => {
                c.granted_res(&a);
                Res::NLMPROC4_GRANTED_RES
            }
            Args::NLMPROC4_SHARE(a) => Res::NLMPROC4_SHARE(c.share(&a)),
            Args::NLMPROC4_UNSHARE(a) => Res::NLMPROC4_UNSHARE(c.unshare(&a)),
            Args::NLMPROC4_NM_LOCK(a) => Res::NLMPROC4_NM_LOCK(c.lock(&a, false, false)),
            Args::NLMPROC4_FREE_ALL(a) => {
                c.free_all(&a)?;
                Res::NLMPROC4_FREE_ALL
            }
        };
        encode_res(&res)
    }

    fn v1(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        use NLM_PROG::NLM_VERS::{Args, Res};
        use conv::{
            cancargs, lockargs, res_down, res_up, testargs, testres_down, testres_up, unlockargs,
        };
        let c = self.core(ctx);
        let res = match decode_args::<Args>(ctx, d)? {
            Args::NLM_NULL => Res::NLM_NULL,
            Args::NLM_TEST(a) => Res::NLM_TEST(testres_down(&c.test(&testargs(&a)))),
            Args::NLM_LOCK(a) => Res::NLM_LOCK(res_down(&c.lock(&lockargs(&a), true, false))),
            Args::NLM_CANCEL(a) => Res::NLM_CANCEL(res_down(&c.cancel(&cancargs(&a)))),
            Args::NLM_UNLOCK(a) => Res::NLM_UNLOCK(res_down(&c.unlock(&unlockargs(&a)))),
            Args::NLM_GRANTED(a) => Res::NLM_GRANTED(res_down(&c.granted(&testargs(&a)))),
            Args::NLM_TEST_MSG(a) => {
                self.later(ctx, 11, &testres_down(&c.test(&testargs(&a))));
                Res::NLM_TEST_MSG
            }
            Args::NLM_LOCK_MSG(a) => {
                self.later(ctx, 12, &res_down(&c.lock(&lockargs(&a), true, true)));
                Res::NLM_LOCK_MSG
            }
            Args::NLM_CANCEL_MSG(a) => {
                self.later(ctx, 13, &res_down(&c.cancel(&cancargs(&a))));
                Res::NLM_CANCEL_MSG
            }
            Args::NLM_UNLOCK_MSG(a) => {
                self.later(ctx, 14, &res_down(&c.unlock(&unlockargs(&a))));
                Res::NLM_UNLOCK_MSG
            }
            Args::NLM_GRANTED_MSG(a) => {
                self.later(ctx, 15, &res_down(&c.granted(&testargs(&a))));
                Res::NLM_GRANTED_MSG
            }
            Args::NLM_TEST_RES(a) => {
                self.result(ctx, 11, &Reply::Test(testres_up(&a)))?;
                Res::NLM_TEST_RES
            }
            Args::NLM_LOCK_RES(a) => {
                self.result(ctx, 12, &Reply::Res(res_up(&a)))?;
                Res::NLM_LOCK_RES
            }
            Args::NLM_CANCEL_RES(a) => {
                self.result(ctx, 13, &Reply::Res(res_up(&a)))?;
                Res::NLM_CANCEL_RES
            }
            Args::NLM_UNLOCK_RES(a) => {
                self.result(ctx, 14, &Reply::Res(res_up(&a)))?;
                Res::NLM_UNLOCK_RES
            }
            Args::NLM_GRANTED_RES(a) => {
                c.granted_res(&res_up(&a));
                Res::NLM_GRANTED_RES
            }
        };
        encode_res(&res)
    }

    fn v3(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        use NLM_PROG::NLM_VERSX::{Args, Res};
        use conv::{
            cancargs, lockargs, notify, res_down, res_up, shareargs, shareres_down, testargs,
            testres_down, testres_up, unlockargs,
        };
        let c = self.core(ctx);
        let res = match decode_args::<Args>(ctx, d)? {
            Args::NLM_NULL => Res::NLM_NULL,
            Args::NLM_TEST(a) => Res::NLM_TEST(testres_down(&c.test(&testargs(&a)))),
            Args::NLM_LOCK(a) => Res::NLM_LOCK(res_down(&c.lock(&lockargs(&a), true, false))),
            Args::NLM_CANCEL(a) => Res::NLM_CANCEL(res_down(&c.cancel(&cancargs(&a)))),
            Args::NLM_UNLOCK(a) => Res::NLM_UNLOCK(res_down(&c.unlock(&unlockargs(&a)))),
            Args::NLM_GRANTED(a) => Res::NLM_GRANTED(res_down(&c.granted(&testargs(&a)))),
            Args::NLM_TEST_MSG(a) => {
                self.later(ctx, 11, &testres_down(&c.test(&testargs(&a))));
                Res::NLM_TEST_MSG
            }
            Args::NLM_LOCK_MSG(a) => {
                self.later(ctx, 12, &res_down(&c.lock(&lockargs(&a), true, true)));
                Res::NLM_LOCK_MSG
            }
            Args::NLM_CANCEL_MSG(a) => {
                self.later(ctx, 13, &res_down(&c.cancel(&cancargs(&a))));
                Res::NLM_CANCEL_MSG
            }
            Args::NLM_UNLOCK_MSG(a) => {
                self.later(ctx, 14, &res_down(&c.unlock(&unlockargs(&a))));
                Res::NLM_UNLOCK_MSG
            }
            Args::NLM_GRANTED_MSG(a) => {
                self.later(ctx, 15, &res_down(&c.granted(&testargs(&a))));
                Res::NLM_GRANTED_MSG
            }
            Args::NLM_TEST_RES(a) => {
                self.result(ctx, 11, &Reply::Test(testres_up(&a)))?;
                Res::NLM_TEST_RES
            }
            Args::NLM_LOCK_RES(a) => {
                self.result(ctx, 12, &Reply::Res(res_up(&a)))?;
                Res::NLM_LOCK_RES
            }
            Args::NLM_CANCEL_RES(a) => {
                self.result(ctx, 13, &Reply::Res(res_up(&a)))?;
                Res::NLM_CANCEL_RES
            }
            Args::NLM_UNLOCK_RES(a) => {
                self.result(ctx, 14, &Reply::Res(res_up(&a)))?;
                Res::NLM_UNLOCK_RES
            }
            Args::NLM_GRANTED_RES(a) => {
                c.granted_res(&res_up(&a));
                Res::NLM_GRANTED_RES
            }
            Args::NLM_SHARE(a) => Res::NLM_SHARE(shareres_down(&c.share(&shareargs(&a)))),
            Args::NLM_UNSHARE(a) => Res::NLM_UNSHARE(shareres_down(&c.unshare(&shareargs(&a)))),
            Args::NLM_NM_LOCK(a) => {
                Res::NLM_NM_LOCK(res_down(&c.lock(&lockargs(&a), false, false)))
            }
            Args::NLM_FREE_ALL(a) => {
                c.free_all(&notify(&a))?;
                Res::NLM_FREE_ALL
            }
        };
        encode_res(&res)
    }

    fn sm(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        use NLM_PROG::NLM_SM::{Args, Res};
        if !crate::nsm::local(ctx.peer.ip()) {
            return Err(AcceptError::ProcUnavail);
        }
        let Args::NLM_SM_NOTIFY(s) = decode_args::<Args>(ctx, d)?;
        self.state.release_client(&self.vfs, &s.mon_name);
        if let Some(c) = self.state.client() {
            c.restarted(&s.mon_name, s.state, s.r#priv);
        }
        encode_res(&Res::NLM_SM_NOTIFY)
    }
}

impl<V: Vfs> LocalProgram for Nlm<V> {
    fn program(&self) -> u32 {
        NLM_PROG::NUMBER
    }
    fn versions(&self) -> &[u32] {
        &[1, 2, 3, 4]
    }
    fn call(&self, ctx: &CallContext, d: &mut Decoder<'_>) -> Answer {
        match ctx.vers {
            1 => self.v1(ctx, d),
            2 => self.sm(ctx, d),
            3 => self.v3(ctx, d),
            4 => self.v4(ctx, d),
            _ => Err(AcceptError::ProcUnavail),
        }
    }
}
