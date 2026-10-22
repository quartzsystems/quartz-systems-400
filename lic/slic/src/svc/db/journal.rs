// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Journal, commitment control and override commands.

use std::fmt::Write as _;

use db::cmt::LckLvl;
use db::odp::{Override, Position};
use dds::FileRef;
use jrn::journal::{CrtJrnOpts, Journal, NewReceiver};
use jrn::rcv::CrtJrnRcvOpts;
use jrn::{Filter, JrnError};
use qsname::{MemberSpec, Name};
use sql::SqlContext;

use super::{JOB, Params, db, db_err, file_id, now, object_name, typed_id, with_txn};
use crate::ipl::system::System;

fn jrn_err(e: &JrnError) -> String {
    match e.msgid() {
        Some(id) => format!("{id} {e}"),
        None => e.to_string(),
    }
}

/// `crtjrnrcv LIB/RCV [THRESHOLD(kb)] [TEXT('..')]`.
pub fn crtjrnrcv(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "JRNRCV")?;
    let name = object_name(&qual)?;
    let opts = CrtJrnRcvOpts {
        text: p.text("TEXT"),
        threshold_kb: p.num("THRESHOLD")?.unwrap_or(0),
        now: now(),
        ..CrtJrnRcvOpts::default()
    };
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let lib = ctx.library_for(txn, &name).map_err(|e| e.to_string())?;
        let n = SqlContext::sys_name(&name.name).map_err(|e| e.to_string())?;
        jrn::rcv::crtjrnrcv(txn, lib, n, &opts).map_err(|e| jrn_err(&e))?;
        Ok(format!("Journal receiver {qual} created."))
    })
}

/// `crtjrn LIB/JRN JRNRCV(LIB/RCV) [MNGRCV(*USER)] [DLTRCV(*YES)] [TEXT('..')]`.
pub fn crtjrn(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "JRN")?;
    let name = object_name(&qual)?;
    let rcv = p.pos_or(1, "JRNRCV")?;
    let opts = CrtJrnOpts {
        text: p.text("TEXT"),
        mngrcv_system: p.upper("MNGRCV").as_deref() != Some("*USER"),
        dltrcv: p.yes("DLTRCV"),
        now: now(),
        ..CrtJrnOpts::default()
    };
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let lib = ctx.library_for(txn, &name).map_err(|e| e.to_string())?;
        let n = SqlContext::sys_name(&name.name).map_err(|e| e.to_string())?;
        let rcv_id = typed_id(txn, ctx, &rcv, jrn::JRNRCV)?;
        jrn::journal::crtjrn(txn, lib, n, rcv_id, &opts).map_err(|e| jrn_err(&e))?;
        Ok(format!(
            "Journal {qual} created with receiver {rcv} attached."
        ))
    })
}

/// `chgjrn LIB/JRN [JRNRCV(*GEN|LIB/RCV)]`.
pub fn chgjrn(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "JRN")?;
    let rcv = p.upper("JRNRCV").unwrap_or_else(|| "*GEN".into());
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let jid = typed_id(txn, ctx, &qual, jrn::JRN)?;
        let new = if rcv == "*GEN" {
            NewReceiver::Gen
        } else {
            NewReceiver::Named(typed_id(txn, ctx, &rcv, jrn::JRNRCV)?)
        };
        let mut j = Journal::load(txn, jid).map_err(|e| jrn_err(&e))?;
        let id = j.chgjrn(txn, new, now()).map_err(|e| jrn_err(&e))?;
        let r = jrn::Receiver::load(txn, id).map_err(|e| jrn_err(&e))?;
        Ok(format!(
            "Journal {qual} now has receiver {} attached.",
            r.name()
        ))
    })
}

/// `dltjrn LIB/JRN`.
pub fn dltjrn(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "JRN")?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let jid = typed_id(txn, ctx, &qual, jrn::JRN)?;
        jrn::journal::dltjrn(txn, JOB, jid, now()).map_err(|e| jrn_err(&e))?;
        Ok(format!("Journal {qual} deleted."))
    })
}

