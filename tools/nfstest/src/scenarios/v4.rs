// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An NFS version 4 client over the hand-written `COMPOUND` (`wire::v4`):
//! the client ID (4.0) or session (4.1), the export through the pseudo
//! file system, a scenario's own directory, and the scenarios both minor
//! versions share.

use super::{Lines, line};
use crate::Error;
use crate::ctx::{Ctx, prog};
use crate::error::{check, fail};
use crate::rpc::Client;
use crate::target::Vers;
use crate::wire::v4::{
    self, Body, COMPOUND, Claim, Locker, Op, Open, Reply, Stateid, op, st, status_name,
};
use crate::wire::v4attr::{REQUIRED, a};

/// A version 4 client with its state.
#[derive(Debug)]
pub struct V4 {
    /// The RPC client.
    pub c: Client,
    /// The minor version.
    pub minor: u32,
    /// The client ID.
    pub clientid: u64,
    /// The session (4.1).
    pub session: Option<[u8; 16]>,
    /// The next sequence ID of slot 0 (4.1).
    pub seq: u32,
    /// What the scenario names itself.
    pub name: String,
    /// `EXCHANGE_ID`'s flags (4.1).
    pub eid_flags: u32,
}

/// The minor version of the run.
pub fn minor_of(c: &Ctx) -> u32 {
    u32::from(c.t.vers == Vers::V41)
}

impl V4 {
    /// A client of the server's NFS port; nothing sent yet.
    pub fn new(c: &Ctx, minor: u32, name: &str) -> V4 {
        V4 {
            c: c.client(c.nfs),
            minor,
            clientid: 0,
            session: None,
            seq: 1,
            name: format!("nfstest-{name}-{}", std::process::id()),
            eid_flags: 0,
        }
    }

    /// One `COMPOUND` as it is.
    pub fn raw(&mut self, minor: u32, ops: &[Op]) -> Result<Reply, Error> {
        let b = self
            .c
            .call_raw(prog::NFS, 4, COMPOUND, &v4::compound("nfstest", minor, ops))?;
        let r = v4::reply(&b)?;
        check(r.tag == b"nfstest", || {
            "the reply's tag is not the call's".into()
        })?;
        Ok(r)
    }

    /// One `COMPOUND`, after `SEQUENCE` in a session.
    pub fn call(&mut self, ops: &[Op]) -> Result<Reply, Error> {
        if let Some(sid) = self.session {
            let mut all = vec![Op::Sequence(sid, self.seq, 0, 0, false)];
            all.extend_from_slice(ops);
            self.seq = self.seq.wrapping_add(1);
            let mut r = self.raw(self.minor, &all)?;
            match r.res.first() {
                Some(s) if s.op == op::SEQUENCE && s.status == st::OK => {
                    r.res.remove(0);
                }
                Some(s) if s.op == op::SEQUENCE => {
                    return fail(format!("SEQUENCE: {}", status_name(s.status)));
                }
                _ => return fail("no SEQUENCE result"),
            }
            Ok(r)
        } else {
            self.raw(self.minor, ops)
        }
    }

    /// One `COMPOUND` that must succeed throughout.
    pub fn ok(&mut self, ops: &[Op]) -> Result<Reply, Error> {
        let r = self.call(ops)?;
        if r.status != st::OK {
            let at = r.res.last().map_or(0, |x| x.op);
            return fail(format!("operation {at}: {}", status_name(r.status)));
        }
        Ok(r)
    }

    /// The client ID (4.0) or the session (4.1).
    pub fn setup(&mut self) -> Result<(), Error> {
        let verf = *b"nfstest4";
        if self.minor == 0 {
            let r = self.raw(
                0,
                &[Op::SetClientId(
                    verf,
                    self.name.clone().into_bytes(),
                    0x4000_0000,
                    "tcp".into(),
                    "127.0.0.1.0.0".into(),
                    1,
                )],
            )?;
            let Some(Body::ClientId(id, confirm)) = r.res.first().map(|x| x.body.clone()) else {
                return fail(format!("SETCLIENTID: {}", status_name(r.status)));
            };
            let r = self.raw(0, &[Op::SetClientIdConfirm(id, confirm)])?;
            check(r.status == st::OK, || {
                format!("SETCLIENTID_CONFIRM: {}", status_name(r.status))
            })?;
            self.clientid = id;
        } else {
            let r = self.raw(
                1,
                &[Op::ExchangeId(verf, self.name.clone().into_bytes(), 0)],
            )?;
            let Some(Body::ExchangeId(id, seq, flags)) = r.res.first().map(|x| x.body.clone())
            else {
                return fail(format!("EXCHANGE_ID: {}", status_name(r.status)));
            };
            self.eid_flags = flags;
            let r = self.raw(1, &[Op::CreateSession(id, seq)])?;
            let Some(Body::Session(sid, _)) = r.res.first().map(|x| x.body.clone()) else {
                return fail(format!("CREATE_SESSION: {}", status_name(r.status)));
            };
            self.clientid = id;
            self.session = Some(sid);
            self.seq = 1;
            let r = self.call(&[Op::ReclaimComplete(false)])?;
            check(r.status == st::OK, || {
                format!("RECLAIM_COMPLETE: {}", status_name(r.status))
            })?;
        }
        Ok(())
    }

