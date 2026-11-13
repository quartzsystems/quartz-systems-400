// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NLM version 4 and NSM, over the hand-written wire in `wire::nlm`.

use std::time::Duration;

use super::{Family, Lines, Scenario, line};
use crate::Error;
use crate::callback::Callbacks;
use crate::ctx::{Ctx, prog};
use crate::error::{check, fail};
use crate::rpc::{Client, Cred};
use crate::v3::Session;
use crate::wire::nlm::{self, Lock, proc4, stat, stat_name};

/// This file's scenarios.
pub const LIST: [Scenario; 8] = [
    s("nlm-test-lock-unlock", test_lock_unlock),
    s("nlm-conflict", conflict),
    s("nlm-blocked-granted", blocked_granted),
    s("nlm-cancel", cancel),
    s("nlm-free-all", free_all),
    s("nlm-share", share),
    s("nlm-grace", grace),
    s("nsm-stat", nsm_stat),
];

const fn s(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario {
        name,
        family: Family::V3,
        run,
    }
}

/// One lock owner: a host name and a process.
struct Owner {
    c: Client,
    caller: Vec<u8>,
    svid: i32,
}

impl Owner {
    fn new(c: &Ctx, caller: &str, svid: i32) -> Result<Owner, Error> {
        let cred = Cred::Sys {
            machine: caller.as_bytes().to_vec(),
            uid: c.t.uid,
            gid: c.t.gid,
            gids: vec![c.t.gid],
        };
        Ok(Owner {
            c: c.client_as(c.nlm_addr()?, c.t.proto, cred),
            caller: caller.as_bytes().to_vec(),
            svid,
        })
    }

    fn lock_of(&self, fh: &[u8], offset: u64, len: u64) -> Lock {
        Lock {
            caller: self.caller.clone(),
            fh: fh.to_vec(),
            oh: format!("{}:{}", String::from_utf8_lossy(&self.caller), self.svid).into_bytes(),
            svid: self.svid,
            offset,
            len,
        }
    }

    fn res(&mut self, proc: u32, args: &[u8]) -> Result<u32, Error> {
        let b = self.c.call_raw(prog::NLM, 4, proc, args)?;
        let (cookie, s) = nlm::res(&b)?;
        check(cookie == b"nfstest", || {
            "the reply's cookie is not the call's".into()
        })?;
        Ok(s)
    }

    fn test(
        &mut self,
        fh: &[u8],
        ex: bool,
        off: u64,
        len: u64,
    ) -> Result<(u32, Option<nlm::Holder>), Error> {
        let l = self.lock_of(fh, off, len);
        let b = self.c.call_raw(
            prog::NLM,
            4,
            proc4::TEST,
            &nlm::test_args(b"nfstest", ex, &l),
        )?;
        nlm::test_res(&b)
    }

    fn lock(
        &mut self,
        fh: &[u8],
        block: bool,
        ex: bool,
        off: u64,
        len: u64,
        reclaim: bool,
    ) -> Result<u32, Error> {
        let l = self.lock_of(fh, off, len);
        self.res(
            proc4::LOCK,
            &nlm::lock_args(b"nfstest", block, ex, &l, reclaim, 1),
        )
    }

    fn unlock(&mut self, fh: &[u8], off: u64, len: u64) -> Result<u32, Error> {
        let l = self.lock_of(fh, off, len);
        self.res(proc4::UNLOCK, &nlm::unlock_args(b"nfstest", &l))
    }

    fn cancel(&mut self, fh: &[u8], ex: bool, off: u64, len: u64) -> Result<u32, Error> {
        let l = self.lock_of(fh, off, len);
        self.res(proc4::CANCEL, &nlm::cancel_args(b"nfstest", true, ex, &l))
    }

    fn share(&mut self, fh: &[u8], mode: u32, access: u32, un: bool) -> Result<u32, Error> {
        let oh = format!("share:{}", self.svid).into_bytes();
        let args = nlm::share_args(b"nfstest", &self.caller, fh, &oh, mode, access, false);
        let p = if un { proc4::UNSHARE } else { proc4::SHARE };
        let b = self.c.call_raw(prog::NLM, 4, p, &args)?;
        Ok(nlm::share_res(&b)?.0)
    }
}

