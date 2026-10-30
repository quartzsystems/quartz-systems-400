// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CHGATR`, `CHKIN`, `CHKOUT`, and `STRJRN`/`ENDJRN` on IFS objects
//! (the journaling attributes are stored; no entries are written, as
//! `docs/IFS.md` says).

use ifs::client::IfsClient;
use ifs::meta::{CrtObjAud, Journal, ObjAudit, Scan, StgOpt};
use ifs::ops::Attr;

use super::{Subtree, each};
use crate::msg::{Msg, Reply};

/// `CHGATR ATR` as the definition passes it (`*READONLY` 1 ...
/// `*CRTRUNEXIT` 21).
pub mod atr {
    #![allow(missing_docs)]
    pub const READONLY: i64 = 1;
    pub const HIDDEN: i64 = 2;
    pub const PCSYSTEM: i64 = 3;
    pub const PCARCHIVE: i64 = 4;
    pub const SYSARCHIVE: i64 = 5;
    pub const CCSID: i64 = 6;
    pub const ALWCKPWRT: i64 = 7;
    pub const USECOUNT: i64 = 8;
    pub const DISKSTGOPT: i64 = 9;
    pub const MAINSTGOPT: i64 = 10;
    pub const CRTOBJSCAN: i64 = 11;
    pub const SCAN: i64 = 12;
    pub const ALWSAV: i64 = 13;
    pub const RSTDRNMUNL: i64 = 14;
    pub const SETUID: i64 = 15;
    pub const SETGID: i64 = 16;
    pub const CRTOBJAUD: i64 = 17;
    pub const INHCKPWRT: i64 = 18;
    pub const TEXT: i64 = 19;
    pub const RUNEXIT: i64 = 20;
    pub const CRTRUNEXIT: i64 = 21;
}

/// `CHGATR VALUE`'s special values as the definition passes them.
pub mod val {
    #![allow(missing_docs)]
    pub const YES: i64 = -1;
    pub const NO: i64 = -2;
    pub const RESET: i64 = -3;
    pub const NORMAL: i64 = -4;
    pub const MINIMIZE: i64 = -5;
    pub const DYNAMIC: i64 = -6;
    pub const CHGONLY: i64 = -7;
    pub const SYSVAL: i64 = -8;
    pub const NONE: i64 = -9;
    pub const USRPRF: i64 = -10;
    pub const ALL: i64 = -11;
    pub const CHANGE: i64 = -12;
}

/// The attribute `CHGATR` sets; `Err` is the refusal.
fn attribute(atr: i64, value: i64, text: &str) -> Result<Option<Attr>, Msg> {
    let bad = || Msg::new("CPDA03D");
    let flag = |v: i64| match v {
        val::YES => Ok(true),
        val::NO => Ok(false),
        _ => Err(bad()),
    };
    let stg = |v: i64| match v {
        val::NORMAL => Ok(StgOpt::Normal),
        val::MINIMIZE => Ok(StgOpt::Minimize),
        val::DYNAMIC => Ok(StgOpt::Dynamic),
        _ => Err(bad()),
    };
    let scan = |v: i64| match v {
        val::YES => Ok(Scan::Yes),
        val::NO => Ok(Scan::No),
        val::CHGONLY => Ok(Scan::ChgOnly),
        _ => Err(bad()),
    };
    if atr == atr::TEXT {
        if value != val::NONE {
            return Err(Msg::new("CPDA0C2"));
        }
    } else if !text.is_empty() && !text.eq_ignore_ascii_case("*BLANK") {
        return Err(Msg::new("CPDA0C1"));
    }
    Ok(Some(match atr {
        atr::READONLY => Attr::PcReadOnly(flag(value)?),
        atr::HIDDEN => Attr::PcHidden(flag(value)?),
        atr::PCSYSTEM => Attr::PcSystem(flag(value)?),
        atr::PCARCHIVE => Attr::PcArchive(flag(value)?),
        atr::SYSARCHIVE => Attr::SystemArchive(flag(value)?),
        atr::CCSID => match u16::try_from(value) {
            Ok(n) if n > 0 => Attr::Ccsid(n),
            _ => return Err(Msg::new("CPFA0C1").bin4(value)),
        },
        atr::ALWCKPWRT => Attr::Alwckpwrt(flag(value)?),
        // The usage count has no store of its own: a reset is accepted.
        atr::USECOUNT if value == val::RESET => return Ok(None),
        atr::DISKSTGOPT => Attr::DiskStgOpt(stg(value)?),
        atr::MAINSTGOPT => Attr::MainStgOpt(stg(value)?),
        atr::CRTOBJSCAN => Attr::Crtobjscan(scan(value)?),
        atr::SCAN => Attr::Scan(scan(value)?),
        atr::ALWSAV => Attr::Alwsav(flag(value)?),
        atr::RSTDRNMUNL => Attr::Rstdrnmunl(flag(value)?),
        atr::SETUID => Attr::Suid(flag(value)?),
        atr::SETGID => Attr::Sgid(flag(value)?),
        atr::CRTOBJAUD => Attr::Crtobjaud(match value {
            val::SYSVAL => CrtObjAud::SysVal,
            val::NONE => CrtObjAud::Value(ObjAudit::None),
            val::USRPRF => CrtObjAud::Value(ObjAudit::UsrPrf),
            val::CHANGE => CrtObjAud::Value(ObjAudit::Change),
            val::ALL => CrtObjAud::Value(ObjAudit::All),
            _ => return Err(bad()),
        }),
        atr::INHCKPWRT => Attr::InheritAlwckpwrt(flag(value)?),
        atr::TEXT => Attr::Text(if text.eq_ignore_ascii_case("*BLANK") {
            String::new()
        } else {
            text.to_string()
        }),
        atr::RUNEXIT | atr::CRTRUNEXIT => {
            let name = if atr == atr::RUNEXIT { "*RUNEXIT" } else { "*CRTRUNEXIT" };
            return Err(Msg::new("CPFA0ED").chars(name, 10));
        }
        _ => return Err(Msg::new("CPDA03C").bin2(atr)),
    }))
}