/// `dltjrnrcv LIB/RCV`.
pub fn dltjrnrcv(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "JRNRCV")?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = typed_id(txn, ctx, &qual, jrn::JRNRCV)?;
        jrn::rcv::dltjrnrcv(txn, JOB, id).map_err(|e| jrn_err(&e))?;
        Ok(format!("Journal receiver {qual} deleted."))
    })
}

/// `dspjrnrcva LIB/RCV`.
pub fn dspjrnrcva(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "JRNRCV")?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = typed_id(txn, ctx, &qual, jrn::JRNRCV)?;
        jrn::dspjrnrcva(txn, id).map_err(|e| jrn_err(&e))
    })
}

/// `strjrnpf LIB/FILE JRN(LIB/JRN) [IMAGES(*BOTH)] [OMTJRNE(*OPNCLO)]`.
pub fn strjrnpf(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let jrn = p.pos_or(1, "JRN")?;
    let both = p.upper("IMAGES").as_deref() == Some("*BOTH");
    let omit = p.upper("OMTJRNE").as_deref() == Some("*OPNCLO");
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        let jid = typed_id(txn, ctx, &jrn, jrn::JRN)?;
        db::journal::strjrnpf(txn, JOB, id, jid, both, omit, now()).map_err(|e| db_err(&e))?;
        Ok(format!("Journaling of {qual} started to {jrn}."))
    })
}

/// `endjrnpf LIB/FILE`.
pub fn endjrnpf(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "FILE")?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let id = file_id(txn, ctx, &qual)?;
        db::journal::endjrnpf(txn, JOB, id, now()).map_err(|e| db_err(&e))?;
        Ok(format!("Journaling of {qual} ended."))
    })
}

/// The filter `FILE`, `MBR`, `JRNCDE`, `ENTTYP`, `FROMENT`, `TOENT` and
/// `JOB` describe.
fn filter(p: &Params) -> Result<Filter, String> {
    let mut f = Filter {
        from_seq: p.num("FROMENT")?,
        to_seq: p.num("TOENT")?,
        job: p.num("JOB")?,
        ..Filter::default()
    };
    if let Some(file) = p.kw("FILE") {
        let name = object_name(file)?;
        let lib = name
            .schema
            .as_ref()
            .map(|s| Name::parse(&s.value).map_err(|e| e.to_string()))
            .transpose()?;
        let mbr = p
            .upper("MBR")
            .map(|m| Name::parse(&m).map_err(|e| e.to_string()))
            .transpose()?;
        f.object = Some((
            lib,
            Name::parse(&name.name.value).map_err(|e| e.to_string())?,
            mbr,
        ));
    }
    if let Some(codes) = p.upper("JRNCDE") {
        f.codes = codes
            .split_whitespace()
            .filter_map(|c| c.bytes().next())
            .collect();
    }
    if let Some(types) = p.upper("ENTTYP") {
        f.types = types
            .split_whitespace()
            .filter(|t| t.len() == 2)
            .map(|t| [t.as_bytes()[0], t.as_bytes()[1]])
            .collect();
    }
    Ok(f)
}

/// `dspjrn LIB/JRN [FILE(LIB/F) MBR(M) JRNCDE(R) ENTTYP(PT) FROMENT(n) TOENT(n) JOB(n)]`.
pub fn dspjrn(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "JRN")?;
    let f = filter(p)?;
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let jid = typed_id(txn, ctx, &qual, jrn::JRN)?;
        let entries = jrn::dspjrn(txn, jid, &f).map_err(|e| jrn_err(&e))?;
        let mut out = jrn::render(&entries);
        let _ = writeln!(out, "{} entries", entries.len());
        Ok(out)
    })
}