fn want(got: u32, want: u32, what: &str) -> Result<(), Error> {
    if got == stat::DENIED_GRACE_PERIOD && want != got {
        return fail(format!(
            "{what}: NLM4_DENIED_GRACE_PERIOD (the server is in its grace period: wait, or run nlm-grace with --expect-grace)"
        ));
    }
    check(got == want, || {
        format!(
            "{what}: expected {}, got {}",
            stat_name(want),
            stat_name(got)
        )
    })
}

/// A file to lock in a session.
fn file(c: &Ctx, name: &str) -> Result<(Session, Vec<u8>), Error> {
    let mut s = Session::open(c, name)?;
    let d = s.dir.clone();
    let f = s.n.create(&d, b"locked", 0o666)?;
    s.n.write(&f, 0, &[0; 1000])?;
    Ok((s, f))
}

fn test_lock_unlock(c: &Ctx) -> Lines {
    let (s, f) = file(c, "nlm-basic")?;
    let mut a = Owner::new(c, "nfstest", 1)?;
    let mut b = Owner::new(c, "nfstest", 2)?;
    want(
        a.test(&f, true, 0, 100)?.0,
        stat::GRANTED,
        "TEST on a free range",
    )?;
    want(
        a.lock(&f, false, true, 0, 100, false)?,
        stat::GRANTED,
        "LOCK",
    )?;
    let (st, h) = b.test(&f, true, 50, 10)?;
    want(st, stat::DENIED, "TEST by another process")?;
    let h = h.ok_or_else(|| Error::Check("no holder".into()))?;
    check(h.svid == 1 && h.exclusive, || format!("holder {h:?}"))?;
    want(
        a.test(&f, true, 0, 100)?.0,
        stat::GRANTED,
        "TEST by the holder",
    )?;
    want(a.unlock(&f, 0, 100)?, stat::GRANTED, "UNLOCK")?;
    want(
        b.test(&f, true, 50, 10)?.0,
        stat::GRANTED,
        "TEST after UNLOCK",
    )?;
    want(a.unlock(&f, 0, 100)?, stat::GRANTED, "UNLOCK of nothing")?;
    s.close()?;
    Ok(vec![line(
        "holder seen by TEST",
        format!("svid {} range {}+{}", h.svid, h.offset, h.len),
    )])
}

fn conflict(c: &Ctx) -> Lines {
    let (s, f) = file(c, "nlm-conflict")?;
    let mut a = Owner::new(c, "nfstest", 11)?;
    let mut b = Owner::new(c, "nfstest-b", 22)?;
    want(
        a.lock(&f, false, true, 0, 100, false)?,
        stat::GRANTED,
        "A LOCK 0+100",
    )?;
    want(
        b.lock(&f, false, true, 50, 100, false)?,
        stat::DENIED,
        "B LOCK 50+100 over A's",
    )?;
    want(
        b.lock(&f, false, true, 200, 100, false)?,
        stat::GRANTED,
        "B LOCK 200+100 beside",
    )?;
    want(
        a.lock(&f, false, false, 500, 0, false)?,
        stat::GRANTED,
        "A shared LOCK 500 to the end",
    )?;
    want(
        b.lock(&f, false, false, 600, 10, false)?,
        stat::GRANTED,
        "B shared LOCK inside A's shared",
    )?;
    want(
        b.lock(&f, false, true, 700, 10, false)?,
        stat::DENIED,
        "B exclusive inside A's shared",
    )?;
    for (off, len) in [(0, 100), (500, 0)] {
        want(a.unlock(&f, off, len)?, stat::GRANTED, "UNLOCK")?;
    }
    for (off, len) in [(200, 100), (600, 10)] {
        want(b.unlock(&f, off, len)?, stat::GRANTED, "UNLOCK")?;
    }
    want(
        b.lock(&f, false, true, 0, 0, false)?,
        stat::GRANTED,
        "B whole file once free",
    )?;
    want(b.unlock(&f, 0, 0)?, stat::GRANTED, "UNLOCK")?;
    s.close()?;
    Ok(vec![line("two hosts, two processes", "conflicts as RFC")])
}

