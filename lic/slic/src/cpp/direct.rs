// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Processing programs with no console word behind them: libraries and
//! objects, data areas and queues, the command objects, the system
//! values, `GO`. Each reads its parameters from the block and calls
//! the store or the console function that does the work.

use std::fmt::Write as _;

use cmd::block::Parms;
use cmd::def::{Allow, Mode, Qual};
use cmd::store::{Changes, Ctx};
use qsname::{MemberSpec, Name};
use stg::proto::CmdReply;
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

use super::{done, escape};
use crate::ipl::system::{System, now_unix};

/// Run `name` directly, or `None` when the bridge handles it.
#[allow(clippy::too_many_lines)] // one arm per command
pub fn run(system: &mut System, name: &str, parms: &Parms) -> Option<CmdReply> {
    Some(match name {
        "CRTLIB" => {
            let lib = parms.name("LIB");
            let text = parms.text("TEXT");
            let out = old(system, &format!("obj crt *LIB QSYS/{lib} {text}"));
            finish(out, name, "CPC2102", &[(&lib, 10)])
        }
        "DLTLIB" => {
            // The objects go first, as IBM's DLTLIB deletes them.
            let lib = parms.name("LIB");
            let cleared = clrlib(system, &lib);
            if cleared.escape.is_some() {
                return Some(cleared);
            }
            let out = old(system, &format!("obj dlt *LIB QSYS/{lib}"));
            finish(out, name, "CPC2194", &[(&lib, 10)])
        }
        "CRTBNDCL" | "CRTCLMOD" | "CRTCLPGM" | "RTVCLSRC" => super::clpgm::run(system, name, parms),
        "CRTBNDRPG" | "CRTRPGMOD" | "CRTSQLRPGI" => super::rpg::run(system, name, parms),
        "CRTPNLGRP" | "CRTMNU" => super::uicmd::run(system, name, parms),
        "CLRLIB" => clrlib(system, &parms.name("LIB")),
        "DLTPGM" | "DLTMOD" | "DLTSRVPGM" | "DLTMSGF" | "DLTBNDDIR" | "DLTAUTL" | "DLTMNU"
        | "DLTPNLGRP" => {
            let (kwd, ty) = match name {
                "DLTPGM" => ("PGM", "*PGM"),
                "DLTMOD" => ("MODULE", "*MODULE"),
                "DLTSRVPGM" => ("SRVPGM", "*SRVPGM"),
                "DLTMSGF" => ("MSGF", "*MSGF"),
                "DLTBNDDIR" => ("BNDDIR", "*BNDDIR"),
                "DLTMNU" => ("MENU", "*MENU"),
                "DLTPNLGRP" => ("PNLGRP", "*PNLGRP"),
                _ => ("AUTL", "*AUTL"),
            };
            let (n, l) = if kwd == "AUTL" {
                (parms.name("AUTL"), "QSYS".to_string())
            } else {
                parms.qual(kwd)
            };
            let out = old(system, &format!("obj dlt {ty} {}/{n}", lib_or(&l)));
            finish(out, name, "CPC2191", &[(&n, 10), (&l, 10), (ty, 10)])
        }
        "DSPLIB" | "WRKLIB" => {
            let lib = parms.name("LIB");
            if lib == "*ALL" || lib == "*LIBL" || lib == "*USRLIBL" || lib.is_empty() {
                plain(old(system, "lib ls"))
            } else {
                plain(old(system, &format!("obj ls {lib}")))
            }
        }
        "RNMOBJ" => {
            let (n, l) = parms.qual("OBJ");
            let out = old(
                system,
                &format!(
                    "obj rnm {} {}/{n} {}",
                    parms.name("OBJTYPE"),
                    lib_or(&l),
                    parms.name("NEWOBJ")
                ),
            );
            finish(out, name, "CPC2192", &[(&n, 10), (&l, 10)])
        }
        "MOVOBJ" => {
            let (n, l) = parms.qual("OBJ");
            let out = old(
                system,
                &format!(
                    "obj mov {} {}/{n} {}",
                    parms.name("OBJTYPE"),
                    lib_or(&l),
                    parms.name("TOLIB")
                ),
            );
            finish(out, name, "CPC2196", &[(&n, 10), (&l, 10)])
        }
        "DSPOBJD" | "WRKOBJ" => {
            let (n, l) = parms.qual("OBJ");
            let ty = parms.name("OBJTYPE");
            let out = if n == "*ALL" || n.ends_with('*') {
                let t = if ty == "*ALL" || ty.is_empty() {
                    String::new()
                } else {
                    format!(" {ty}")
                };
                old(system, &format!("obj ls {}{t}", lib_or(&l)))
            } else {
                old(system, &format!("obj dsp {}/{n}", lib_or(&l)))
            };
            plain(out)
        }
        "CHGOBJD" => chgobjd(system, parms),
        "CRTDTAARA" => crtdtaara(system, parms),
        "DLTDTAARA" => {
            let (n, l) = parms.qual("DTAARA");
            let out = old(system, &format!("obj dlt *DTAARA {}/{n}", lib_or(&l)));
            finish(out, name, "CPC2191", &[(&n, 10), (&l, 10), ("*DTAARA", 10)])
        }
        "DSPDTAARA" => {
            let (n, l) = parms.qual("DTAARA");
            if n == "*LDA" || n == "*GDA" || n == "*PDA" {
                return Some(escape(
                    "CPF0006",
                    name,
                    &format!("{n} is the job's; DSPDTAARA shows it in the job"),
                ));
            }
            plain(old(system, &format!("dtaara get {}/{n}", lib_or(&l))))
        }
        "CHGDTAARA" => {
            let (n, l) = parms.qual("DTAARA");
            let n = n.split_whitespace().next().unwrap_or("").to_string();
            let value = parms.text("VALUE");
            let out = old(system, &format!("dtaara set {}/{n} {value}", lib_or(&l)));
            finish(out, name, "CPC0901", &[(&n, 10)])
        }
        "CRTDTAQ" => {
            let (n, l) = parms.qual("DTAQ");
            let text = parms.text("TEXT");
            let num = |k: &str, d: u32| parms.text(k).trim().parse::<u32>().unwrap_or(d);
            let order = match parms.text("SEQ").trim() {
                "*LIFO" => store::obj::dtaq::Order::Lifo,
                "*KEYED" => store::obj::dtaq::Order::Keyed {
                    key_len: u16::try_from(num("KEYLEN", 1).clamp(1, 256)).unwrap_or(1),
                },
                _ => store::obj::dtaq::Order::Fifo,
            };
            let sender = parms.text("SENDERID").trim() == "*YES";
            let qual = format!("{}/{n}", lib_or(&l));
            let out = crate::svc::objects::crt_dtaq(system, &qual, &text, order, num("MAXLEN", 1024), sender)
                .unwrap_or_else(|e| format!("CPF9898 {e}
"));
            finish(out, name, "CPC2101", &[(&n, 10), (&l, 10)])
        }
        "DLTDTAQ" => {
            let (n, l) = parms.qual("DTAQ");
            let out = old(system, &format!("obj dlt *DTAQ {}/{n}", lib_or(&l)));
            finish(out, name, "CPC2191", &[(&n, 10), (&l, 10), ("*DTAQ", 10)])
        }
        "RCLSTG" => plain(old(system, "rclstg")),
        "RCLTMPSTG" => plain("Temporary storage reclaimed.\n".into()),
        "DSPSYSVAL" => plain(old(system, &format!("sysval {}", parms.name("SYSVAL")))),
        "CHGSYSVAL" => {
            let sv = parms.name("SYSVAL");
            let out = old(system, &format!("sysval {sv} {}", parms.text("VALUE")));
            finish(out, name, "CPC1601", &[(&sv, 10)])
        }
        "WRKSYSVAL" => wrksysval(system, &parms.name("SYSVAL")),
        "GO" => {
            let (menu, _) = parms.qual("MENU");
            escape(
                "CPF0901",
                &menu,
                &format!("menu {menu} is not available in this release"),
            )
        }
        "CRTCMD" => crtcmd(system, parms),
        "CHGCMD" => chgcmd(system, parms),
        "DLTCMD" | "DLTPRXCMD" => dltcmd(system, parms),
        "DSPCMD" => dspcmd(system, parms),
        "CHGCMDDFT" => chgcmddft(system, parms),
        "CRTPRXCMD" => crtprxcmd(system, parms),
        "CHGPRXCMD" => chgprxcmd(system, parms),
        _ => return None,
    })
}

/// A library name for the old commands: `*LIBL` and `*CURLIB` as they
/// take them.
pub(super) fn lib_or(l: &str) -> String {
    if l.is_empty() {
        "*LIBL".into()
    } else {
        l.to_string()
    }
}

/// A line of the old console table.
pub(super) fn old(system: &mut System, line: &str) -> String {
    crate::stgsvc::exec::run(system, line)
}

/// Output alone.
pub(super) fn plain(text: String) -> CmdReply {
    match super::leading_msgid(text.lines().next().unwrap_or("").trim()) {
        Some(id) if !id.starts_with("CPC") && !id.starts_with("CPI") => CmdReply {
            escape: Some(stg::proto::CmdMsg {
                msgid: id.to_string(),
                kind: 7,
                data: Vec::new(),
            }),
            text,
            ..CmdReply::default()
        },
        _ => CmdReply {
            text,
            ..CmdReply::default()
        },
    }
}

/// Output with the completion message when it did not fail.
pub(super) fn finish(text: String, name: &str, comp: &str, fields: &[(&str, usize)]) -> CmdReply {
    let _ = name;
    let r = plain(text);
    if r.escape.is_some() {
        return r;
    }
    done(r.text, comp, fields)
}

/// Run `f` on ASP 1 in a transaction with the job's names.
pub(super) fn in_store<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId, &ops::NameContext) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    crate::svc::db::with_txn(&mut s.store, |txn| {
        let names = crate::svc::ile::name_context(txn, qsys);
        f(txn, qsys, &names)
    })
}

