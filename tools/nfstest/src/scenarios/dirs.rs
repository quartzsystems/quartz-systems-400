// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Directories: `MKDIR`/`RMDIR`, `RENAME`, and `READDIR`/`READDIRPLUS`
//! paged in small replies.

use std::collections::BTreeMap;

use nfs_v3::wire::{
    NFS_PROGRAM as P, READDIR3args, READDIR3res, RENAME3args, RENAME3res, cookieverf3, nfsstat3,
};

use super::{Family, Lines, Scenario, line};
use crate::ctx::Ctx;
use crate::error::{check, fail};
use crate::v3::{Nfs3, Session, dirop, expect_err, fh, ok, status};

/// This file's scenarios.
pub const LIST: [Scenario; 7] = [
    s("mkdir-rmdir", mkdir_rmdir),
    s("rename-over", rename_over),
    s("rename-across", rename_across),
    s("rename-dir-into-itself", rename_into_itself),
    s("readdir-paging", readdir_paging),
    s("readdirplus-paging", readdirplus_paging),
    s("readdir-bad-cookie", bad_cookie),
];

const fn s(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario {
        name,
        family: Family::V3,
        run,
    }
}

fn rename(
    n: &mut Nfs3,
    fd: &[u8],
    from: &[u8],
    td: &[u8],
    to: &[u8],
) -> Result<RENAME3res, crate::Error> {
    n.call(
        P::NFSPROC3_RENAME,
        &RENAME3args {
            from: dirop(fd, from),
            to: dirop(td, to),
        },
    )
}

fn mkdir_rmdir(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "mkdir-rmdir")?;
    let d = s.dir.clone();
    let sub = s.n.mkdir(&d, b"sub")?;
    expect_err(
        &s.n.mkdir_res(&d, b"sub", 0o755)?,
        nfsstat3::NFS3ERR_EXIST,
        "MKDIR again",
    )?;
    s.n.create(&sub, b"f", 0o644)?;
    expect_err(
        &s.n.rmdir_res(&d, b"sub")?,
        nfsstat3::NFS3ERR_NOTEMPTY,
        "RMDIR of a non-empty directory",
    )?;
    ok(s.n.remove_res(&sub, b"f")?, "REMOVE")?;
    ok(s.n.rmdir_res(&d, b"sub")?, "RMDIR")?;
    s.n.create(&d, b"file", 0o644)?;
    let st = status(&s.n.rmdir_res(&d, b"file")?);
    check(st == nfsstat3::NFS3ERR_NOTDIR, || {
        format!("RMDIR of a file: {st}")
    })?;
    let st = status(&s.n.rmdir_res(&d, b".")?);
    check(st != nfsstat3::NFS3_OK, || "RMDIR of `.` was done".into())?;
    s.close()?;
    Ok(vec![
        line("RMDIR non-empty", "NFS3ERR_NOTEMPTY"),
        line("RMDIR of `.`", st),
    ])
}

fn rename_over(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "rename-over")?;
    let d = s.dir.clone();
    let a = s.n.create(&d, b"a", 0o644)?;
    s.n.write(&a, 0, b"from a")?;
    let b = s.n.create(&d, b"b", 0o644)?;
    s.n.write(&b, 0, b"from b")?;
    ok(rename(&mut s.n, &d, b"a", &d, b"b")?, "RENAME a over b")?;
    expect_err(
        &s.n.lookup_res(&d, b"a")?,
        nfsstat3::NFS3ERR_NOENT,
        "LOOKUP a after RENAME",
    )?;
    let nb = s.n.lookup(&d, b"b")?;
    check(s.n.read(&nb, 0, 100)?.0 == b"from a", || {
        "b does not hold a's data".into()
    })?;
    // Renaming a name onto itself does nothing.
    ok(rename(&mut s.n, &d, b"b", &d, b"b")?, "RENAME b to b")?;
    // A directory over a file, and a missing source.
    s.n.mkdir(&d, b"dir")?;
    let st = status(&rename(&mut s.n, &d, b"dir", &d, b"b")?);
    check(st != nfsstat3::NFS3_OK, || {
        "a directory renamed over a file".into()
    })?;
    let miss = status(&rename(&mut s.n, &d, b"nope", &d, b"x")?);
    check(miss == nfsstat3::NFS3ERR_NOENT, || {
        format!("RENAME of a missing name: {miss}")
    })?;
    s.close()?;
    Ok(vec![
        line("directory over a file", st),
        line("missing source", miss),
    ])
}

