// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Attributes, `LOOKUP` of `.` and `..`, `ACCESS`, the file system's
//! information, and handles that are stale or not handles at all.

use nfs_v3::wire::{
    ACCESS3_DELETE, ACCESS3_EXECUTE, ACCESS3_EXTEND, ACCESS3_LOOKUP, ACCESS3_MODIFY, ACCESS3_READ,
    ACCESS3args, ACCESS3res, FSINFO3args, FSINFO3res, FSSTAT3args, FSSTAT3res, NFS_PROGRAM as P,
    Nfs3Option, PATHCONF3args, PATHCONF3res, ftype3, nfsstat3, nfstime3, sattr3, set_atime,
    set_mtime,
};

use super::{Family, Lines, Scenario, line};
use crate::ctx::{Ctx, prog};
use crate::error::{check, fail};
use crate::v3::{Nfs3, Session, expect_err, fh, ok, root_handle, status};

/// This file's scenarios.
pub const LIST: [Scenario; 7] = [
    s("getattr-setattr", getattr_setattr),
    s("setattr-guard", setattr_guard),
    s("lookup-dots", lookup_dots),
    s("access-bits", access_bits),
    s("fsinfo-fsstat-pathconf", fs_info),
    s("stale-handle", stale),
    s("bad-handle", bad_handle),
];

const fn s(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario {
        name,
        family: Family::V3,
        run,
    }
}

fn getattr_setattr(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "getattr-setattr")?;
    let d = s.dir.clone();
    let f = s.n.create(&d, b"f", 0o644)?;
    let a = s.n.getattr(&f)?;
    check(a.type_ == ftype3::NF3REG, || format!("type {}", a.type_))?;
    // Owner and public bits; the group's show only with a primary group.
    check(a.mode & 0o707 == 0o604, || format!("mode {:o}", a.mode))?;
    let group_bits = a.mode & 0o070;
    check(a.size == 0, || format!("size {}", a.size))?;
    let set = |m: sattr3| (m, Nfs3Option::None);
    let (m, g) = set(crate::v3::mode(0o600));
    ok(s.n.setattr(&f, m, g)?, "SETATTR mode")?;
    let a = s.n.getattr(&f)?;
    check(a.mode & 0o777 == 0o600, || {
        format!("mode after SETATTR {:o}", a.mode)
    })?;
    ok(
        s.n.setattr(
            &f,
            sattr3 {
                size: Nfs3Option::Some(100),
                ..sattr3::default()
            },
            Nfs3Option::None,
        )?,
        "SETATTR size",
    )?;
    let a = s.n.getattr(&f)?;
    check(a.size == 100, || format!("size after SETATTR {}", a.size))?;
    let t = nfstime3 {
        seconds: 1_000_000_000,
        nseconds: 0,
    };
    ok(
        s.n.setattr(
            &f,
            sattr3 {
                atime: set_atime::SET_TO_CLIENT_TIME(t),
                mtime: set_mtime::SET_TO_CLIENT_TIME(t),
                ..sattr3::default()
            },
            Nfs3Option::None,
        )?,
        "SETATTR times",
    )?;
    let a = s.n.getattr(&f)?;
    check(
        a.mtime.seconds == t.seconds && a.atime.seconds == t.seconds,
        || format!("times after SETATTR {:?} {:?}", a.atime, a.mtime),
    )?;
    ok(
        s.n.setattr(
            &f,
            sattr3 {
                mtime: set_mtime::SET_TO_SERVER_TIME,
                ..sattr3::default()
            },
            Nfs3Option::None,
        )?,
        "SETATTR server time",
    )?;
    let b = s.n.getattr(&f)?;
    check(b.mtime.seconds > t.seconds, || "server time not set".into())?;
    s.close()?;
    Ok(vec![
        line("mode asked 644, given", format!("{:o}", 0o604 | group_bits)),
        line("uid", a.uid),
        line("gid", a.gid),
        line("fsid", a.fsid),
        line("fileid", a.fileid),
    ])
}