fn blocked_granted(c: &Ctx) -> Lines {
    if !c.t.callbacks {
        return Err(Error::Skip(
            "needs --callbacks (the server calls this client back)".into(),
        ));
    }
    let cb = Callbacks::start(&c.t.cb_host, c.t.cb_pmap_port, c.t.cb_nlm_port)?;
    let (s, f) = file(c, "nlm-blocked")?;
    let mut a = Owner::new(c, "nfstest", 31)?;
    // The waiter is named by the callback address, so the server finds it.
    let mut b = Owner::new(c, &c.t.cb_host, 32)?;
    want(
        a.lock(&f, false, true, 0, 100, false)?,
        stat::GRANTED,
        "A LOCK",
    )?;
    want(
        b.lock(&f, true, true, 0, 100, false)?,
        stat::BLOCKED,
        "B blocking LOCK",
    )?;
    want(a.unlock(&f, 0, 100)?, stat::GRANTED, "A UNLOCK")?;
    let g = cb
        .wait_granted(32, Duration::from_secs(20))
        .ok_or_else(|| Error::Check("no GRANTED callback within 20 s".into()))?;
    check(g.fh == f && g.offset == 0 && g.len == 100, || {
        format!("GRANTED for {g:?}")
    })?;
    let (st, _) = a.test(&f, true, 0, 100)?;
    want(st, stat::DENIED, "A TEST after B was granted")?;
    want(b.unlock(&f, 0, 100)?, stat::GRANTED, "B UNLOCK")?;
    s.close()?;
    Ok(vec![
        line("callback lock manager", cb.nlm),
        line("GRANTED", "received for svid 32"),
    ])
}

fn cancel(c: &Ctx) -> Lines {
    let (s, f) = file(c, "nlm-cancel")?;
    let mut a = Owner::new(c, "nfstest", 41)?;
    let mut b = Owner::new(c, "nfstest-b", 42)?;
    want(
        b.cancel(&f, true, 0, 100)?,
        stat::DENIED,
        "CANCEL with nothing waiting",
    )?;
    want(
        a.lock(&f, false, true, 0, 100, false)?,
        stat::GRANTED,
        "A LOCK",
    )?;
    want(
        b.lock(&f, true, true, 0, 100, false)?,
        stat::BLOCKED,
        "B blocking LOCK",
    )?;
    want(b.cancel(&f, true, 0, 100)?, stat::GRANTED, "B CANCEL")?;
    want(a.unlock(&f, 0, 100)?, stat::GRANTED, "A UNLOCK")?;
    std::thread::sleep(Duration::from_millis(500));
    // The cancelled request was not granted meanwhile: A may lock again.
    want(
        a.lock(&f, false, true, 0, 100, false)?,
        stat::GRANTED,
        "A LOCK again",
    )?;
    want(a.unlock(&f, 0, 100)?, stat::GRANTED, "A UNLOCK")?;
    s.close()?;
    Ok(vec![line("CANCEL of a waiting lock", "NLM4_GRANTED")])
}

fn free_all(c: &Ctx) -> Lines {
    let (s, f) = file(c, "nlm-free-all")?;
    let mut a = Owner::new(c, "nfstest", 51)?;
    let mut b = Owner::new(c, "nfstest-b", 52)?;
    want(
        a.lock(&f, false, true, 0, 10, false)?,
        stat::GRANTED,
        "A LOCK 0+10",
    )?;
    want(
        a.lock(&f, false, true, 100, 10, false)?,
        stat::GRANTED,
        "A LOCK 100+10",
    )?;
    // FREE_ALL is a message: no results.
    a.c.call_raw(
        prog::NLM,
        4,
        proc4::FREE_ALL,
        &nlm::notify_args(b"nfstest", 3),
    )?;
    want(
        b.test(&f, true, 0, 200)?.0,
        stat::GRANTED,
        "TEST after FREE_ALL",
    )?;
    s.close()?;
    Ok(vec![line("FREE_ALL nfstest", "every lock gone")])
}