fn rename_across(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "rename-across")?;
    let d = s.dir.clone();
    let x = s.n.mkdir(&d, b"x")?;
    let y = s.n.mkdir(&d, b"y")?;
    let f = s.n.create(&x, b"f", 0o644)?;
    let id = s.n.getattr(&f)?.fileid;
    ok(rename(&mut s.n, &x, b"f", &y, b"g")?, "RENAME x/f to y/g")?;
    let g = s.n.lookup(&y, b"g")?;
    check(s.n.getattr(&g)?.fileid == id, || {
        "another file arrived".into()
    })?;
    expect_err(
        &s.n.lookup_res(&x, b"f")?,
        nfsstat3::NFS3ERR_NOENT,
        "x/f after RENAME",
    )?;
    // A directory moves with its contents.
    let sub = s.n.mkdir(&x, b"sub")?;
    s.n.create(&sub, b"inner", 0o644)?;
    ok(
        rename(&mut s.n, &x, b"sub", &y, b"moved")?,
        "RENAME x/sub to y/moved",
    )?;
    let m = s.n.lookup(&y, b"moved")?;
    s.n.lookup(&m, b"inner")?;
    let up = s.n.lookup(&m, b"..")?;
    check(s.n.getattr(&up)?.fileid == s.n.getattr(&y)?.fileid, || {
        "`..` of the moved directory".into()
    })?;
    s.close()?;
    Ok(vec![line("file and directory across", "moved")])
}

fn rename_into_itself(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "rename-into-itself")?;
    let d = s.dir.clone();
    let p = s.n.mkdir(&d, b"p")?;
    let q = s.n.mkdir(&p, b"q")?;
    let mut out = Vec::new();
    for (what, to, name) in [
        ("into itself", &p, &b"self"[..]),
        ("into its child", &q, &b"deeper"[..]),
    ] {
        let st = status(&rename(&mut s.n, &d, b"p", to, name)?);
        check(st != nfsstat3::NFS3_OK, || {
            format!("RENAME of a directory {what} was done")
        })?;
        out.push(line(&format!("RENAME {what}"), st));
    }
    s.n.lookup(&d, b"p")?;
    s.close()?;
    Ok(out)
}

fn fill(n: &mut Nfs3, d: &[u8], count: u32) -> Result<Vec<Vec<u8>>, crate::Error> {
    let mut names = Vec::new();
    for i in 0..count {
        let name = format!("entry-{i:05}").into_bytes();
        let r = n.create_res(
            d,
            &name,
            nfs_v3::wire::createhow3::UNCHECKED(crate::v3::mode(0o644)),
        )?;
        ok(r, "CREATE")?;
        names.push(name);
    }
    Ok(names)
}

fn readdir(
    n: &mut Nfs3,
    d: &[u8],
    cookie: u64,
    verf: [u8; 8],
    count: u32,
) -> Result<READDIR3res<'static>, crate::Error> {
    n.call(
        P::NFSPROC3_READDIR,
        &READDIR3args {
            dir: fh(d),
            cookie,
            cookieverf: cookieverf3(verf),
            count,
        },
    )
}

/// One page: names with cookies, EOF, the verifier.
type Page = (Vec<(Vec<u8>, u64)>, bool, [u8; 8]);