    /// Ends the session and the client ID (4.1).
    pub fn teardown(&mut self) -> Result<(), Error> {
        if let Some(sid) = self.session.take() {
            let r = self.raw(1, &[Op::DestroySession(sid)])?;
            check(r.status == st::OK, || {
                format!("DESTROY_SESSION: {}", status_name(r.status))
            })?;
            let r = self.raw(1, &[Op::DestroyClientId(self.clientid)])?;
            check(r.status == st::OK, || {
                format!("DESTROY_CLIENTID: {}", status_name(r.status))
            })?;
        }
        Ok(())
    }

    /// The handle of `path` under the pseudo file system's root.
    pub fn walk(&mut self, path: &str) -> Result<Vec<u8>, Error> {
        let mut ops = vec![Op::PutRootFh];
        for p in path.split('/').filter(|p| !p.is_empty()) {
            ops.push(Op::Lookup(p.as_bytes().to_vec()));
        }
        ops.push(Op::GetFh);
        let r = self.ok(&ops)?;
        match r.of(op::GETFH).map(|x| &x.body) {
            Some(Body::Fh(h)) => Ok(h.clone()),
            _ => fail("no GETFH result"),
        }
    }

    /// A directory under `dir` (made when missing): its handle.
    pub fn mkdir(&mut self, dir: &[u8], name: &[u8]) -> Result<Vec<u8>, Error> {
        let r = self.call(&[
            Op::PutFh(dir.to_vec()),
            Op::Mkdir(name.to_vec(), 0o777),
            Op::GetFh,
        ])?;
        if r.status == st::EXIST {
            return self.lookup(dir, name);
        }
        match r.of(op::GETFH).map(|x| &x.body) {
            Some(Body::Fh(h)) => Ok(h.clone()),
            _ => fail(format!("CREATE directory: {}", status_name(r.status))),
        }
    }

    /// `LOOKUP` of one name: its handle.
    pub fn lookup(&mut self, dir: &[u8], name: &[u8]) -> Result<Vec<u8>, Error> {
        let r = self.ok(&[
            Op::PutFh(dir.to_vec()),
            Op::Lookup(name.to_vec()),
            Op::GetFh,
        ])?;
        match r.of(op::GETFH).map(|x| &x.body) {
            Some(Body::Fh(h)) => Ok(h.clone()),
            _ => fail("no GETFH result"),
        }
    }

    /// Every name in `dir`.
    pub fn names(&mut self, dir: &[u8]) -> Result<Vec<Vec<u8>>, Error> {
        let mut out = Vec::new();
        let (mut cookie, mut verf) = (0u64, [0u8; 8]);
        for _ in 0..10_000 {
            let r = self.ok(&[
                Op::PutFh(dir.to_vec()),
                Op::ReadDir(cookie, verf, 4096, 16384, vec![a::TYPE]),
            ])?;
            let Some(Body::ReadDir(v, entries, eof)) = r.of(op::READDIR).map(|x| x.body.clone())
            else {
                return fail("no READDIR result");
            };
            verf = v;
            for e in entries {
                cookie = e.cookie;
                out.push(e.name);
            }
            if eof {
                return Ok(out);
            }
        }
        fail("READDIR never reached the end")
    }