fn setattr_guard(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "setattr-guard")?;
    let d = s.dir.clone();
    let f = s.n.create(&d, b"f", 0o644)?;
    let a = s.n.getattr(&f)?;
    let wrong = nfstime3 {
        seconds: a.ctime.seconds.wrapping_sub(1000),
        nseconds: 7,
    };
    let r =
        s.n.setattr(&f, crate::v3::mode(0o600), Nfs3Option::Some(wrong))?;
    expect_err(
        &r,
        nfsstat3::NFS3ERR_NOT_SYNC,
        "SETATTR with a wrong ctime guard",
    )?;
    let r =
        s.n.setattr(&f, crate::v3::mode(0o600), Nfs3Option::Some(a.ctime))?;
    ok(r, "SETATTR with the right ctime guard")?;
    s.close()?;
    Ok(vec![line("wrong guard", "NFS3ERR_NOT_SYNC")])
}

fn lookup_dots(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "lookup-dots")?;
    let root = s.root.clone();
    let ra = s.n.getattr(&root)?;
    let dot = s.n.lookup(&root, b".")?;
    let da = s.n.getattr(&dot)?;
    check(da.fileid == ra.fileid, || {
        "`.` at the root is another object".into()
    })?;
    let mut out = vec![line("`.` at the root", "the root")];
    match crate::v3::split(s.n.lookup_res(&root, b"..")?) {
        Ok(r) => {
            let pa = s.n.getattr(&r.object.data)?;
            out.push(line(
                "`..` at the root",
                if pa.fileid == ra.fileid {
                    "the root itself".to_string()
                } else {
                    format!("another object, fileid {} (outside the export)", pa.fileid)
                },
            ));
            check(pa.fsid != ra.fsid || pa.fileid == ra.fileid, || {
                "`..` at the export's root leaves the export".into()
            })?;
        }
        Err(st) => out.push(line("`..` at the root", st)),
    }
    let d = s.dir.clone();
    let up = s.n.lookup(&d, b"..")?;
    let sa = s.n.getattr(&s.scratch.clone())?;
    check(s.n.getattr(&up)?.fileid == sa.fileid, || {
        "`..` of a directory is not its parent".into()
    })?;
    out.push(line("`..` below", "the parent"));
    s.close()?;
    Ok(out)
}

fn access(n: &mut Nfs3, f: &[u8], bits: u32) -> Result<u32, crate::Error> {
    let r: ACCESS3res = n.call(
        P::NFSPROC3_ACCESS,
        &ACCESS3args {
            object: fh(f),
            access: bits,
        },
    )?;
    Ok(ok(r, "ACCESS")?.access)
}

fn bits(b: u32) -> String {
    let names = [
        (ACCESS3_READ, "READ"),
        (ACCESS3_LOOKUP, "LOOKUP"),
        (ACCESS3_MODIFY, "MODIFY"),
        (ACCESS3_EXTEND, "EXTEND"),
        (ACCESS3_DELETE, "DELETE"),
        (ACCESS3_EXECUTE, "EXECUTE"),
    ];
    names
        .iter()
        .filter(|(m, _)| b & m != 0)
        .map(|(_, n)| *n)
        .collect::<Vec<_>>()
        .join("|")
}

fn access_bits(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "access-bits")?;
    let d = s.dir.clone();
    let f = s.n.create(&d, b"f", 0o644)?;
    let all = 0x3f;
    let fa = access(&mut s.n, &f, all)?;
    check(fa & !all == 0, || format!("bits not asked for: {fa:#x}"))?;
    check(fa & ACCESS3_READ != 0 && fa & ACCESS3_MODIFY != 0, || {
        format!("own file 0644: {}", bits(fa))
    })?;
    let da = access(&mut s.n, &d, all)?;
    check(da & ACCESS3_LOOKUP != 0 && da & ACCESS3_EXTEND != 0, || {
        format!("own directory: {}", bits(da))
    })?;
    ok(
        s.n.setattr(&f, crate::v3::mode(0o444), Nfs3Option::None)?,
        "SETATTR 0444",
    )?;
    let ro = access(&mut s.n, &f, all)?;
    if !c.t.expect_root {
        check(ro & ACCESS3_MODIFY == 0, || {
            format!("0444 file: {}", bits(ro))
        })?;
    }
    s.close()?;
    Ok(vec![
        line("file 0644", bits(fa)),
        line("directory", bits(da)),
        line("file 0444", bits(ro)),
    ])
}