/// A library by name, `*CURLIB` through the names.
pub(super) fn library(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    lib: &str,
) -> Result<ObjectId, String> {
    match lib {
        "*CURLIB" | "" => names
            .curlib
            .ok_or_else(|| "CPF2110 no current library".to_string()),
        "*LIBL" => Err("CPD0084 a library name is needed".into()),
        l => wm::obj::library(txn, qsys, l).map_err(|e| format!("CPF2110 {e}")),
    }
}

pub(super) fn ctx(system: &System) -> Ctx {
    Ctx {
        now: now_unix(),
        owner: system.svc_user.clone().unwrap_or_else(|| "QSYS".into()),
        system: "QS400".into(),
    }
}

pub(super) fn failed(name: &str, e: &str) -> CmdReply {
    let id = super::leading_msgid(e).unwrap_or("CPF0006").to_string();
    let detail = e.strip_prefix(&id).unwrap_or(e).trim().to_string();
    escape(&id, name, &detail)
}

/// `CLRLIB`: every object of the library deleted.
fn clrlib(system: &mut System, lib: &str) -> CmdReply {
    let lib = lib.to_string();
    let r = in_store(system, |txn, qsys, names| {
        let id = library(txn, qsys, names, &lib)?;
        let entries = ops::list(txn, id, None).map_err(|e| e.to_string())?;
        let mut n = 0;
        for (_, oid) in entries {
            ops::delete(txn, JobHandle(0), oid)
                .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF2189")))?;
            n += 1;
        }
        Ok(n)
    });
    match r {
        Ok(n) => done(
            format!("Library {lib} cleared: {n} object(s) deleted.\n"),
            "CPC2170",
            &[(&lib, 10)],
        ),
        Err(e) => failed("CLRLIB", &e),
    }
}