/// Pages through `d`: every name with how often it came, and the page count.
fn pages(
    n: &mut Nfs3,
    d: &[u8],
    plus: bool,
) -> Result<(BTreeMap<Vec<u8>, u32>, u32), crate::Error> {
    let mut seen = BTreeMap::new();
    let (mut cookie, mut verf, mut pages) = (0u64, [0u8; 8], 0u32);
    loop {
        pages += 1;
        if pages > 100_000 {
            return fail("never reached the end");
        }
        let (entries, eof, v): Page = if plus {
            let r = ok(n.readdirplus(d, cookie, verf, 512, 4096)?, "READDIRPLUS")?;
            let e = r
                .reply
                .entries
                .0
                .iter()
                .map(|e| (e.name.0.to_vec(), e.cookie))
                .collect();
            (e, r.reply.eof, r.cookieverf.0)
        } else {
            let r = ok(readdir(n, d, cookie, verf, 1024)?, "READDIR")?;
            let e = r
                .reply
                .entries
                .0
                .iter()
                .map(|e| (e.name.0.to_vec(), e.cookie))
                .collect();
            (e, r.reply.eof, r.cookieverf.0)
        };
        if cookie != 0 && v != verf && verf != [0; 8] {
            return fail("the cookie verifier changed while paging");
        }
        verf = v;
        for (name, ck) in &entries {
            *seen.entry(name.clone()).or_insert(0) += 1;
            cookie = *ck;
        }
        if eof {
            return Ok((seen, pages));
        }
        if entries.is_empty() {
            return fail("an empty page that is not the end");
        }
    }
}

fn paging(c: &Ctx, plus: bool) -> Lines {
    let mut s = Session::open(c, if plus { "readdirplus" } else { "readdir" })?;
    let d = s.dir.clone();
    let names = fill(&mut s.n, &d, c.t.entries)?;
    let (seen, pages) = pages(&mut s.n, &d, plus)?;
    let twice: Vec<_> = seen.iter().filter(|(_, n)| **n > 1).collect();
    check(twice.is_empty(), || {
        format!("{} names came more than once", twice.len())
    })?;
    let missing = names.iter().filter(|n| !seen.contains_key(*n)).count();
    check(missing == 0, || format!("{missing} names never came"))?;
    check(
        seen.contains_key(&b"."[..]) && seen.contains_key(&b".."[..]),
        || "`.` or `..` missing".into(),
    )?;
    let extra = seen.len() - names.len() - 2;
    check(extra == 0, || format!("{extra} names nobody made"))?;
    s.close()?;
    Ok(vec![line("entries", names.len()), line("pages", pages)])
}

fn readdir_paging(c: &Ctx) -> Lines {
    paging(c, false)
}

fn readdirplus_paging(c: &Ctx) -> Lines {
    paging(c, true)
}

fn bad_cookie(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "bad-cookie")?;
    let d = s.dir.clone();
    fill(&mut s.n, &d, 200)?;
    let r = ok(readdir(&mut s.n, &d, 0, [0; 8], 1024)?, "READDIR")?;
    let last = r
        .reply
        .entries
        .0
        .last()
        .map(|e| e.cookie)
        .ok_or_else(|| crate::Error::Check("no entries".into()))?;
    let verf = r.cookieverf.0;
    if verf == [0; 8] {
        return Err(crate::Error::Skip(
            "the server's cookie verifier is zero (not checked)".into(),
        ));
    }
    let mangled: [u8; 8] = verf.map(|b| !b);
    let r = readdir(&mut s.n, &d, last, mangled, 1024)?;
    let st = status(&r);
    check(st == nfsstat3::NFS3ERR_BAD_COOKIE, || {
        format!("READDIR with a mangled verifier: {st}")
    })?;
    // The right verifier still pages on.
    if let Err(st) = crate::v3::split(readdir(&mut s.n, &d, last, verf, 1024)?) {
        return fail(format!("READDIR with the right verifier: {st}"));
    }
    s.close()?;
    Ok(vec![line("mangled verifier", st)])
}