fn fs_info(c: &Ctx) -> Lines {
    let root = root_handle(c)?;
    let mut n = Nfs3 { c: c.client(c.nfs) };
    let i: FSINFO3res = n.call(P::NFSPROC3_FSINFO, &FSINFO3args { fsroot: fh(&root) })?;
    let i = ok(i, "FSINFO")?;
    check(
        i.rtmax > 0 && i.wtmax > 0 && i.rtpref <= i.rtmax && i.wtpref <= i.wtmax,
        || format!("sizes {i:?}"),
    )?;
    let st: FSSTAT3res = n.call(P::NFSPROC3_FSSTAT, &FSSTAT3args { fsroot: fh(&root) })?;
    let st = ok(st, "FSSTAT")?;
    check(st.tbytes >= st.fbytes && st.fbytes >= st.abytes, || {
        format!(
            "bytes total {} free {} available {}",
            st.tbytes, st.fbytes, st.abytes
        )
    })?;
    let pc: PATHCONF3res = n.call(P::NFSPROC3_PATHCONF, &PATHCONF3args { object: fh(&root) })?;
    let pc = ok(pc, "PATHCONF")?;
    check(pc.name_max >= 255, || format!("name_max {}", pc.name_max))?;
    Ok(vec![
        format!(
            "rtmax {} rtpref {} wtmax {} wtpref {} dtpref {}",
            i.rtmax, i.rtpref, i.wtmax, i.wtpref, i.dtpref
        ),
        format!(
            "properties {:#x} maxfilesize {}",
            i.properties, i.maxfilesize
        ),
        format!(
            "tbytes {} fbytes {} abytes {}",
            st.tbytes, st.fbytes, st.abytes
        ),
        format!(
            "name_max {} no_trunc {} case_insensitive {} case_preserving {}",
            pc.name_max, pc.no_trunc, pc.case_insensitive, pc.case_preserving
        ),
    ])
}

fn stale(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "stale-handle")?;
    let d = s.dir.clone();
    let f = s.n.create(&d, b"f", 0o644)?;
    ok(s.n.remove_res(&d, b"f")?, "REMOVE")?;
    let r = s.n.getattr_res(&f)?;
    expect_err(&r, nfsstat3::NFS3ERR_STALE, "GETATTR after REMOVE")?;
    let r = s.n.read_res(&f, 0, 10)?;
    expect_err(&r, nfsstat3::NFS3ERR_STALE, "READ after REMOVE")?;
    s.close()?;
    Ok(vec![line("after REMOVE", "NFS3ERR_STALE")])
}

fn bad_handle(c: &Ctx) -> Lines {
    let root = root_handle(c)?;
    let mut n = Nfs3 { c: c.client(c.nfs) };
    let mut out = Vec::new();
    let flipped: Vec<u8> = root.iter().map(|b| !b).collect();
    let mut short = root.clone();
    short.truncate(root.len() / 2);
    for (what, h) in [
        ("every byte flipped", flipped),
        ("half of it", short),
        ("empty", Vec::new()),
        ("64 bytes of 0xA5", vec![0xA5; 64]),
    ] {
        let r = n.getattr_res(&h)?;
        let st = status(&r);
        check(
            matches!(st, nfsstat3::NFS3ERR_BADHANDLE | nfsstat3::NFS3ERR_STALE),
            || format!("{what}: {st}"),
        )?;
        out.push(line(what, st));
    }
    n.c.call_raw(prog::NFS, 3, 0, &[])?;
    if out.is_empty() {
        return fail("nothing tried");
    }
    out.push("still serving".into());
    Ok(out)
}