    /// Removes everything in `dir`, then `name` in `parent`.
    pub fn remove_tree(&mut self, parent: &[u8], name: &[u8]) -> Result<(), Error> {
        let r = self.call(&[
            Op::PutFh(parent.to_vec()),
            Op::Lookup(name.to_vec()),
            Op::GetFh,
        ])?;
        let Some(Body::Fh(h)) = r.of(op::GETFH).map(|x| x.body.clone()) else {
            return Ok(());
        };
        let r = self.ok(&[Op::PutFh(h.clone()), Op::GetAttr(vec![a::TYPE])])?;
        if let Some(Body::Attrs(at)) = r.of(op::GETATTR).map(|x| &x.body)
            && at.ftype == Some(2)
        {
            for n in self.names(&h)? {
                self.remove_tree(&h, &n)?;
            }
        }
        self.ok(&[Op::PutFh(parent.to_vec()), Op::Remove(name.to_vec())])?;
        Ok(())
    }
}

/// A scenario's directory under the scratch directory, with its client.
#[derive(Debug)]
pub struct Session4 {
    /// The client.
    pub v: V4,
    /// The export.
    pub export: Vec<u8>,
    /// The scratch directory.
    pub scratch: Vec<u8>,
    /// This scenario's directory.
    pub dir: Vec<u8>,
    own: Vec<u8>,
}

impl Session4 {
    /// The client set up, `SCRATCH/<name>-v4-<pid>` made.
    pub fn open(c: &Ctx, name: &str) -> Result<Session4, Error> {
        let mut v = V4::new(c, minor_of(c), name);
        v.setup()?;
        let export = v.walk(&c.t.export)?;
        let mut scratch = export.clone();
        for p in c.t.scratch.split('/').filter(|p| !p.is_empty()) {
            scratch = v.mkdir(&scratch, p.as_bytes())?;
        }
        let own = format!("{name}-v4-{}", std::process::id()).into_bytes();
        v.remove_tree(&scratch, &own)?;
        let dir = v.mkdir(&scratch, &own)?;
        Ok(Session4 {
            v,
            export,
            scratch,
            dir,
            own,
        })
    }

    /// Removes the directory and ends the client's state.
    pub fn close(mut self) -> Result<(), Error> {
        let (s, o) = (self.scratch.clone(), self.own.clone());
        self.v.remove_tree(&s, &o)?;
        self.v.teardown()
    }
}

/// An open owner and its sequence (4.0; 4.1 sends 0).
#[derive(Debug)]
pub struct OpenOwner {
    /// The owner.
    pub owner: Vec<u8>,
    /// The next seqid.
    pub seqid: u32,
}

impl OpenOwner {
    /// A new owner.
    pub fn new(name: &str) -> OpenOwner {
        OpenOwner {
            owner: name.as_bytes().to_vec(),
            seqid: 0,
        }
    }

    /// The seqid of the next operation (4.0), or 0 (4.1).
    pub fn next(&mut self, minor: u32) -> u32 {
        if minor == 0 {
            let s = self.seqid;
            self.seqid += 1;
            s
        } else {
            0
        }
    }
}

/// Opens (creating when `create`) `name` in `dir`: the stateid, confirmed in 4.0.
pub fn open(
    v: &mut V4,
    o: &mut OpenOwner,
    dir: &[u8],
    name: &[u8],
    access: u32,
    deny: u32,
    create: bool,
) -> Result<Result<(Stateid, Vec<u8>), u32>, Error> {
    let seqid = o.next(v.minor);
    let r = v.call(&[
        Op::PutFh(dir.to_vec()),
        Op::Open(Open {
            seqid,
            access,
            deny,
            clientid: v.clientid,
            owner: o.owner.clone(),
            create: create.then_some(0o644),
            claim: Claim::Null(name.to_vec()),
        }),
        Op::GetFh,
    ])?;
    if r.status != st::OK {
        return Ok(Err(r.status));
    }
    let (Some(Body::Open(mut sid, rflags)), Some(Body::Fh(fh))) = (
        r.of(op::OPEN).map(|x| x.body.clone()),
        r.of(op::GETFH).map(|x| x.body.clone()),
    ) else {
        return fail("OPEN without its results");
    };
    if rflags & 2 != 0 {
        check(v.minor == 0, || "OPEN4_RESULT_CONFIRM in 4.1".into())?;
        let seqid = o.next(v.minor);
        let r = v.ok(&[Op::PutFh(fh.clone()), Op::OpenConfirm(sid, seqid)])?;
        let Some(Body::Stateid(s)) = r.of(op::OPEN_CONFIRM).map(|x| x.body.clone()) else {
            return fail("OPEN_CONFIRM without a stateid");
        };
        sid = s;
    }
    Ok(Ok((sid, fh)))
}

