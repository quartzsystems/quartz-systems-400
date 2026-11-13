// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Versions 1 and 3 as version 4: 32-bit offsets widened on the way
//! in, clamped on the way out, the version 4 statuses that versions 1
//! and 3 lack sent as `nlm_denied_nolocks`.

use super::state::Lock;
use crate::types::nlm::{
    netobj, nlm_cancargs, nlm_holder, nlm_lock, nlm_lockargs, nlm_notify, nlm_res, nlm_shareargs,
    nlm_shareres, nlm_stat, nlm_stats, nlm_testargs, nlm_testres, nlm_testrply, nlm_unlockargs,
    nlm4_cancargs, nlm4_holder, nlm4_lock, nlm4_lockargs, nlm4_notify, nlm4_res, nlm4_share,
    nlm4_shareargs, nlm4_shareres, nlm4_stat, nlm4_stats, nlm4_testargs, nlm4_testres,
    nlm4_testrply, nlm4_unlockargs,
};
use crate::xdr;

/// A version 4 lock as [`Lock`].
#[must_use]
pub fn lock_of(l: &nlm4_lock) -> Lock {
    Lock {
        caller_name: l.caller_name.clone(),
        fh: l.fh.0.clone(),
        oh: l.oh.0.clone(),
        svid: l.svid,
        offset: l.l_offset,
        len: l.l_len,
    }
}

/// A [`Lock`] as version 4's.
#[must_use]
pub fn lock4(l: &Lock) -> nlm4_lock {
    nlm4_lock {
        caller_name: l.caller_name.clone(),
        fh: netobj(l.fh.clone()),
        oh: netobj(l.oh.clone()),
        svid: l.svid,
        l_offset: l.offset,
        l_len: l.len,
    }
}