/// `CHGOBJD`: the text.
fn chgobjd(system: &mut System, parms: &Parms) -> CmdReply {
    let (n, l) = parms.qual("OBJ");
    let ty = parms.name("OBJTYPE");
    let text = parms.text("TEXT");
    if !parms.specified("TEXT") || text == "*SAME" {
        return plain(String::new());
    }
    let r = in_store(system, |txn, qsys, names| {
        let code = store::obj::types::type_by_name(&ty)
            .map(|t| t.code)
            .ok_or_else(|| format!("CPD0084 OBJTYPE({ty}) is not a type"))?;
        let found = wm::obj::find(txn, qsys, names, &lib_or(&l), &n, code)
            .map_err(|e| format!("CPF9801 {e}"))?;
        ops::change_text(txn, found, &text, now_unix())
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF2189")))
    });
    match r {
        Ok(()) => done(String::new(), "CPC2101", &[(&n, 10), (&l, 10)]),
        Err(e) => failed("CHGOBJD", &e),
    }
}

/// `CRTDTAARA DTAARA(LIB/NAME) TYPE(*CHAR|*DEC|*LGL) LEN(n d) VALUE(v) TEXT(t)`.
fn crtdtaara(system: &mut System, parms: &Parms) -> CmdReply {
    use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
    let (n, l) = parms.qual("DTAARA");
    let ty = parms.name("TYPE");
    let elems = parms.elems("LEN");
    let len = elems
        .first()
        .copied()
        .flatten()
        .and_then(|v| v.source().parse::<u32>().ok());
    let dec = elems
        .get(1)
        .copied()
        .flatten()
        .and_then(|v| v.source().parse::<u8>().ok())
        .unwrap_or(0);
    let value = parms.text("VALUE");
    let text = parms.text("TEXT");
    let r = in_store(system, |txn, qsys, names| {
        let lib = library(txn, qsys, names, &lib_or(&l).replace("*LIBL", "*CURLIB"))?;
        let name = Name::parse(&n).map_err(|e| format!("CPD0078 {e}"))?;
        let (kind, v) = match ty.as_str() {
            "*DEC" => {
                let digits = u8::try_from(len.unwrap_or(15)).unwrap_or(15);
                let d = store::obj::dtaara::Decimal::parse(
                    if value.is_empty() { "0" } else { &value },
                    digits,
                    dec,
                )
                .map_err(|e| format!("CPD0084 VALUE: {e}"))?;
                (
                    DataAreaType::Dec {
                        digits,
                        decimals: dec,
                    },
                    DataAreaValue::Dec(d.scaled),
                )
            }
            "*LGL" => (DataAreaType::Lgl, DataAreaValue::Lgl(value == "1")),
            _ => (
                DataAreaType::Char {
                    len: u16::try_from(len.unwrap_or(32)).unwrap_or(2000),
                },
                DataAreaValue::Char(value.clone()),
            ),
        };
        let p = store::obj::CreateParams {
            library: lib,
            name,
            type_code: types::DTAARA,
            attribute: String::new(),
            text: text.clone(),
            owner: system_owner(),
            system: "QS400".into(),
            now: now_unix(),
            by_system: false,
        };
        dtaara::create(txn, &p, kind, &v)
            .map_err(|e| format!("{} {e}", e.msgid().unwrap_or("CPF1023")))?;
        Ok(())
    });
    match r {
        Ok(()) => done(
            format!("Data area {n} created.\n"),
            "CPC0904",
            &[(&n, 10), (&l, 10)],
        ),
        Err(e) => failed("CRTDTAARA", &e),
    }
}

