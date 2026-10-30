// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DEL`/`RMVLNK`, `REN`/`RNM`, `MOV`, `CPY`, `ADDLNK`. `DEL`, `MOV`
//! and `CPY` take a pattern (`*`, `?`) in the last component of `OBJ`.

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::ops::{CopyOpts, RenameMode};

use crate::display::{absolute, folds, is_pattern, join, matches, split};
use crate::msg::{Msg, Reply};

/// The paths `obj` names: itself, or with a pattern in its last
/// component every match in name order (none: `CPFA093`).
pub fn expand(c: &dyn IfsClient, obj: &str) -> Result<Vec<String>, Reply> {
    let cwd = c.getcwd().unwrap_or_else(|_| "/".into());
    let full = absolute(&cwd, obj);
    let (dir, last) = split(&full);
    if !is_pattern(&last) {
        return Ok(vec![obj.to_string()]);
    }
    if is_pattern(&dir) {
        return Err(Reply::fail(Msg::path("CPFA08C", obj)));
    }
    let fold = folds(&dir);
    let mut names: Vec<String> = c
        .read_dir(&dir)
        .map_err(|e| Reply::errno(e, &dir))?
        .into_iter()
        .map(|e| e.name)
        .filter(|n| matches(&last, n, fold))
        .collect();
    if names.is_empty() {
        return Err(Reply::fail(Msg::path("CPFA093", obj)));
    }
    names.sort();
    Ok(names.into_iter().map(|n| join(&dir, &n)).collect())
}

/// `DEL`, `RMVLNK`: each link removed is a `CPCA089`; when some fail
/// the escape is `CPFA0BD` with the counts.
pub fn del(c: &dyn IfsClient, obj: &str) -> Reply {
    let all = match expand(c, obj) {
        Ok(v) => v,
        Err(r) => return r,
    };
    if all.len() == 1 {
        return match c.unlink(&all[0]) {
            Ok(()) => Reply::done(Msg::path("CPCA089", &all[0])),
            Err(e) => Reply::errno(e, &all[0]),
        };
    }
    let mut r = Reply::default();
    let (mut ok, mut bad) = (0i64, 0i64);
    for p in &all {
        match c.unlink(p) {
            Ok(()) => {
                ok += 1;
                r = r.with(crate::msg::COMPLETION, Msg::path("CPCA089", p));
            }
            Err(e) => {
                bad += 1;
                r = r.diag(Msg::errno(e, p));
            }
        }
    }
    if bad > 0 {
        r.escape = Some(Msg::new("CPFA0BD").bin4(ok).bin4(bad));
    }
    r
}

/// `REN`, `RNM`: `new` is a name in the same directory.
pub fn ren(c: &dyn IfsClient, obj: &str, new: &str) -> Reply {
    let new = new.trim();
    if new.starts_with('/') {
        return Reply::fail(Msg::path("CPFA0B4", new));
    }
    if new.contains('/') {
        return Reply::fail(Msg::path("CPFA0B5", new));
    }
    let cwd = c.getcwd().unwrap_or_else(|_| "/".into());
    let (dir, _) = split(&absolute(&cwd, obj));
    let to = join(if dir.is_empty() { "/" } else { &dir }, new);
    match c.rename(obj, &to, RenameMode::Keep) {
        Ok(()) => Reply::done(Msg::paths("CPCA084", &[obj, &to])),
        Err(e) => Reply::errno(e, obj),
    }
}

/// Where `MOV` and `CPY` put an object: `TOOBJ`, else `TODIR` and the
/// object's name.
fn target(from: &str, todir: &str, toobj: Option<&str>) -> String {
    if let Some(t) = toobj { t.to_string() } else {
        let (_, last) = split(from.trim_end_matches('/'));
        join(if todir.is_empty() { "." } else { todir }, &last)
    }
}

/// The `TODIR`/`TOOBJ` checks both commands make.
fn to_checks(obj: &str, todir: &str, toobj: Option<&str>, many: bool, copy: bool) -> Option<Reply> {
    if toobj.is_some() && !todir.is_empty() && todir != "." {
        return Some(Reply::fail(Msg::new("CPFA0B3")));
    }
    if toobj.is_some() && (many || is_pattern(obj)) {
        return Some(if copy {
            Reply::fail(Msg::path("CPFA0B6", obj))
        } else {
            Reply::fail(Msg::path("CPFA089", obj))
        });
    }
    None
}

/// `MOV`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mov {
    /// `OBJ`.
    pub obj: String,
    /// `TODIR` (`.` by default).
    pub todir: String,
    /// `TOOBJ`.
    pub toobj: Option<String>,
    /// `DTAFMT(*TEXT)` with `TOCCSID`: convert as it moves.
    pub to_ccsid: Option<u16>,
}

