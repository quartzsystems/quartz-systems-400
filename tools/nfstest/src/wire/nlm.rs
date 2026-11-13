// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NLM version 4 (the Open Group's XNFS, as RFC 1813 appendix II repeats it)
//! and NSM version 1, by hand.

use super::xdr::{R, W};
use crate::Error;

/// NLM procedures (version 4).
pub mod proc4 {
    /// `NLM4_TEST`.
    pub const TEST: u32 = 1;
    /// `NLM4_LOCK`.
    pub const LOCK: u32 = 2;
    /// `NLM4_CANCEL`.
    pub const CANCEL: u32 = 3;
    /// `NLM4_UNLOCK`.
    pub const UNLOCK: u32 = 4;
    /// `NLM4_GRANTED`.
    pub const GRANTED: u32 = 5;
    /// `NLM4_SHARE`.
    pub const SHARE: u32 = 20;
    /// `NLM4_UNSHARE`.
    pub const UNSHARE: u32 = 21;
    /// `NLM4_FREE_ALL`.
    pub const FREE_ALL: u32 = 23;
}

/// `nlm4_stats` values.
pub mod stat {
    /// `NLM4_GRANTED`.
    pub const GRANTED: u32 = 0;
    /// `NLM4_DENIED`.
    pub const DENIED: u32 = 1;
    /// `NLM4_DENIED_NOLOCKS`.
    pub const DENIED_NOLOCKS: u32 = 2;
    /// `NLM4_BLOCKED`.
    pub const BLOCKED: u32 = 3;
    /// `NLM4_DENIED_GRACE_PERIOD`.
    pub const DENIED_GRACE_PERIOD: u32 = 4;
}

/// The name of an `nlm4_stats` value.
pub fn stat_name(s: u32) -> String {
    let n = [
        "NLM4_GRANTED",
        "NLM4_DENIED",
        "NLM4_DENIED_NOLOCKS",
        "NLM4_BLOCKED",
        "NLM4_DENIED_GRACE_PERIOD",
        "NLM4_DEADLCK",
        "NLM4_ROFS",
        "NLM4_STALE_FH",
        "NLM4_FBIG",
        "NLM4_FAILED",
    ];
    n.get(s as usize)
        .map_or_else(|| format!("nlm4_stats {s}"), |x| (*x).to_string())
}

/// `nlm4_lock`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lock {
    /// `caller_name`.
    pub caller: Vec<u8>,
    /// `fh`: the NFS version 3 handle.
    pub fh: Vec<u8>,
    /// `oh`: the owner handle.
    pub oh: Vec<u8>,
    /// `svid`: the process.
    pub svid: i32,
    /// `l_offset`.
    pub offset: u64,
    /// `l_len` (0: to the end).
    pub len: u64,
}

impl Lock {
    fn put(&self, w: &mut W) {
        w.opaque(&self.caller)
            .opaque(&self.fh)
            .opaque(&self.oh)
            .i32(self.svid)
            .u64(self.offset)
            .u64(self.len);
    }

    /// Decodes one.
    pub fn get(r: &mut R<'_>) -> Result<Lock, Error> {
        Ok(Lock {
            caller: r.opaque(1024)?.to_vec(),
            fh: r.opaque(1024)?.to_vec(),
            oh: r.opaque(1024)?.to_vec(),
            svid: r.i32()?,
            offset: r.u64()?,
            len: r.u64()?,
        })
    }
}

/// `nlm4_testargs` (`TEST`, `GRANTED`).
pub fn test_args(cookie: &[u8], exclusive: bool, l: &Lock) -> Vec<u8> {
    let mut w = W::new();
    w.opaque(cookie).bool(exclusive);
    l.put(&mut w);
    w.done()
}

/// `nlm4_lockargs`.
pub fn lock_args(
    cookie: &[u8],
    block: bool,
    exclusive: bool,
    l: &Lock,
    reclaim: bool,
    state: i32,
) -> Vec<u8> {
    let mut w = W::new();
    w.opaque(cookie).bool(block).bool(exclusive);
    l.put(&mut w);
    w.bool(reclaim).i32(state);
    w.done()
}

/// `nlm4_cancargs`.
pub fn cancel_args(cookie: &[u8], block: bool, exclusive: bool, l: &Lock) -> Vec<u8> {
    let mut w = W::new();
    w.opaque(cookie).bool(block).bool(exclusive);
    l.put(&mut w);
    w.done()
}