fn system_owner() -> String {
    crate::stgsvc::security::actor().map_or_else(|| "QSYS".into(), |a| a.user.clone())
}

/// `WRKSYSVAL`: every system value, or those of a name.
fn wrksysval(system: &mut System, which: &str) -> CmdReply {
    let Some(vals) = system.sysvals.as_ref() else {
        return escape("CPF0006", "WRKSYSVAL", "system values not loaded");
    };
    let mut out = String::from("System value   Value\n");
    let mut names: Vec<String> = vals.catalog().entries().map(|e| e.name.clone()).collect();
    names.sort();
    for name in names {
        if which != "*ALL"
            && !which.is_empty()
            && which != "*SYSVAL"
            && !name.starts_with(which.trim_end_matches('*'))
        {
            continue;
        }
        if let Ok(v) = vals.get(&name) {
            let _ = writeln!(out, "{name:<14} {v}");
        }
    }
    plain(out)
}

/// The source of a member, for `CRTCMD`.
pub(super) fn member_text(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    file: &str,
    lib: &str,
    mbr: &str,
) -> Result<String, String> {
    let id = wm::obj::find(txn, qsys, names, &lib_or(lib), file, types::FILE)
        .map_err(|e| format!("CPF9801 {e}"))?;
    let m = Name::parse(mbr).map_err(|e| format!("CPD0078 {e}"))?;
    db::source::srcget(txn, id, MemberSpec::Name(m)).map_err(|e| format!("CPF9801 {e}"))
}