/// `MOV`.
pub fn mov(c: &dyn IfsClient, a: &Mov) -> Reply {
    let all = match expand(c, &a.obj) {
        Ok(v) => v,
        Err(r) => return r,
    };
    if let Some(r) = to_checks(&a.obj, &a.todir, a.toobj.as_deref(), all.len() > 1, false) {
        return r;
    }
    let one = |from: &str| -> Result<String, Errno> {
        let to = target(from, &a.todir, a.toobj.as_deref());
        match a.to_ccsid {
            Some(ccsid) => {
                c.copy(
                    from,
                    &to,
                    CopyOpts {
                        subtree: true,
                        replace: false,
                        to_ccsid: Some(ccsid),
                        keep_owner: true,
                    },
                )?;
                match c.stat(from, false) {
                    Ok(s) if s.objtype == "*DIR" => c.remove_tree(from)?,
                    _ => c.unlink(from)?,
                }
            }
            None => c.mov(from, &to, false)?,
        }
        Ok(to)
    };
    if all.len() == 1 {
        return match one(&all[0]) {
            Ok(to) => Reply::done(Msg::paths("CPCA086", &[&all[0], &to])),
            Err(e) => Reply::errno(e, &all[0]),
        };
    }
    let mut r = Reply::default();
    let (mut ok, mut bad) = (0u64, 0u64);
    for p in &all {
        match one(p) {
            Ok(to) => {
                ok += 1;
                r = r.with(crate::msg::COMPLETION, Msg::paths("CPCA086", &[p, &to]));
            }
            Err(e) => {
                bad += 1;
                r = r.diag(Msg::errno(e, p));
            }
        }
    }
    if bad > 0 {
        r.escape = Some(Msg::new("CPFA0B8").bin2(0).bin2(0).u64(ok).u64(bad));
    }
    r
}

/// `CPY`'s `SUBTREE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CpySubtree {
    /// `*NODIR`: a directory is not copied.
    #[default]
    NoDir,
    /// `*NONE`: a directory without its contents.
    None,
    /// `*ALL`: a directory and everything under it.
    All,
}

/// `CPY`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Cpy {
    /// `OBJ`.
    pub obj: String,
    /// `TODIR` (`.` by default).
    pub todir: String,
    /// `TOOBJ`.
    pub toobj: Option<String>,
    /// `SUBTREE`.
    pub subtree: CpySubtree,
    /// `REPLACE(*YES)`.
    pub replace: bool,
    /// `OWNER(*KEEP)`.
    pub keep_owner: bool,
    /// `DTAFMT(*TEXT)` with `TOCCSID`.
    pub to_ccsid: Option<u16>,
}

/// `CPY`: one object is `CPCA087`; several are `CPCA082` with the
/// count, or the escape `CPFA0BB` with both counts when some fail.
pub fn cpy(c: &dyn IfsClient, a: &Cpy) -> Reply {
    let all = match expand(c, &a.obj) {
        Ok(v) => v,
        Err(r) => return r,
    };
    if let Some(r) = to_checks(&a.obj, &a.todir, a.toobj.as_deref(), all.len() > 1, true) {
        return r;
    }
    let one = |from: &str| -> Result<String, Errno> {
        let to = target(from, &a.todir, a.toobj.as_deref());
        if a.subtree == CpySubtree::NoDir && c.stat(from, true)?.objtype == "*DIR" {
            return Err(Errno::EISDIR);
        }
        c.copy(
            from,
            &to,
            CopyOpts {
                subtree: a.subtree == CpySubtree::All,
                replace: a.replace,
                to_ccsid: a.to_ccsid,
                keep_owner: a.keep_owner,
            },
        )?;
        Ok(to)
    };
    if all.len() == 1 {
        return match one(&all[0]) {
            Ok(to) => Reply::done(Msg::paths("CPCA087", &[&all[0], &to])),
            Err(e) => Reply::fail(Msg::paths("CPFA097", &[&all[0], &target(&all[0], &a.todir, a.toobj.as_deref())]))
                .diag(Msg::errno(e, &all[0])),
        };
    }
    let mut r = Reply::default();
    let (mut ok, mut bad) = (0i64, 0i64);
    for p in &all {
        match one(p) {
            Ok(_) => ok += 1,
            Err(e) => {
                bad += 1;
                r = r.diag(Msg::errno(e, p));
            }
        }
    }
    if bad > 0 {
        r.escape = Some(Msg::new("CPFA0BB").bin4(ok).bin4(bad));
        return r;
    }
    let to = if a.todir.is_empty() { "." } else { &a.todir };
    r.with(crate::msg::COMPLETION, Msg::paths("CPCA082", &[&a.obj, to]).bin4(ok))
}

/// `ADDLNK`: a symbolic link holding `obj`'s text, or a hard link.
pub fn addlnk(c: &dyn IfsClient, obj: &str, newlnk: &str, hard: bool) -> Reply {
    let r = if hard {
        c.link(obj, newlnk)
    } else {
        c.symlink(obj, newlnk)
    };
    match r {
        Ok(()) => Reply::done(Msg::paths("CPCA088", &[obj, newlnk])),
        Err(e) => Reply::errno(e, if hard { obj } else { newlnk }),
    }
}