/// `nlm4_unlockargs`.
pub fn unlock_args(cookie: &[u8], l: &Lock) -> Vec<u8> {
    let mut w = W::new();
    w.opaque(cookie);
    l.put(&mut w);
    w.done()
}

/// `nlm4_shareargs`: mode `fsh4_mode`, access `fsh4_access`.
pub fn share_args(
    cookie: &[u8],
    caller: &[u8],
    fh: &[u8],
    oh: &[u8],
    mode: u32,
    access: u32,
    reclaim: bool,
) -> Vec<u8> {
    W::new()
        .opaque(cookie)
        .opaque(caller)
        .opaque(fh)
        .opaque(oh)
        .u32(mode)
        .u32(access)
        .bool(reclaim)
        .done()
}

/// `nlm4_notify` (`FREE_ALL`).
pub fn notify_args(name: &[u8], state: i32) -> Vec<u8> {
    W::new().opaque(name).i32(state).done()
}

/// `nlm4_res`: the cookie and the status.
pub fn res(b: &[u8]) -> Result<(Vec<u8>, u32), Error> {
    let mut r = R::new(b);
    let cookie = r.opaque(1024)?.to_vec();
    let s = r.u32()?;
    r.end()?;
    Ok((cookie, s))
}

/// The holder of a conflicting lock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holder {
    /// `exclusive`.
    pub exclusive: bool,
    /// `svid`.
    pub svid: i32,
    /// `oh`.
    pub oh: Vec<u8>,
    /// `l_offset`.
    pub offset: u64,
    /// `l_len`.
    pub len: u64,
}

/// `nlm4_testres`: the status and, when denied, the holder.
pub fn test_res(b: &[u8]) -> Result<(u32, Option<Holder>), Error> {
    let mut r = R::new(b);
    r.opaque(1024)?;
    let s = r.u32()?;
    let h = if s == stat::DENIED {
        Some(Holder {
            exclusive: r.bool()?,
            svid: r.i32()?,
            oh: r.opaque(1024)?.to_vec(),
            offset: r.u64()?,
            len: r.u64()?,
        })
    } else {
        None
    };
    r.end()?;
    Ok((s, h))
}

/// `nlm4_shareres`: the status and the sequence.
pub fn share_res(b: &[u8]) -> Result<(u32, i32), Error> {
    let mut r = R::new(b);
    r.opaque(1024)?;
    let s = r.u32()?;
    let seq = r.i32()?;
    r.end()?;
    Ok((s, seq))
}

/// NSM `sm_name`.
pub fn sm_name(name: &[u8]) -> Vec<u8> {
    W::new().opaque(name).done()
}

/// NSM `sm_stat_res`: `res` (0 `stat_succ`) and the state.
pub fn sm_stat_res(b: &[u8]) -> Result<(u32, i32), Error> {
    let mut r = R::new(b);
    let res = r.u32()?;
    let state = r.i32()?;
    r.end()?;
    Ok((res, state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_args_layout() {
        let l = Lock {
            caller: b"c".to_vec(),
            fh: vec![9; 4],
            oh: b"oh".to_vec(),
            svid: -1,
            offset: 1,
            len: 2,
        };
        let b = lock_args(b"k", true, false, &l, false, 3);
        // cookie(8) block exclusive (8) caller(8) fh(8) oh(8) svid(4) off(8) len(8) reclaim state(8)
        assert_eq!(b.len(), 8 + 8 + 8 + 8 + 8 + 4 + 8 + 8 + 8);
        assert_eq!(&b[0..4], &1u32.to_be_bytes());
        assert_eq!(&b[b.len() - 4..], &3u32.to_be_bytes());
        let mut r = R::new(&b[16..]);
        assert_eq!(Lock::get(&mut r).unwrap(), l);
    }

    #[test]
    fn test_res_with_holder() {
        let b = W::new()
            .opaque(b"k")
            .u32(stat::DENIED)
            .bool(true)
            .i32(7)
            .opaque(b"o")
            .u64(0)
            .u64(10)
            .done();
        let (s, h) = test_res(&b).unwrap();
        assert_eq!(s, stat::DENIED);
        assert_eq!(h.unwrap().svid, 7);
    }
}