fn want_ok<T>(r: Result<T, u32>, what: &str) -> Result<T, Error> {
    r.or_else(|s| fail(format!("{what}: {}", status_name(s))))
}

/// `NULL` of version 4.
pub fn null(c: &Ctx) -> Lines {
    c.client(c.nfs).call_raw(prog::NFS, 4, 0, &[])?;
    Ok(vec![line("NFS version 4 NULL", "answered")])
}

/// Minor version 2 is refused with no results.
pub fn minor2(c: &Ctx) -> Lines {
    let mut v = V4::new(c, 2, "minor2");
    let r = v.raw(2, &[Op::PutRootFh, Op::GetFh])?;
    check(r.status == st::MINOR_VERS_MISMATCH, || {
        format!("minor version 2: {}", status_name(r.status))
    })?;
    check(r.res.is_empty(), || format!("{} results", r.res.len()))?;
    Ok(vec![line("minor version 2", status_name(r.status))])
}

/// An operation number nobody has is `OP_ILLEGAL`.
pub fn illegal(c: &Ctx) -> Lines {
    let mut v = V4::new(c, minor_of(c), "illegal");
    v.setup()?;
    let r = v.call(&[Op::PutRootFh, Op::Raw(9999)])?;
    let last = r
        .res
        .last()
        .ok_or_else(|| Error::Check("no results".into()))?;
    check(
        last.op == op::ILLEGAL && last.status == st::OP_ILLEGAL,
        || {
            format!(
                "operation 9999: op {} {}",
                last.op,
                status_name(last.status)
            )
        },
    )?;
    check(r.status == st::OP_ILLEGAL, || status_name(r.status))?;
    v.teardown()?;
    Ok(vec![line(
        "operation 9999",
        "OP_ILLEGAL, NFS4ERR_OP_ILLEGAL",
    )])
}

/// Down the pseudo file system to the export, and its required attributes.
pub fn lookup_export(c: &Ctx) -> Lines {
    let mut v = V4::new(c, minor_of(c), "lookup");
    v.setup()?;
    let mut out = Vec::new();
    let mut ops = vec![Op::PutRootFh, Op::GetAttr(vec![a::TYPE, a::FSID])];
    for p in c.t.export.split('/').filter(|p| !p.is_empty()) {
        ops.push(Op::Lookup(p.as_bytes().to_vec()));
        ops.push(Op::GetAttr(vec![a::TYPE, a::FSID]));
    }
    ops.push(Op::GetAttr(REQUIRED.to_vec()));
    let r = v.ok(&ops)?;
    for x in r.res.iter().filter(|x| x.op == op::GETATTR) {
        if let Body::Attrs(at) = &x.body {
            if at.present.len() == 2 {
                check(at.ftype == Some(2), || {
                    format!("a step is type {:?}", at.ftype)
                })?;
                out.push(format!("step fsid {:?}", at.fsid.unwrap_or_default()));
            } else {
                let missing: Vec<u32> = REQUIRED
                    .iter()
                    .filter(|n| !at.present.contains(n))
                    .copied()
                    .collect();
                check(missing.is_empty(), || {
                    format!("required attributes missing: {missing:?}")
                })?;
                let sup = at.supported.clone().unwrap_or_default();
                check(REQUIRED.iter().all(|n| sup.contains(n)), || {
                    "supported_attrs lacks a required attribute".into()
                })?;
                out.push(line("lease_time", at.lease_time.unwrap_or(0)));
                out.push(line("fh_expire_type", at.fh_expire_type.unwrap_or(0)));
                out.push(line("supported", format!("{sup:?}")));
            }
        }
    }
    v.teardown()?;
    Ok(out)
}