/// `apyjrnchg` (`apply`) or `rmvjrnchg` `LIB/JRN [FILE(LIB/F) FROMENT(n) TOENT(n) CMTBDY(*YES)]`.
pub fn apply(system: &mut System, p: &Params, forward: bool) -> Result<String, String> {
    let qual = p.pos_or(0, "JRN")?;
    let range = db::journal::JrnRange {
        from_seq: p.num("FROMENT")?,
        to_seq: p.num("TOENT")?,
        ..db::journal::JrnRange::default()
    };
    let cmtbdy = p.yes("CMTBDY");
    let file = p.kw("FILE").map(str::to_string);
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let jid = typed_id(txn, ctx, &qual, jrn::JRN)?;
        let files = match &file {
            Some(f) => Some(vec![file_id(txn, ctx, f)?]),
            None => None,
        };
        let r = if forward {
            db::journal::apyjrnchg(txn, JOB, jid, files.as_deref(), &range, cmtbdy, now())
        } else {
            db::journal::rmvjrnchg(txn, JOB, jid, files.as_deref(), &range, cmtbdy, now())
        }
        .map_err(|e| db_err(&e))?;
        Ok(format!(
            "{} entries {}, {} skipped{}.",
            r.applied,
            if forward { "applied" } else { "removed" },
            r.skipped,
            r.range
                .map_or(String::new(), |(a, b)| format!(" (sequence {a} to {b})"))
        ))
    })
}

/// `strcmtctl [LCKLVL(*CHG|*CS|*ALL)] [DFTJRN(LIB/JRN)]`.
pub fn strcmtctl(system: &mut System, p: &Params) -> Result<String, String> {
    let lcklvl = match p.upper("LCKLVL").as_deref() {
        None | Some("*CHG") => LckLvl::Chg,
        Some("*CS") => LckLvl::Cs,
        Some("*ALL") => LckLvl::All,
        Some(x) => return Err(format!("LCKLVL({x}) is not *CHG, *CS or *ALL")),
    };
    let dft = p.kw("DFTJRN").map(str::to_string);
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let dft_jrn = match &dft {
            Some(j) => Some(typed_id(txn, ctx, j, jrn::JRN)?),
            None => None,
        };
        ctx.session
            .strcmtctl(txn, lcklvl, dft_jrn, now())
            .map_err(|e| db_err(&e))?;
        Ok(
            format!("Commitment control started with LCKLVL({lcklvl:?}).")
                .to_ascii_uppercase()
                .replace("COMMITMENT CONTROL STARTED", "Commitment control started"),
        )
    })
}

/// `commit`.
pub fn commit(system: &mut System) -> Result<String, String> {
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let cycle = ctx.session.commit(txn, None).map_err(|e| db_err(&e))?;
        sql::exec::cursor::end_of_unit(ctx);
        Ok(if cycle == 0 {
            "Nothing to commit.".to_string()
        } else {
            format!("Commit cycle {cycle} committed.")
        })
    })
}

/// `rollbck`.
pub fn rollbck(system: &mut System) -> Result<String, String> {
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let r = ctx.session.rollbck(txn, None).map_err(|e| db_err(&e))?;
        sql::exec::cursor::end_of_unit(ctx);
        Ok(format!("Rolled back: {} record changes undone.", r.applied))
    })
}

/// `endcmtctl`.
pub fn endcmtctl(system: &mut System) -> Result<String, String> {
    let (store, ctx) = db(system)?;
    with_txn(store, |txn| {
        let r = ctx.session.endcmtctl(txn, None).map_err(|e| db_err(&e))?;
        Ok(format!(
            "Commitment control ended; {} record changes undone.",
            r.applied
        ))
    })
}

/// `CALLLVL(n)`: the call level the job says issued the command; 0,
/// command level, when typed at a console.
pub fn call_level(p: &Params) -> u32 {
    p.kw("CALLLVL")
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0)
}

