// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The procedures, in version 4's terms (versions 1 and 3 are converted
//! by [`super::conv`]).
//!
//! In the grace period only reclaims lock or share
//! (`NLM4_DENIED_GRACE_PERIOD`); `TEST` is refused the same way;
//! `UNLOCK` and `CANCEL` are taken. A lock on a handle that is not good
//! is `NLM4_STALE_FH`; an exclusive lock through an export the client
//! may not write is `NLM4_ROFS`. The first lock of a host, unless it is
//! `NM_LOCK`, asks the local status monitor to watch the host
//! (`NLM4_DENIED_NOLOCKS` when it will not).

#![allow(clippy::many_single_char_names)]
use super::call;
use super::conv::lock_of;
use super::state::{Blocked, NlmState, Share, kind};
use crate::rpc::CallContext;
use crate::server::gate::writable;
use crate::server::vfs::{LockOutcome, ShareAccess, ShareDeny};
use crate::server::{FileHandle, Gate, Req, Vfs, VfsError};
use crate::types::nlm::{
    fsh_access, fsh_mode, netobj, nlm4_cancargs, nlm4_holder, nlm4_lockargs, nlm4_notify, nlm4_res,
    nlm4_shareargs, nlm4_shareres, nlm4_stat, nlm4_stats, nlm4_testargs, nlm4_testres,
    nlm4_testrply, nlm4_unlockargs,
};

/// The status of an error.
#[must_use]
pub fn status_of(e: VfsError) -> nlm4_stats {
    match e {
        VfsError::Again | VfsError::Busy => nlm4_stats::NLM4_DENIED,
        VfsError::Deadlock => nlm4_stats::NLM4_DEADLCK,
        VfsError::Stale | VfsError::BadHandle => nlm4_stats::NLM4_STALE_FH,
        VfsError::RoFs => nlm4_stats::NLM4_ROFS,
        VfsError::FBig => nlm4_stats::NLM4_FBIG,
        VfsError::NoLocks => nlm4_stats::NLM4_DENIED_NOLOCKS,
        _ => nlm4_stats::NLM4_FAILED,
    }
}

pub(super) fn res(cookie: &netobj, s: nlm4_stats) -> nlm4_res {
    nlm4_res {
        cookie: cookie.clone(),
        stat: nlm4_stat { stat: s },
    }
}

/// One call's context.
pub(super) struct Core<'a, V: Vfs> {
    pub vfs: &'a V,
    pub gate: &'a Gate,
    pub st: &'a NlmState,
    pub ctx: &'a CallContext,
}