/// `OPEN` create, `WRITE`, `COMMIT`, `READ`, `CLOSE`.
pub fn open_write_read(c: &Ctx) -> Lines {
    let mut s = Session4::open(c, "open-write-read")?;
    let mut o = OpenOwner::new("ow");
    let d = s.dir.clone();
    let (sid, fh) = want_ok(open(&mut s.v, &mut o, &d, b"f", 3, 0, true)?, "OPEN create")?;
    let data = super::files::pattern(0, 70_000);
    let r =
        s.v.ok(&[Op::PutFh(fh.clone()), Op::Write(sid, 0, 0, data.clone())])?;
    let Some(Body::Write(n, _, wv)) = r.of(op::WRITE).map(|x| x.body.clone()) else {
        return fail("no WRITE result");
    };
    check(n as usize == data.len(), || format!("WRITE took {n}"))?;
    let r = s.v.ok(&[Op::PutFh(fh.clone()), Op::Commit(0, 0)])?;
    let Some(Body::Commit(cv)) = r.of(op::COMMIT).map(|x| x.body.clone()) else {
        return fail("no COMMIT result");
    };
    check(cv == wv, || "COMMIT's verifier is not the write's".into())?;
    let r =
        s.v.ok(&[Op::PutFh(fh.clone()), Op::Read(sid, 0, 100_000)])?;
    let Some(Body::Read(eof, back)) = r.of(op::READ).map(|x| x.body.clone()) else {
        return fail("no READ result");
    };
    check(back == data && eof, || {
        format!("READ: {} bytes, eof {eof}", back.len())
    })?;
    let seqid = o.next(s.v.minor);
    s.v.ok(&[Op::PutFh(fh), Op::Close(seqid, sid)])?;
    s.close()?;
    Ok(vec![line("bytes", data.len())])
}

/// `READDIR` of 300 names in small pages.
pub fn readdir(c: &Ctx) -> Lines {
    let mut s = Session4::open(c, "readdir")?;
    let d = s.dir.clone();
    for i in 0..300 {
        let mut o = OpenOwner::new(&format!("rd{i}"));
        let name = format!("e{i:04}").into_bytes();
        let (sid, fh) = want_ok(open(&mut s.v, &mut o, &d, &name, 3, 0, true)?, "OPEN")?;
        let seqid = o.next(s.v.minor);
        s.v.ok(&[Op::PutFh(fh), Op::Close(seqid, sid)])?;
    }
    let names = s.v.names(&d)?;
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    check(sorted.len() == names.len() && names.len() == 300, || {
        format!("{} names, {} distinct", names.len(), sorted.len())
    })?;
    check(!names.iter().any(|n| n == b"." || n == b".."), || {
        "`.` or `..` listed".into()
    })?;
    s.close()?;
    Ok(vec![line("entries", 300)])
}

/// `RENAME` and `REMOVE`.
pub fn rename_remove(c: &Ctx) -> Lines {
    let mut s = Session4::open(c, "rename-remove")?;
    let mut o = OpenOwner::new("rr");
    let d = s.dir.clone();
    let (sid, fh) = want_ok(open(&mut s.v, &mut o, &d, b"a", 3, 0, true)?, "OPEN")?;
    let seqid = o.next(s.v.minor);
    s.v.ok(&[Op::PutFh(fh), Op::Close(seqid, sid)])?;
    let sub = s.v.mkdir(&d, b"sub")?;
    s.v.ok(&[
        Op::PutFh(d.clone()),
        Op::SaveFh,
        Op::PutFh(sub.clone()),
        Op::Rename(b"a".to_vec(), b"b".to_vec()),
    ])?;
    s.v.lookup(&sub, b"b")?;
    let r =
        s.v.call(&[Op::PutFh(d.clone()), Op::Lookup(b"a".to_vec())])?;
    check(r.status == st::NOENT, || {
        format!("LOOKUP a: {}", status_name(r.status))
    })?;
    s.v.ok(&[Op::PutFh(sub.clone()), Op::Remove(b"b".to_vec())])?;
    let r = s.v.call(&[Op::PutFh(sub), Op::Remove(b"b".to_vec())])?;
    check(r.status == st::NOENT, || {
        format!("REMOVE again: {}", status_name(r.status))
    })?;
    s.close()?;
    Ok(vec![line(
        "RENAME across, REMOVE twice",
        "NFS4ERR_NOENT the second time",
    )])
}

/// `SETATTR` of mode and size.
pub fn setattr(c: &Ctx) -> Lines {
    let mut s = Session4::open(c, "setattr")?;
    let mut o = OpenOwner::new("sa");
    let d = s.dir.clone();
    let (sid, fh) = want_ok(open(&mut s.v, &mut o, &d, b"f", 3, 0, true)?, "OPEN")?;
    let r = s.v.ok(&[
        Op::PutFh(fh.clone()),
        Op::SetAttr(sid, Some(0o600), Some(1234)),
        Op::GetAttr(vec![a::SIZE, a::MODE]),
    ])?;
    let Some(Body::Attrs(at)) = r.of(op::GETATTR).map(|x| x.body.clone()) else {
        return fail("no GETATTR result");
    };
    check(at.size == Some(1234), || format!("size {:?}", at.size))?;
    check(at.mode.map(|m| m & 0o707) == Some(0o600), || {
        format!("mode {:?}", at.mode)
    })?;
    let seqid = o.next(s.v.minor);
    s.v.ok(&[Op::PutFh(fh), Op::Close(seqid, sid)])?;
    s.close()?;
    Ok(vec![
        line("size", 1234),
        line("mode", format!("{:o}", at.mode.unwrap_or(0))),
    ])
}