/// `CHGATR`.
pub fn chgatr(c: &dyn IfsClient, obj: &str, atr: i64, value: i64, text: &str, subtree: Subtree, symlnk: bool) -> Reply {
    let a = match attribute(atr, value, text.trim()) {
        Ok(Some(a)) => a,
        Ok(None) => return Reply::text("Attribute changed.\n"),
        Err(m) => return Reply::fail(m),
    };
    each(
        c,
        obj,
        subtree,
        !symlnk,
        |p| c.set_attr(p, &a, !symlnk),
        |n| {
            let s = if n == 1 { "" } else { "s" };
            Reply::text(format!("Attribute changed for {n} object{s}.\n"))
        },
    )
}

/// `CHKOUT` (`out`) and `CHKIN`: one object is `CPCA08F` or `CPCA08A`;
/// with `SUBTREE(*ALL)` failures end with `CPFA0BF` or `CPFA0BE` and
/// the counts.
pub fn checkout(c: &dyn IfsClient, obj: &str, subtree: Subtree, out: bool) -> Reply {
    let comp = if out { "CPCA08F" } else { "CPCA08A" };
    if subtree == Subtree::None {
        let r = if out { c.check_out(obj) } else { c.check_in(obj) };
        return match r {
            Ok(()) => Reply::done(Msg::path(comp, obj)),
            Err(e) => Reply::errno(e, obj),
        };
    }
    let all = match super::targets(c, obj, subtree, true) {
        Ok(v) => v,
        Err(e) => return Reply::errno(e, obj),
    };
    let mut r = Reply::default();
    let (mut ok, mut bad) = (0u64, 0u64);
    for p in &all {
        // Directories are not checked out; what is in them is.
        if c.stat(p, true).is_ok_and(|s| s.objtype == "*DIR") {
            continue;
        }
        let x = if out { c.check_out(p) } else { c.check_in(p) };
        match x {
            Ok(()) => ok += 1,
            Err(e) => {
                bad += 1;
                r = r.diag(Msg::errno(e, p));
            }
        }
    }
    if bad > 0 {
        let id = if out { "CPFA0BF" } else { "CPFA0BE" };
        r.escape = Some(Msg::new(id).bin2(0).bin2(0).u64(ok).u64(bad));
        return r;
    }
    r.with(crate::msg::COMPLETION, Msg::path(comp, obj))
}

/// `STRJRN` on IFS objects.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StrJrn {
    /// `OBJ`: the paths.
    pub objs: Vec<String>,
    /// `JRN` as `LIB/NAME`.
    pub jrn: String,
    /// `SUBTREE`.
    pub subtree: Subtree,
    /// `INHERIT(*YES)`.
    pub inherit: bool,
    /// `IMAGES(*BOTH)`.
    pub both: bool,
    /// `OMTJRNE(*OPNCLO)`.
    pub omit_open_close: bool,
}

/// `STRJRN`: the attributes stored on each object.
pub fn strjrn(c: &dyn IfsClient, a: &StrJrn) -> Reply {
    let j = Journal {
        journal: a.jrn.to_ascii_uppercase(),
        images: u8::from(a.both),
        omit_open_close: a.omit_open_close,
        inherit: a.inherit,
        ..Journal::default()
    };
    journal_each(c, &a.objs, a.subtree, "started", |p| c.start_journal(p, j.clone()))
}

/// `ENDJRN` on IFS objects.
pub fn endjrn(c: &dyn IfsClient, objs: &[String], subtree: Subtree) -> Reply {
    journal_each(c, objs, subtree, "ended", |p| c.end_journal(p))
}

fn journal_each(
    c: &dyn IfsClient,
    objs: &[String],
    subtree: Subtree,
    what: &str,
    mut f: impl FnMut(&str) -> ifs::Result<()>,
) -> Reply {
    let mut total = 0;
    for o in objs {
        let r = each(c, o, subtree, true, &mut f, |n| Reply::text(n.to_string()));
        if r.failed() {
            return r;
        }
        total += r.text.parse::<usize>().unwrap_or(0);
    }
    let s = if total == 1 { "" } else { "s" };
    Reply::text(format!(
        "Journaling {what} for {total} object{s}; IFS journal entries are not written in this release.\n"
    ))
}
