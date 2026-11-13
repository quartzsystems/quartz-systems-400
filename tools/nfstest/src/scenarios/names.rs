// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Names: the longest, one too long, one with `/`, bytes that are not UTF-8.

use nfs_v3::wire::{createhow3, nfsstat3};

use super::{Family, Lines, Scenario, line};
use crate::ctx::Ctx;
use crate::error::check;
use crate::v3::{Session, mode, ok, status};

/// This file's scenarios.
pub const LIST: [Scenario; 4] = [
    s("name-255", name_255),
    s("name-256", name_256),
    s("name-slash", name_slash),
    s("name-non-utf8", name_non_utf8),
];

const fn s(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario {
        name,
        family: Family::V3,
        run,
    }
}

fn name_255(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "name-255")?;
    let d = s.dir.clone();
    let name = vec![b'n'; 255];
    s.n.create(&d, &name, 0o644)?;
    s.n.lookup(&d, &name)?;
    let listed = s.n.list(&d)?;
    check(listed.iter().any(|(n, _, _)| *n == name), || {
        "not listed".into()
    })?;
    ok(s.n.remove_res(&d, &name)?, "REMOVE")?;
    s.close()?;
    Ok(vec![line("255 bytes", "made, found, listed, removed")])
}

fn name_256(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "name-256")?;
    let d = s.dir.clone();
    let name = vec![b'n'; 256];
    let st = status(&s.n.create_res(&d, &name, createhow3::GUARDED(mode(0o644)))?);
    check(st == nfsstat3::NFS3ERR_NAMETOOLONG, || {
        format!("CREATE: {st}")
    })?;
    let lk = status(&s.n.lookup_res(&d, &name)?);
    check(lk == nfsstat3::NFS3ERR_NAMETOOLONG, || {
        format!("LOOKUP: {lk}")
    })?;
    let mk = status(&s.n.mkdir_res(&d, &name, 0o755)?);
    check(mk == nfsstat3::NFS3ERR_NAMETOOLONG, || {
        format!("MKDIR: {mk}")
    })?;
    s.close()?;
    Ok(vec![line("CREATE, LOOKUP, MKDIR", "NFS3ERR_NAMETOOLONG")])
}

fn name_slash(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "name-slash")?;
    let d = s.dir.clone();
    s.n.mkdir(&d, b"a")?;
    let st = status(&s.n.create_res(&d, b"a/b", createhow3::GUARDED(mode(0o644)))?);
    check(st != nfsstat3::NFS3_OK, || {
        "a name with `/` was made".into()
    })?;
    let lk = status(&s.n.lookup_res(&d, b"a/b")?);
    check(lk != nfsstat3::NFS3_OK, || {
        "a name with `/` was found".into()
    })?;
    let empty = status(&s.n.create_res(&d, b"", createhow3::GUARDED(mode(0o644)))?);
    check(empty != nfsstat3::NFS3_OK, || {
        "an empty name was made".into()
    })?;
    s.close()?;
    Ok(vec![
        line("CREATE a/b", st),
        line("LOOKUP a/b", lk),
        line("CREATE of an empty name", empty),
    ])
}

fn name_non_utf8(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "name-non-utf8")?;
    let d = s.dir.clone();
    let name = b"bad-\xff\xfe-name".to_vec();
    let st = status(&s.n.create_res(&d, &name, createhow3::GUARDED(mode(0o644)))?);
    let mut out = vec![line("CREATE of bytes FF FE", st)];
    if st == nfsstat3::NFS3_OK {
        let listed = s.n.list(&d)?;
        let back: Vec<String> = listed.iter().map(|(n, _, _)| super::hex(n)).collect();
        out.push(line("listed as", back.join(",")));
        let lk = status(&s.n.lookup_res(&d, &name)?);
        out.push(line("LOOKUP of the same bytes", lk));
    }
    s.close()?;
    Ok(out)
}