impl<V: Vfs> Core<'_, V> {
    fn open(&self, fh: &[u8]) -> Result<(Req, FileHandle), nlm4_stats> {
        self.gate.open(self.ctx, fh, 3).map_err(status_of)
    }

    pub(super) fn test(&self, a: &nlm4_testargs) -> nlm4_testres {
        let out = |s| nlm4_testres {
            cookie: a.cookie.clone(),
            test_stat: nlm4_testrply::default(s),
        };
        if self.st.in_grace() {
            return out(nlm4_stats::NLM4_DENIED_GRACE_PERIOD);
        }
        let l = lock_of(&a.alock);
        let (r, h) = match self.open(&l.fh) {
            Ok(v) => v,
            Err(s) => return out(s),
        };
        let sub = self.st.sub_of(&l);
        match self
            .vfs
            .test_lock(&r, &h, sub, kind(a.exclusive), l.offset, l.len)
        {
            Ok(None) => out(nlm4_stats::NLM4_GRANTED),
            Ok(Some(c)) => {
                let (svid, oh) = self.st.owner_of(c.sub).unwrap_or_else(|| {
                    (
                        i32::from_ne_bytes(
                            u32::try_from(c.owner & 0x7fff_ffff)
                                .unwrap_or(0)
                                .to_ne_bytes(),
                        ),
                        Vec::new(),
                    )
                });
                nlm4_testres {
                    cookie: a.cookie.clone(),
                    test_stat: nlm4_testrply::NLM4_DENIED(nlm4_holder {
                        exclusive: c.kind == crate::server::LockKind::Write,
                        svid,
                        oh: netobj(oh),
                        l_offset: c.start,
                        l_len: c.len,
                    }),
                }
            }
            Err(e) => out(status_of(e)),
        }
    }

    pub(super) fn lock(&self, a: &nlm4_lockargs, monitor: bool, msg: bool) -> nlm4_res {
        let out = |s| res(&a.cookie, s);
        if self.st.in_grace() && !a.reclaim {
            return out(nlm4_stats::NLM4_DENIED_GRACE_PERIOD);
        }
        let l = lock_of(&a.alock);
        let (r, h) = match self.open(&l.fh) {
            Ok(v) => v,
            Err(s) => return out(s),
        };
        if a.exclusive && !writable(&r) {
            return out(nlm4_stats::NLM4_ROFS);
        }
        if monitor && self.st.cfg.monitor && self.st.needs_monitor(&l.caller_name) {
            if !call::monitor(self.st, &l.caller_name) {
                return out(nlm4_stats::NLM4_DENIED_NOLOCKS);
            }
            self.st.monitored(&l.caller_name);
        }
        let sub = self.st.sub_of(&l);
        let block = a.block && monitor;
        if block && self.st.waiting(sub, &l, a.exclusive).is_some() {
            return out(nlm4_stats::NLM4_BLOCKED);
        }
        match self
            .vfs
            .lock(&r, &h, sub, kind(a.exclusive), l.offset, l.len, block)
        {
            Ok(LockOutcome::Granted) => out(nlm4_stats::NLM4_GRANTED),
            Ok(LockOutcome::Denied(_)) => out(nlm4_stats::NLM4_DENIED),
            Ok(LockOutcome::Queued(t)) => {
                self.st.block(
                    t,
                    Blocked {
                        sub,
                        lock: l,
                        exclusive: a.exclusive,
                        vers: self.ctx.vers,
                        msg,
                        peer: self.ctx.peer,
                        req: r,
                        handle: h,
                        cookie: a.cookie.0.clone(),
                    },
                );
                out(nlm4_stats::NLM4_BLOCKED)
            }
            Err(e) => out(status_of(e)),
        }
    }

    pub(super) fn cancel(&self, a: &nlm4_cancargs) -> nlm4_res {
        let out = |s| res(&a.cookie, s);
        let l = lock_of(&a.alock);
        let (r, h) = match self.open(&l.fh) {
            Ok(v) => v,
            Err(s) => return out(s),
        };
        let sub = self.st.sub_of(&l);
        let Some(t) = self.st.waiting(sub, &l, a.exclusive) else {
            return out(nlm4_stats::NLM4_DENIED);
        };
        self.st.unblock(t);
        match self.vfs.cancel_wait(t) {
            Ok(true) => {}
            // Granted meanwhile: the lock is held, and given back.
            Ok(false) => {
                let _ = self.vfs.unlock(&r, &h, sub, l.offset, l.len);
            }
            Err(e) => return out(status_of(e)),
        }
        out(nlm4_stats::NLM4_GRANTED)
    }

    pub(super) fn unlock(&self, a: &nlm4_unlockargs) -> nlm4_res {
        let out = |s| res(&a.cookie, s);
        let l = lock_of(&a.alock);
        let (r, h) = match self.open(&l.fh) {
            Ok(v) => v,
            Err(s) => return out(s),
        };
        let sub = self.st.sub_of(&l);
        match self.vfs.unlock(&r, &h, sub, l.offset, l.len) {
            Ok(()) => out(nlm4_stats::NLM4_GRANTED),
            Err(e) => out(status_of(e)),
        }
    }

    /// `NLM_GRANTED` to this host, as a lock client.
    pub(super) fn granted(&self, a: &nlm4_testargs) -> nlm4_res {
        let ok = self
            .st
            .client()
            .is_some_and(|c| c.granted(self.ctx.peer, a.exclusive, &lock_of(&a.alock)));
        res(
            &a.cookie,
            if ok {
                nlm4_stats::NLM4_GRANTED
            } else {
                nlm4_stats::NLM4_DENIED
            },
        )
    }

    /// A client's answer to our `GRANTED_MSG`: a lock it does not want
    /// is given back.
    pub(super) fn granted_res(&self, a: &nlm4_res) {
        let Some(g) = self.st.grant_answered(&a.cookie.0) else {
            return;
        };
        if a.stat.stat != nlm4_stats::NLM4_GRANTED {
            let b = g.blocked;
            let _ = self
                .vfs
                .unlock(&b.req, &b.handle, b.sub, b.lock.offset, b.lock.len);
        }
    }

    pub(super) fn share(&self, a: &nlm4_shareargs) -> nlm4_shareres {
        let out = |s| nlm4_shareres {
            cookie: a.cookie.clone(),
            stat: s,
            sequence: 0,
        };
        if self.st.in_grace() && !a.reclaim {
            return out(nlm4_stats::NLM4_DENIED_GRACE_PERIOD);
        }
        let s = &a.share;
        let (r, h) = match self.open(&s.fh.0) {
            Ok(v) => v,
            Err(st) => return out(st),
        };
        let access = match s.access {
            fsh_access::fsa_NONE | fsh_access::fsa_R => ShareAccess::Read,
            fsh_access::fsa_W => ShareAccess::Write,
            fsh_access::fsa_RW => ShareAccess::Both,
        };
        if access != ShareAccess::Read && !writable(&r) {
            return out(nlm4_stats::NLM4_ROFS);
        }
        let deny = match s.mode {
            fsh_mode::fsm_DN => ShareDeny::None,
            fsh_mode::fsm_DR => ShareDeny::Read,
            fsh_mode::fsm_DW => ShareDeny::Write,
            fsh_mode::fsm_DRW => ShareDeny::Both,
        };
        if let Some(old) = self.st.take_share(&s.caller_name, &s.oh.0, &s.fh.0) {
            let _ = self.vfs.close(old.open);
        }
        match self.vfs.open(&r, &h, access, deny) {
            Ok(open) => {
                self.st
                    .put_share(&s.caller_name, &s.oh.0, &s.fh.0, Share { open });
                out(nlm4_stats::NLM4_GRANTED)
            }
            Err(e) => out(status_of(e)),
        }
    }

    pub(super) fn unshare(&self, a: &nlm4_shareargs) -> nlm4_shareres {
        let s = &a.share;
        if let Some(old) = self.st.take_share(&s.caller_name, &s.oh.0, &s.fh.0) {
            let _ = self.vfs.close(old.open);
        }
        nlm4_shareres {
            cookie: a.cookie.clone(),
            stat: nlm4_stats::NLM4_GRANTED,
            sequence: 0,
        }
    }

    /// `FREE_ALL`: taken from the host itself (its `AUTH_SYS` machine
    /// name or its address is the name) or from this host.
    pub(super) fn free_all(&self, a: &nlm4_notify) {
        let peer = self.ctx.peer.ip();
        let mine = match &self.ctx.cred {
            crate::rpc::Credential::Sys(p) => p.machinename.eq_ignore_ascii_case(&a.name),
            crate::rpc::Credential::None => false,
        };
        if mine || crate::nsm::local(peer) || a.name == peer.to_string().as_bytes() {
            self.st.release_client(self.vfs, &a.name);
        } else {
            tracing::warn!("FREE_ALL from {peer} for another host refused");
        }
    }
}
