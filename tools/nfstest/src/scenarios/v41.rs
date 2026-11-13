// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS version 4.1: sessions, slots, and what 4.1 refuses.

use super::v4::{self, OpenOwner, Session4, V4, open};
use super::{Family, Lines, Scenario, line};
use crate::ctx::{Ctx, prog};
use crate::error::{check, fail};
use crate::wire::v4::{Body, COMPOUND, Op, op, st, status_name};
use crate::wire::v4attr::a;

/// This file's scenarios.
pub const LIST: [Scenario; 13] = [
    s("v41-exchange-session", exchange_session),
    s("v41-lookup-export", v4::lookup_export),
    s("v41-open-write-read", v4::open_write_read),
    s("v41-readdir", v4::readdir),
    s("v41-rename-remove", v4::rename_remove),
    s("v41-setattr", v4::setattr),
    s("v41-locks", v4::locks),
    s("v41-share-deny", v4::share_deny),
    s("v41-slot-replay", slot_replay),
    s("v41-misordered-sequence", misordered),
    s("v41-no-sequence-refused", no_sequence),
    s("v41-layoutget-refused", layoutget),
    s("v41-acl-not-returned", acl),
];

const fn s(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario {
        name,
        family: Family::V41,
        run,
    }
}

/// `NFS4ERR_BADSESSION`.
const BADSESSION: u32 = 10052;
/// `EXCHGID4_FLAG_USE_NON_PNFS`.
const USE_NON_PNFS: u32 = 0x0001_0000;

fn exchange_session(c: &Ctx) -> Lines {
    let mut v = V4::new(c, 1, "session");
    v.setup()?;
    let flags = v.eid_flags;
    check(flags & USE_NON_PNFS != 0, || {
        format!("EXCHANGE_ID flags {flags:#x} without USE_NON_PNFS")
    })?;
    let sid = v.session.expect("set up");
    v.teardown()?;
    let r = v.raw(1, &[Op::Sequence(sid, 2, 0, 0, false), Op::PutRootFh])?;
    check(r.status == BADSESSION, || {
        format!("SEQUENCE on a destroyed session: {}", status_name(r.status))
    })?;
    Ok(vec![
        line("EXCHANGE_ID flags", format!("{flags:#x}")),
        line("after DESTROY_SESSION", status_name(r.status)),
    ])
}

fn slot_replay(c: &Ctx) -> Lines {
    let mut v = V4::new(c, 1, "replay");
    v.setup()?;
    let sid = v.session.expect("set up");
    let args = crate::wire::v4::compound(
        "nfstest",
        1,
        &[
            Op::Sequence(sid, v.seq, 0, 0, true),
            Op::PutRootFh,
            Op::GetFh,
        ],
    );
    let first = v.c.call_raw(prog::NFS, 4, COMPOUND, &args)?;
    let again = v.c.call_raw(prog::NFS, 4, COMPOUND, &args)?;
    check(first == again, || {
        "the replayed slot did not get the cached reply".into()
    })?;
    v.seq += 1;
    v.teardown()?;
    Ok(vec![line("same slot and sequence ID", "the cached reply")])
}

fn misordered(c: &Ctx) -> Lines {
    let mut v = V4::new(c, 1, "misordered");
    v.setup()?;
    let sid = v.session.expect("set up");
    let r = v.raw(
        1,
        &[Op::Sequence(sid, v.seq + 5, 0, 0, false), Op::PutRootFh],
    )?;
    check(r.status == st::SEQ_MISORDERED, || {
        format!("sequence ID ahead by 5: {}", status_name(r.status))
    })?;
    v.teardown()?;
    Ok(vec![line("sequence ID ahead by 5", status_name(r.status))])
}

fn no_sequence(c: &Ctx) -> Lines {
    let mut v = V4::new(c, 1, "nosequence");
    let r = v.raw(1, &[Op::PutRootFh, Op::GetFh])?;
    check(r.status == st::OP_NOT_IN_SESSION, || {
        format!("PUTROOTFH first: {}", status_name(r.status))
    })?;
    Ok(vec![line("no SEQUENCE", status_name(r.status))])
}

fn layoutget(c: &Ctx) -> Lines {
    let mut s = Session4::open(c, "layoutget")?;
    let mut o = OpenOwner::new("lg");
    let d = s.dir.clone();
    let Ok((sid, fh)) = open(&mut s.v, &mut o, &d, b"f", 3, 0, true)? else {
        return fail("OPEN");
    };
    let r = s.v.call(&[Op::PutFh(fh.clone()), Op::LayoutGet(sid)])?;
    check(r.status == st::NOTSUPP, || {
        format!("LAYOUTGET: {}", status_name(r.status))
    })?;
    s.v.ok(&[Op::PutFh(fh), Op::Close(0, sid)])?;
    s.close()?;
    Ok(vec![line("LAYOUTGET", status_name(r.status))])
}

fn acl(c: &Ctx) -> Lines {
    let mut v = V4::new(c, 1, "acl");
    v.setup()?;
    let export = v.walk(&c.t.export)?;
    let r = match v.call(&[Op::PutFh(export), Op::GetAttr(vec![a::TYPE, a::ACL])]) {
        Ok(r) => r,
        Err(crate::Error::Decode(why)) => return fail(format!("acl came back ({why})")),
        Err(e) => return Err(e),
    };
    check(r.status == st::OK, || {
        format!("GETATTR: {}", status_name(r.status))
    })?;
    let Some(Body::Attrs(at)) = r.of(op::GETATTR).map(|x| x.body.clone()) else {
        return fail("no GETATTR result");
    };
    check(!at.present.contains(&a::ACL), || "acl returned".into())?;
    v.teardown()?;
    Ok(vec![line(
        "asked type and acl, got",
        format!("{:?}", at.present),
    )])
}