/// `CRTCMD`.
fn crtcmd(system: &mut System, parms: &Parms) -> CmdReply {
    let (n, l) = parms.qual("CMD");
    let (pgm, pgmlib) = parms.qual("PGM");
    let (sf, sfl) = parms.qual("SRCFILE");
    let mbr = {
        let m = parms.name("SRCMBR");
        if m.is_empty() || m == "*CMD" {
            n.clone()
        } else {
            m
        }
    };
    let text = parms.text("TEXT");
    let replace = parms.yes("REPLACE");
    let cx = ctx(system);
    // SRCSTMF: the definition source is a stream file read in the job's name.
    let stmf = match parms.specified("SRCSTMF").then(|| parms.text("SRCSTMF")).filter(|s| !s.is_empty()) {
        Some(path) => match super::stmf::read_as_job(system, &path) {
            Ok(t) => Some(t.replace("\r\n", "\n")),
            Err(e) => return failed("CRTCMD", &e),
        },
        None => None,
    };
    let r = in_store(system, |txn, qsys, names| {
        let source = match &stmf {
            Some(t) => t.clone(),
            None => member_text(txn, qsys, names, &sf, &sfl, &mbr)?,
        };
        let mut def = cmd::source::compile(&n, &source).map_err(|diags| {
            let mut s = String::from("CPF0276");
            for d in diags {
                let _ = write!(s, "\n{} {}", d.msgid, d.detail);
            }
            s
        })?;
        def.cpp = Qual::new(&pgm, &pgmlib);
        if !text.is_empty() && text != "*SRCMBRTXT" && text != "*BLANK" {
            def.text.clone_from(&text);
        }
        if parms.specified("ALLOW") {
            let mut bits = 0;
            for v in parms.list("ALLOW") {
                bits |= Allow::flag(&v.source()).unwrap_or(0);
            }
            def.allow = Allow(bits);
        }
        if parms.specified("MODE") {
            let mut bits = 0;
            for v in parms.list("MODE") {
                bits |= Mode::flag(&v.source()).unwrap_or(0);
            }
            def.mode = Mode(bits);
        }
        def.alwlmtusr = parms.yes("ALWLMTUSR");
        for (kw, dst) in [
            ("VLDCKR", &mut def.vldckr),
            ("PMTOVRPGM", &mut def.pmtovrpgm),
            ("HLPPNLGRP", &mut def.hlppnlgrp),
        ] {
            let (qn, ql) = parms.qual(kw);
            if !qn.is_empty() && !qn.starts_with('*') {
                *dst = Some(Qual::new(&qn, &ql));
            }
        }
        let lib = library(txn, qsys, names, &lib_or(&l).replace("*LIBL", "*CURLIB"))?;
        let name = Name::parse(&n).map_err(|e| format!("CPD0078 {e}"))?;
        let object = cmd::CmdObject::new(def, source, cx.now);
        cmd::store::crtcmd(txn, &cx, lib, name, &object, replace, false)
            .map_err(|e| e.to_string())?;
        Ok(())
    });
    match r {
        Ok(()) => done(
            format!("Command {n} created.\n"),
            "CPC0202",
            &[(&n, 10), (&l, 10)],
        ),
        Err(e) => failed("CRTCMD", &e),
    }
}

/// The command `CMD(LIB/NAME)` names.
fn find_cmd(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    names: &ops::NameContext,
    parms: &Parms,
) -> Result<(ObjectId, String, String), String> {
    let (n, l) = parms.qual("CMD");
    let found = cmd::store::find(txn, qsys, names, &n, &lib_or(&l)).map_err(|e| e.to_string())?;
    let lib = ops::describe(txn, found.library)
        .map(|h| h.name.to_string())
        .unwrap_or_default();
    Ok((found.id, n, lib))
}

/// `CHGCMD`.
fn chgcmd(system: &mut System, parms: &Parms) -> CmdReply {
    let r = in_store(system, |txn, qsys, names| {
        let (id, n, l) = find_cmd(txn, qsys, names, parms)?;
        let mut c = Changes::default();
        let (pgm, pgmlib) = parms.qual("PGM");
        if parms.specified("PGM") && pgm != "*SAME" {
            c.cpp = Some(Qual::new(&pgm, &pgmlib));
        }
        if parms.specified("ALLOW") {
            let mut bits = 0;
            for v in parms.list("ALLOW") {
                bits |= Allow::flag(&v.source()).unwrap_or(0);
            }
            if bits != 0 {
                c.allow = Some(Allow(bits));
            }
        }
        if parms.specified("MODE") {
            let mut bits = 0;
            for v in parms.list("MODE") {
                bits |= Mode::flag(&v.source()).unwrap_or(0);
            }
            if bits != 0 {
                c.mode = Some(Mode(bits));
            }
        }
        if parms.specified("ALWLMTUSR") && parms.name("ALWLMTUSR") != "*SAME" {
            c.alwlmtusr = Some(parms.yes("ALWLMTUSR"));
        }
        if parms.specified("TEXT") && parms.text("TEXT") != "*SAME" {
            c.text = Some(parms.text("TEXT"));
        }
        for (kw, dst) in [
            ("VLDCKR", &mut c.vldckr),
            ("PMTOVRPGM", &mut c.pmtovrpgm),
            ("HLPPNLGRP", &mut c.hlppnlgrp),
        ] {
            if parms.specified(kw) {
                let (qn, ql) = parms.qual(kw);
                if qn == "*NONE" {
                    *dst = Some(None);
                } else if qn != "*SAME" {
                    *dst = Some(Some(Qual::new(&qn, &ql)));
                }
            }
        }
        cmd::store::chgcmd(txn, id, &c, now_unix()).map_err(|e| e.to_string())?;
        Ok((n, l))
    });
    match r {
        Ok((n, l)) => done(String::new(), "CPC0203", &[(&n, 10), (&l, 10)]),
        Err(e) => failed("CHGCMD", &e),
    }
}

