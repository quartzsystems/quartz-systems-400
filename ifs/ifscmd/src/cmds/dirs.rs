// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CD`/`CHGCURDIR`, `MD`/`MKDIR`/`CRTDIR`, `RD`/`RMDIR`.

use ifs::Errno;
use ifs::auth::data::DataAuth;
use ifs::client::IfsClient;
use ifs::meta::{CrtObjAud, ObjAudit, Scan};
use ifs::ops::{Attr, ChgAut, Who};
use sec::auth::bits::Auth;

use super::Subtree;
use crate::display::{absolute, split};
use crate::msg::{Msg, Reply};

/// `CD DIR(path)`: the job's current directory.
pub fn cd(c: &dyn IfsClient, dir: &str) -> Reply {
    match c.chdir(dir) {
        Ok(()) => {
            let now = c.getcwd().unwrap_or_else(|_| dir.to_string());
            Reply::done(Msg::path("CPCA08B", &now))
        }
        Err(e) => Reply::errno(e, dir),
    }
}

/// `MD`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Md {
    /// `DIR`.
    pub dir: String,
    /// `DTAAUT`: `*INDIR` or a data authority for `*PUBLIC`.
    pub dtaaut: String,
    /// `OBJAUT`: `*INDIR`, `*NONE`, `*ALL` or object authorities.
    pub objaut: Vec<String>,
    /// `CRTOBJAUD`.
    pub crtobjaud: String,
    /// `CRTOBJSCAN`: `*PARENT`, `*YES`, `*NO`, `*CHGONLY`.
    pub crtobjscan: String,
    /// `RSTDRNMUNL(*YES)`.
    pub rstdrnmunl: bool,
}

impl Md {
    /// A directory with every default.
    #[must_use]
    pub fn new(dir: &str) -> Self {
        Self {
            dir: dir.to_string(),
            dtaaut: "*INDIR".into(),
            objaut: vec!["*INDIR".into()],
            crtobjaud: "*SYSVAL".into(),
            crtobjscan: "*PARENT".into(),
            rstdrnmunl: false,
        }
    }
}

/// The object authorities `OBJAUT` names; `None` for `*INDIR`.
pub fn objaut(words: &[String]) -> Result<Option<Auth>, Msg> {
    let words: Vec<&str> = words
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |w: &str| words.iter().any(|x| x.eq_ignore_ascii_case(w));
    if words.is_empty() || has("*INDIR") || has("*SAME") {
        if words.len() > 1 {
            return Err(Msg::new(if has("*INDIR") { "CPFA0B9" } else { "CPFA0C3" }));
        }
        return Ok(None);
    }
    if has("*NONE") {
        return Ok(Some(Auth::EXCLUDE));
    }
    if has("*ALL") {
        return Ok(Some(Auth::parse(&["*OBJEXIST", "*OBJMGT", "*OBJALTER", "*OBJREF"], false).unwrap_or_default()));
    }
    Auth::parse(&words, false)
        .map(Some)
        .map_err(|_| Msg::new("CPDA091"))
}

/// An auditing value word.
fn audit_word(w: &str) -> Option<CrtObjAud> {
    Some(match w.trim().to_ascii_uppercase().as_str() {
        "*SYSVAL" => CrtObjAud::SysVal,
        "*NONE" => CrtObjAud::Value(ObjAudit::None),
        "*USRPRF" => CrtObjAud::Value(ObjAudit::UsrPrf),
        "*CHANGE" => CrtObjAud::Value(ObjAudit::Change),
        "*ALL" => CrtObjAud::Value(ObjAudit::All),
        _ => return None,
    })
}

/// A scan word; `None` for `*PARENT`.
fn scan_word(w: &str) -> Option<Scan> {
    match w.trim().to_ascii_uppercase().as_str() {
        "*YES" => Some(Scan::Yes),
        "*NO" => Some(Scan::No),
        "*CHGONLY" => Some(Scan::ChgOnly),
        _ => None,
    }
}

/// `MD`, `MKDIR`, `CRTDIR`.
pub fn md(c: &dyn IfsClient, a: &Md) -> Reply {
    let indir = a.dtaaut.trim().eq_ignore_ascii_case("*INDIR");
    let data = if indir {
        None
    } else {
        match DataAuth::parse(a.dtaaut.trim()) {
            Some(d) => Some(d),
            None => return Reply::fail(Msg::new("CPDA091")),
        }
    };
    let objects = match objaut(&a.objaut) {
        Ok(o) => o,
        Err(m) => return Reply::fail(m),
    };
    let Some(audit) = audit_word(&a.crtobjaud) else {
        return Reply::fail(Msg::new("CPFA0CB").chars(&a.crtobjaud, 10));
    };
    // The parent's mode for *INDIR: the new directory's data authorities.
    let cwd = c.getcwd().unwrap_or_else(|_| "/".into());
    let (parent, _) = split(&absolute(&cwd, &a.dir));
    let parent = if parent.is_empty() { "/".to_string() } else { parent };
    let mode = if indir {
        c.stat(&parent, true).map_or(0o755, |s| s.mode & 0o777)
    } else {
        0o700
    };
    if let Err(e) = c.mkdir(&a.dir, mode) {
        return Reply::errno(e, &a.dir);
    }
    let after = || -> Result<(), Errno> {
        if indir {
            c.chmod(&a.dir, mode)?;
        }
        if data.is_some() || objects.is_some() {
            c.chgaut(
                &a.dir,
                &ChgAut {
                    who: vec![Who::Public],
                    data,
                    data_autl: false,
                    objects,
                    autl: None,
                },
            )?;
        }
        if audit != CrtObjAud::SysVal {
            c.set_attr(&a.dir, &Attr::Crtobjaud(audit), false)?;
        }
        if let Some(s) = scan_word(&a.crtobjscan) {
            c.set_attr(&a.dir, &Attr::Crtobjscan(s), false)?;
        }
        if a.rstdrnmunl {
            c.set_attr(&a.dir, &Attr::Rstdrnmunl(true), false)?;
        }
        Ok(())
    };
    match after() {
        Ok(()) => Reply::done(Msg::path("CPCA083", &a.dir)),
        // Made, but not as asked: the directory stays, the escape says so.
        Err(e) => Reply::fail(Msg::new("CPFA0AF")).diag(Msg::errno(e, &a.dir)),
    }
}

/// `RD`, `RMDIR`: `SUBTREE(*ALL)` removes everything under it,
/// `RMVLNK(*YES)` the links in it that are not directories.
pub fn rd(c: &dyn IfsClient, dir: &str, subtree: Subtree, rmvlnk: bool) -> Reply {
    if subtree == Subtree::All && rmvlnk {
        return Reply::fail(Msg::new("CPDA0B8"));
    }
    let r = match (subtree, rmvlnk) {
        (Subtree::All, _) => c.remove_tree(dir),
        (_, true) => c.read_dir(dir).and_then(|ents| {
            for e in ents {
                let p = crate::display::join(dir, &e.name);
                if c.stat(&p, false).is_ok_and(|s| s.objtype != "*DIR") {
                    c.unlink(&p)?;
                }
            }
            c.rmdir(dir)
        }),
        _ => c.rmdir(dir),
    };
    match r {
        Ok(()) => Reply::done(Msg::path("CPCA085", dir)),
        Err(e) => Reply::errno(e, dir),
    }
}