/// `ovrdbf FILE(F) [TOFILE(LIB/F) MBR(M) SHARE(*YES) LVLCHK(*NO) INHWRT(*YES)
/// POSITION(*START|*END|*RRN n) SECURE(*YES) OVRSCOPE(*JOB) CALLLVL(n)]`.
pub fn ovrdbf(system: &mut System, p: &Params) -> Result<String, String> {
    let file = p.pos_or(0, "FILE")?;
    let fname = Name::parse(&file).map_err(|e| format!("FILE({file}): {e}"))?;
    let mut o = Override::new(fname);
    if let Some(t) = p.kw("TOFILE") {
        o = o.tofile(FileRef::parse(t).map_err(|e| format!("TOFILE({t}): {e}"))?);
    }
    if let Some(m) = p.upper("MBR") {
        o = o.member(match m.as_str() {
            "*FIRST" => MemberSpec::First,
            "*LAST" => MemberSpec::Last,
            "*ALL" => MemberSpec::All,
            other => {
                MemberSpec::Name(Name::parse(other).map_err(|e| format!("MBR({other}): {e}"))?)
            }
        });
    }
    if let Some(s) = p.upper("SHARE") {
        o = o.share(s == "*YES");
    }
    if let Some(l) = p.upper("LVLCHK") {
        o = o.lvlchk(l == "*YES");
    }
    if let Some(i) = p.upper("INHWRT") {
        o = o.inhwrt(i == "*YES");
    }
    if let Some(pos) = p.upper("POSITION") {
        let words: Vec<&str> = pos.split_whitespace().collect();
        o = o.position(match words.as_slice() {
            ["*START"] => Position::Start,
            ["*END"] => Position::End,
            ["*RRN", n] => Position::Rrn(
                n.parse()
                    .map_err(|_| format!("POSITION({pos}): a record number is needed"))?,
            ),
            _ => return Err(format!("POSITION({pos}) is not *START, *END or *RRN n")),
        });
    }
    if p.upper("SECURE").as_deref() == Some("*YES") {
        o = o.secure();
    }
    match p.upper("OVRSCOPE").as_deref() {
        None | Some("*ACTGRPDFN" | "*CALLLVL") => {}
        Some("*JOB") => o = o.scope("*JOB"),
        Some(s) => return Err(format!("CPD0084 OVRSCOPE({s}) is not valid")),
    }
    o = o.level(call_level(p));
    let (_, ctx) = db(system)?;
    ctx.session.ovrdbf(o);
    Ok(format!(
        "Override of {file} in effect ({} total).",
        ctx.session.overrides().len()
    ))
}

/// `dltovr FILE(F|*ALL) [LVL(*|*JOB) CALLLVL(n)]`, or `dltovr ENDLVL(n)`
/// from the job when call level `n` returned.
pub fn dltovr(system: &mut System, p: &Params) -> Result<String, String> {
    if let Some(n) = p.kw("ENDLVL") {
        let level: u32 = n
            .trim()
            .parse()
            .map_err(|_| format!("CPD0084 ENDLVL({n}) is not valid"))?;
        let (_, ctx) = db(system)?;
        let k = ctx.session.end_level(level);
        let k = k + super::prtf::end_level(system, level);
        return Ok(format!("{k} override(s) deleted."));
    }
    let file = p.pos_or(0, "FILE")?;
    let level = call_level(p);
    let every_level = matches!(p.upper("LVL").as_deref(), Some("*JOB" | "*ALL"));
    let fname = if file.eq_ignore_ascii_case("*ALL") {
        None
    } else {
        Some(Name::parse(&file).map_err(|e| format!("FILE({file}): {e}"))?)
    };
    let (_, ctx) = db(system)?;
    let n = if every_level {
        ctx.session.dltovr(fname)
    } else {
        ctx.session.dltovr_at(fname, level)
    };
    let n = n + super::prtf::dltovr(system, &file, (!every_level).then_some(level));
    Ok(format!("{n} override(s) deleted."))
}