fn clamp(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn up(l: &nlm_lock) -> nlm4_lock {
    nlm4_lock {
        caller_name: l.caller_name.clone(),
        fh: l.fh.clone(),
        oh: l.oh.clone(),
        svid: l.svid,
        l_offset: u64::from(l.l_offset),
        l_len: u64::from(l.l_len),
    }
}

fn down(l: &nlm4_lock) -> nlm_lock {
    nlm_lock {
        caller_name: l.caller_name.clone(),
        fh: l.fh.clone(),
        oh: l.oh.clone(),
        svid: l.svid,
        l_offset: clamp(l.l_offset),
        l_len: clamp(l.l_len),
    }
}

/// Version 1 and 3 `nlm_testargs`.
#[must_use]
pub fn testargs(a: &nlm_testargs) -> nlm4_testargs {
    nlm4_testargs {
        cookie: a.cookie.clone(),
        exclusive: a.exclusive,
        alock: up(&a.alock),
    }
}

/// Version 1 and 3 `nlm_lockargs`.
#[must_use]
pub fn lockargs(a: &nlm_lockargs) -> nlm4_lockargs {
    nlm4_lockargs {
        cookie: a.cookie.clone(),
        block: a.block,
        exclusive: a.exclusive,
        alock: up(&a.alock),
        reclaim: a.reclaim,
        state: a.state,
    }
}

/// Version 1 and 3 `nlm_cancargs`.
#[must_use]
pub fn cancargs(a: &nlm_cancargs) -> nlm4_cancargs {
    nlm4_cancargs {
        cookie: a.cookie.clone(),
        block: a.block,
        exclusive: a.exclusive,
        alock: up(&a.alock),
    }
}

/// Version 1 and 3 `nlm_unlockargs`.
#[must_use]
pub fn unlockargs(a: &nlm_unlockargs) -> nlm4_unlockargs {
    nlm4_unlockargs {
        cookie: a.cookie.clone(),
        alock: up(&a.alock),
    }
}

/// Version 3 `nlm_shareargs`.
#[must_use]
pub fn shareargs(a: &nlm_shareargs) -> nlm4_shareargs {
    nlm4_shareargs {
        cookie: a.cookie.clone(),
        share: nlm4_share {
            caller_name: a.share.caller_name.clone(),
            fh: a.share.fh.clone(),
            oh: a.share.oh.clone(),
            mode: a.share.mode,
            access: a.share.access,
        },
        reclaim: a.reclaim,
    }
}

/// Version 3 `nlm_notify`.
#[must_use]
pub fn notify(a: &nlm_notify) -> nlm4_notify {
    nlm4_notify {
        name: a.name.clone(),
        state: a.state,
    }
}

/// A version 4 status as versions 1 and 3 have it.
#[must_use]
pub fn stat_down(s: nlm4_stats) -> nlm_stats {
    match s {
        nlm4_stats::NLM4_GRANTED => nlm_stats::nlm_granted,
        nlm4_stats::NLM4_DENIED => nlm_stats::nlm_denied,
        nlm4_stats::NLM4_BLOCKED => nlm_stats::nlm_blocked,
        nlm4_stats::NLM4_DENIED_GRACE_PERIOD => nlm_stats::nlm_denied_grace_period,
        nlm4_stats::NLM4_DEADLCK => nlm_stats::nlm_deadlck,
        nlm4_stats::NLM4_DENIED_NOLOCKS
        | nlm4_stats::NLM4_ROFS
        | nlm4_stats::NLM4_STALE_FH
        | nlm4_stats::NLM4_FBIG
        | nlm4_stats::NLM4_FAILED => nlm_stats::nlm_denied_nolocks,
    }
}

/// A versions 1 and 3 status as version 4's.
#[must_use]
pub fn stat_up(s: nlm_stats) -> nlm4_stats {
    match s {
        nlm_stats::nlm_granted => nlm4_stats::NLM4_GRANTED,
        nlm_stats::nlm_denied => nlm4_stats::NLM4_DENIED,
        nlm_stats::nlm_denied_nolocks => nlm4_stats::NLM4_DENIED_NOLOCKS,
        nlm_stats::nlm_blocked => nlm4_stats::NLM4_BLOCKED,
        nlm_stats::nlm_denied_grace_period => nlm4_stats::NLM4_DENIED_GRACE_PERIOD,
        nlm_stats::nlm_deadlck => nlm4_stats::NLM4_DEADLCK,
    }
}

/// `nlm4_res` down.
#[must_use]
pub fn res_down(r: &nlm4_res) -> nlm_res {
    nlm_res {
        cookie: r.cookie.clone(),
        stat: nlm_stat {
            stat: stat_down(r.stat.stat),
        },
    }
}

/// `nlm_res` up.
#[must_use]
pub fn res_up(r: &nlm_res) -> nlm4_res {
    nlm4_res {
        cookie: r.cookie.clone(),
        stat: nlm4_stat {
            stat: stat_up(r.stat.stat),
        },
    }
}

/// `nlm4_testres` down.
#[must_use]
pub fn testres_down(r: &nlm4_testres) -> nlm_testres {
    nlm_testres {
        cookie: r.cookie.clone(),
        test_stat: match &r.test_stat {
            nlm4_testrply::NLM4_DENIED(h) => nlm_testrply::nlm_denied(nlm_holder {
                exclusive: h.exclusive,
                svid: h.svid,
                oh: h.oh.clone(),
                l_offset: clamp(h.l_offset),
                l_len: clamp(h.l_len),
            }),
            nlm4_testrply::default(s) => nlm_testrply::default(stat_down(*s)),
        },
    }
}

/// `nlm_testres` up.
#[must_use]
pub fn testres_up(r: &nlm_testres) -> nlm4_testres {
    nlm4_testres {
        cookie: r.cookie.clone(),
        test_stat: match &r.test_stat {
            nlm_testrply::nlm_denied(h) => nlm4_testrply::NLM4_DENIED(nlm4_holder {
                exclusive: h.exclusive,
                svid: h.svid,
                oh: h.oh.clone(),
                l_offset: u64::from(h.l_offset),
                l_len: u64::from(h.l_len),
            }),
            nlm_testrply::default(s) => nlm4_testrply::default(stat_up(*s)),
        },
    }
}

/// `nlm4_shareres` down.
#[must_use]
pub fn shareres_down(r: &nlm4_shareres) -> nlm_shareres {
    nlm_shareres {
        cookie: r.cookie.clone(),
        stat: stat_down(r.stat),
        sequence: r.sequence,
    }
}

/// The encoded `nlm_testargs` (versions 1, 3) or `nlm4_testargs` of a grant.
///
/// # Errors
///
/// A bound the lock breaks.
pub fn testargs_bytes(vers: u32, cookie: &[u8], exclusive: bool, l: &Lock) -> xdr::Result<Vec<u8>> {
    let a = nlm4_testargs {
        cookie: netobj(cookie.to_vec()),
        exclusive,
        alock: lock4(l),
    };
    if vers == 4 {
        xdr::to_bytes(&a)
    } else {
        xdr::to_bytes(&nlm_testargs {
            cookie: a.cookie,
            exclusive,
            alock: down(&a.alock),
        })
    }
}