fn share(c: &Ctx) -> Lines {
    let (mut s, f) = file(c, "nlm-share")?;
    let mut a = Owner::new(c, "nfstest", 61)?;
    let mut b = Owner::new(c, "nfstest-b", 62)?;
    // fsm_DW (deny write), fsa_R (read).
    let first = a.share(&f, 2, 1, false)?;
    if first == stat::DENIED {
        // Nobody else shares the file: is the server's own descriptor in the way?
        std::thread::sleep(Duration::from_secs(7));
        let later = a.share(&f, 2, 1, false)?;
        // Another call on the connection that wrote.
        s.n.getattr(&f)?;
        let after_call = a.share(&f, 2, 1, false)?;
        let _ = a.share(&f, 2, 1, true);
        return fail(format!(
            "A SHARE read, deny write, of a file only this client uses: NLM4_DENIED right after a WRITE, {} 7 s later, {} after one more NFS call on the writing connection",
            stat_name(later),
            stat_name(after_call)
        ));
    }
    want(first, stat::GRANTED, "A SHARE read, deny write")?;
    want(b.share(&f, 0, 2, false)?, stat::DENIED, "B SHARE write")?;
    want(b.share(&f, 0, 1, false)?, stat::GRANTED, "B SHARE read")?;
    want(a.share(&f, 2, 1, true)?, stat::GRANTED, "A UNSHARE")?;
    want(b.share(&f, 0, 1, true)?, stat::GRANTED, "B UNSHARE read")?;
    want(
        b.share(&f, 0, 2, false)?,
        stat::GRANTED,
        "B SHARE write once free",
    )?;
    want(b.share(&f, 0, 2, true)?, stat::GRANTED, "B UNSHARE")?;
    s.close()?;
    Ok(vec![line("deny write against write", "NLM4_DENIED")])
}

fn grace(c: &Ctx) -> Lines {
    if !c.t.expect_grace {
        return Err(Error::Skip(
            "needs --expect-grace right after the server starts".into(),
        ));
    }
    let (s, f) = file(c, "nlm-grace")?;
    let mut a = Owner::new(c, "nfstest", 71)?;
    let st = a.lock(&f, false, true, 0, 10, false)?;
    check(st == stat::DENIED_GRACE_PERIOD, || {
        format!("LOCK in the grace period: {}", stat_name(st))
    })?;
    let t = a.test(&f, true, 0, 10)?.0;
    check(t == stat::DENIED_GRACE_PERIOD, || {
        format!("TEST in the grace period: {}", stat_name(t))
    })?;
    let r = a.lock(&f, false, true, 0, 10, true)?;
    check(r == stat::GRANTED, || {
        format!("reclaim in the grace period: {}", stat_name(r))
    })?;
    want(
        a.unlock(&f, 0, 10)?,
        stat::GRANTED,
        "UNLOCK in the grace period",
    )?;
    s.close()?;
    Ok(vec![
        line("LOCK, TEST", "NLM4_DENIED_GRACE_PERIOD"),
        line("reclaim", "NLM4_GRANTED"),
    ])
}

fn nsm_stat(c: &Ctx) -> Lines {
    let mut cl = c.client_as(c.nsm_addr()?, c.t.proto, Cred::sys(c.t.uid, c.t.gid));
    let b = cl.call_raw(prog::NSM, 1, 1, &nlm::sm_name(c.t.host.as_bytes()))?;
    let (res, state) = nlm::sm_stat_res(&b)?;
    check(res == 0, || format!("SM_STAT: res {res}"))?;
    check(state % 2 == 1, || format!("state {state} is even (down)"))?;
    Ok(vec![line("SM_STAT", "stat_succ"), line("state", state)])
}