/// `LOCK`, `LOCKT`, `LOCKU` with two owners.
pub fn locks(c: &Ctx) -> Lines {
    let mut s = Session4::open(c, "locks")?;
    let d = s.dir.clone();
    let minor = s.v.minor;
    let mut oa = OpenOwner::new("lock-a");
    let mut ob = OpenOwner::new("lock-b");
    let (sa, fh) = want_ok(open(&mut s.v, &mut oa, &d, b"f", 3, 0, true)?, "OPEN a")?;
    let (sb, _) = want_ok(open(&mut s.v, &mut ob, &d, b"f", 3, 0, false)?, "OPEN b")?;
    let id = s.v.clientid;
    let lock_a = Locker::New {
        open_seqid: oa.next(minor),
        open_stateid: sa,
        lock_seqid: 0,
        clientid: id,
        owner: b"la".to_vec(),
    };
    let r =
        s.v.ok(&[Op::PutFh(fh.clone()), Op::Lock(2, false, 0, 100, lock_a)])?;
    let Some(Body::Stateid(la)) = r.of(op::LOCK).map(|x| x.body.clone()) else {
        return fail("no LOCK stateid");
    };
    let r = s.v.call(&[
        Op::PutFh(fh.clone()),
        Op::LockT(2, 50, 10, id, b"lb".to_vec()),
    ])?;
    check(r.status == st::DENIED, || {
        format!("LOCKT by b: {}", status_name(r.status))
    })?;
    let held = r.of(op::LOCKT).map(|x| x.body.clone());
    let lock_b = Locker::New {
        open_seqid: ob.next(minor),
        open_stateid: sb,
        lock_seqid: 0,
        clientid: id,
        owner: b"lb".to_vec(),
    };
    let r =
        s.v.call(&[Op::PutFh(fh.clone()), Op::Lock(2, false, 50, 10, lock_b)])?;
    check(r.status == st::DENIED, || {
        format!("LOCK by b: {}", status_name(r.status))
    })?;
    let r = s.v.ok(&[
        Op::PutFh(fh.clone()),
        Op::LockU(2, u32::from(minor == 0), la, 0, 100),
    ])?;
    check(r.of(op::LOCKU).is_some(), || "no LOCKU result".into())?;
    let r = s.v.call(&[
        Op::PutFh(fh.clone()),
        Op::LockT(2, 50, 10, id, b"lb".to_vec()),
    ])?;
    check(r.status == st::OK, || {
        format!("LOCKT after LOCKU: {}", status_name(r.status))
    })?;
    for (o, sid) in [(&mut oa, sa), (&mut ob, sb)] {
        let seqid = o.next(minor);
        let r = s.v.call(&[Op::PutFh(fh.clone()), Op::Close(seqid, sid)])?;
        check(r.status == st::OK || r.status == 10037, || {
            format!("CLOSE: {}", status_name(r.status))
        })?;
    }
    s.close()?;
    Ok(vec![line("holder seen by LOCKT", format!("{held:?}"))])
}

/// A deny-write open conflicts with a write open.
pub fn share_deny(c: &Ctx) -> Lines {
    let mut s = Session4::open(c, "share-deny")?;
    let d = s.dir.clone();
    let mut oa = OpenOwner::new("deny-a");
    let mut ob = OpenOwner::new("deny-b");
    let (sa, fh) = want_ok(
        open(&mut s.v, &mut oa, &d, b"f", 1, 2, true)?,
        "OPEN read, deny write",
    )?;
    let r = open(&mut s.v, &mut ob, &d, b"f", 2, 0, false)?;
    let Err(st_b) = r else {
        return fail("OPEN write over a deny-write open was granted");
    };
    check(st_b == st::SHARE_DENIED, || {
        format!("OPEN write: {}", status_name(st_b))
    })?;
    let seqid = oa.next(s.v.minor);
    s.v.ok(&[Op::PutFh(fh), Op::Close(seqid, sa)])?;
    s.close()?;
    Ok(vec![line("write over deny write", status_name(st_b))])
}