/// `DLTCMD` and `DLTPRXCMD`.
fn dltcmd(system: &mut System, parms: &Parms) -> CmdReply {
    let r = in_store(system, |txn, qsys, names| {
        let (id, n, l) = find_cmd(txn, qsys, names, parms)?;
        cmd::store::dltcmd(txn, JobHandle(0), id).map_err(|e| e.to_string())?;
        Ok((n, l))
    });
    match r {
        Ok((n, l)) => done(
            String::new(),
            "CPC2191",
            &[(&n, 10), (&l, 10), ("*CMD", 10)],
        ),
        Err(e) => failed("DLTCMD", &e),
    }
}

/// `DSPCMD`.
fn dspcmd(system: &mut System, parms: &Parms) -> CmdReply {
    let r = in_store(system, |txn, qsys, names| {
        let (id, _, l) = find_cmd(txn, qsys, names, parms)?;
        let o = cmd::store::load(txn, id).map_err(|e| e.to_string())?;
        Ok(cmd::store::dspcmd(&o, &l))
    });
    match r {
        Ok(text) => plain(text),
        Err(e) => failed("DSPCMD", &e),
    }
}

/// `CHGCMDDFT`.
fn chgcmddft(system: &mut System, parms: &Parms) -> CmdReply {
    let newdft = parms.text("NEWDFT");
    let r = in_store(system, |txn, qsys, names| {
        let (id, n, l) = find_cmd(txn, qsys, names, parms)?;
        let ov = cmd::store::chgcmddft(txn, id, &newdft).map_err(|e| e.to_string())?;
        Ok((n, l, ov.len()))
    });
    match r {
        Ok((n, l, k)) => done(
            format!("{k} default(s) of {n} changed.\n"),
            "CPC0203",
            &[(&n, 10), (&l, 10)],
        ),
        Err(e) => failed("CHGCMDDFT", &e),
    }
}

/// `CRTPRXCMD`.
fn crtprxcmd(system: &mut System, parms: &Parms) -> CmdReply {
    let (n, l) = parms.qual("CMD");
    let (tn, tl) = parms.qual("TGTCMD");
    let text = parms.text("TEXT");
    let replace = parms.yes("REPLACE");
    let cx = ctx(system);
    let r = in_store(system, |txn, qsys, names| {
        let lib = library(txn, qsys, names, &lib_or(&l).replace("*LIBL", "*CURLIB"))?;
        let name = Name::parse(&n).map_err(|e| format!("CPD0078 {e}"))?;
        cmd::store::crtprxcmd(txn, &cx, lib, name, Qual::new(&tn, &tl), &text, replace)
            .map_err(|e| e.to_string())?;
        Ok(())
    });
    match r {
        Ok(()) => done(
            format!("Proxy command {n} created.\n"),
            "CPC0202",
            &[(&n, 10), (&l, 10)],
        ),
        Err(e) => failed("CRTPRXCMD", &e),
    }
}

/// `CHGPRXCMD`.
fn chgprxcmd(system: &mut System, parms: &Parms) -> CmdReply {
    let r = in_store(system, |txn, qsys, names| {
        let (n, l) = parms.qual("CMD");
        let found =
            cmd::store::find(txn, qsys, names, &n, &lib_or(&l)).map_err(|e| e.to_string())?;
        let (tn, tl) = parms.qual("TGTCMD");
        let target = (parms.specified("TGTCMD") && tn != "*SAME").then(|| Qual::new(&tn, &tl));
        let text = parms.text("TEXT");
        let text = (parms.specified("TEXT") && text != "*SAME").then_some(text);
        cmd::store::chgprxcmd(txn, found.id, target, text.as_deref(), now_unix())
            .map_err(|e| e.to_string())?;
        Ok((n, l))
    });
    match r {
        Ok((n, l)) => done(String::new(), "CPC0203", &[(&n, 10), (&l, 10)]),
        Err(e) => failed("CHGPRXCMD", &e),
    }
}
