// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ILE commands of the service console: assembling IR, creating
//! and updating programs and binding directories, calling, displaying
//! objects and the job, replying to inquiries, dumps, message
//! descriptions, and the debugger outside a stop.

use std::fmt::Write as _;

use ile::store::{Ctx, ObjSpec};
use ile::{
    ActGrpAttr, BindOptions, BndDirEntry, BndEntryKind, CreateData, ExportSpec, ModuleObject,
    PgmKind,
};
use mi::QueueRef;
use qsname::{LibSpec, MemberSpec, Name};
use store::obj::ops::{self, NameContext};
use store::obj::{ObjectId, types};
use store::txn::Txn;

use super::super::db::{Params, with_txn};
use crate::ipl::system::{System, now_unix};

/// The commands of this module, or none.
pub fn command(system: &mut System, cmd: &str, args: &[&str], line: &str) -> Option<String> {
    let p = Params::parse(line);
    let r = match cmd {
        "qir" => qir_cmd(system, &p),
        "crtpgm" => crtpgm(system, &p, PgmKind::Pgm),
        "crtsrvpgm" => crtpgm(system, &p, PgmKind::SrvPgm),
        "updpgm" => updpgm(system, &p),
        "crtbnddir" => crtbnddir(system, &p),
        "addbnddire" => addbnddire(system, &p),
        "call" => call(system, line),
        "dsppgm" => display(system, &p, types::PGM),
        "dspsrvpgm" => display(system, &p, types::SRVPGM),
        "dspmod" => display(system, &p, types::MODULE),
        "dsppgmref" => display(system, &p, u16::MAX),
        "dspbnddir" => display(system, &p, types::BNDDIR),
        "dspactgrp" => with_job(system, |j| j.actgrp_text()),
        "rclactgrp" => rclactgrp(system, args),
        "dspcallstk" => with_job(system, |j| j.call_stack_text()),
        "dspmsgq" => dspmsgq(system, args),
        "rpy" => rpy(system, args),
        "dmp" => with_job(system, |j| {
            j.last_dump.clone().unwrap_or_else(|| j.dump(true))
        }),
        "wrkmsgd" => wrkmsgd(system, &p),
        "dspmsgd" => dspmsgd(system, &p),
        "strdbg" => strdbg(system, &p),
        "enddbg" => with_job(system, |j| {
            j.enddbg();
            "Debug ended.\n".to_string()
        }),
        "addbkp" => addbkp(system, &p, true),
        "rmvbkp" => addbkp(system, &p, false),
        "dspbkp" => with_job(system, |j| {
            let mut out = String::new();
            if let Some(d) = &j.debug {
                for (pgm, n) in &d.breakpoints {
                    let _ = writeln!(out, "  {pgm:<21} statement {n}");
                }
                for pgm in &d.programs {
                    let _ = writeln!(out, "  {pgm} under debug");
                }
            }
            if out.is_empty() {
                "No breakpoints.\n".into()
            } else {
                out
            }
        }),
        _ => return None,
    };
    Some(match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    })
}

/// `LIB/NAME` split and validated.
fn qualified(s: &str) -> Result<(String, Name), String> {
    let (lib, name) = s
        .split_once('/')
        .ok_or_else(|| format!("{s}: use LIB/NAME"))?;
    let name = Name::parse(name).map_err(|e| e.to_string())?;
    Ok((lib.to_ascii_uppercase(), name))
}

/// The create context of the service job.
fn ctx(txn: &mut Txn<'_>, qsys: ObjectId) -> Ctx {
    Ctx {
        qsys,
        names: super::name_context(txn, qsys),
        now: now_unix(),
        owner: "QSYS".into(),
        system: "QS400".into(),
    }
}

/// Run `f` in the store that holds `lib`, with the library's id.
fn in_store<T>(
    system: &mut System,
    lib: &str,
    f: impl FnOnce(&mut Txn<'_>, &Ctx, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    if lib.eq_ignore_ascii_case("QTEMP") {
        let t = system.temp.as_mut().ok_or("QTEMP is not available")?;
        let qtemp = t.qtemp;
        return with_txn(&mut t.store, |txn| {
            let c = Ctx {
                qsys: qtemp,
                names: NameContext {
                    curlib: Some(qtemp),
                    libl: vec![qtemp],
                },
                now: now_unix(),
                owner: "QSYS".into(),
                system: "QS400".into(),
            };
            f(txn, &c, qtemp)
        });
    }
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    with_txn(&mut s.store, |txn| {
        let c = ctx(txn, qsys);
        let library = ops::find_library(txn, qsys, Name::parse(lib).map_err(|e| e.to_string())?)
            .map(|found| found.id)
            .map_err(|e| e.to_string())?;
        f(txn, &c, library)
    })
}

/// An object by `LIB/NAME` and type, in either store.
fn find(system: &mut System, qual: &str, type_code: u16) -> Result<(ObjectId, bool), String> {
    let (lib, name) = qualified(qual)?;
    in_store(system, &lib, |txn, _, library| {
        let l = store::obj::library::Library::load(txn, library).map_err(|e| e.to_string())?;
        l.lookup(txn, type_code, name)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("{qual} not found"))
    })
    .map(|id| (id, lib.eq_ignore_ascii_case("QTEMP")))
}

/// A `LIB/NAME` list of a keyword, space separated.
fn spec_list(p: &Params, kw: &str) -> Result<Vec<ObjSpec>, String> {
    let Some(v) = p.kw(kw) else {
        return Ok(Vec::new());
    };
    v.split_whitespace()
        .map(|s| {
            let (lib, name) = s.split_once('/').map_or(("*LIBL", s), |(l, n)| (l, n));
            Ok(ObjSpec {
                lib: match lib.to_ascii_uppercase().as_str() {
                    "*LIBL" => LibSpec::Libl,
                    "*CURLIB" => LibSpec::Curlib,
                    l => LibSpec::Name(Name::parse(l).map_err(|e| e.to_string())?),
                },
                name: Name::parse(name).map_err(|e| e.to_string())?,
            })
        })
        .collect()
}

/// `qir asm|check|dis`.
fn qir_cmd(system: &mut System, p: &Params) -> Result<String, String> {
    match p.pos.first().map(String::as_str) {
        Some("asm") => {
            let text = source_text(system, p)?;
            let module = qir::parse(&text).map_err(|e| format!("{e}"))?;
            let target = p.kw("MODULE").ok_or("MODULE(LIB/NAME) is needed")?;
            let (lib, name) = qualified(target)?;
            let mut create = CreateData {
                compiler: "qirc".into(),
                ..CreateData::default()
            };
            if p.pos.len() >= 3 {
                let (sl, sf) = qualified(&p.pos[1])?;
                create.src_lib = sl;
                create.src_file = sf.to_string();
                create.src_mbr = p.pos[2].to_ascii_uppercase();
            }
            let object =
                ModuleObject::compile(module, create, now_unix()).map_err(|e| e.to_string())?;
            let text = p.text("TEXT");
            let procs = object.ir.procs.len();
            in_store(system, &lib, |txn, c, library| {
                ile::store::crtmod(txn, c, library, name, &object, &text).map_err(|e| e.to_string())
            })?;
            Ok(format!(
                "Module {lib}/{name} created: {procs} procedure(s), {} bytes of code.\n",
                object.code.code.len()
            ))
        }
        Some("check") => {
            let text = source_text(system, p)?;
            match qir::parse(&text) {
                Ok(m) => match qir::verify(&m) {
                    Ok(()) => Ok(format!("Module {} verifies.\n", m.name)),
                    Err(e) => Ok(format!("{e}\n")),
                },
                Err(e) => Ok(format!("{e}\n")),
            }
        }
        Some("dis") => {
            let qual = p.pos.get(1).ok_or("qir dis LIB/MODULE")?;
            let (id, _) = find(system, qual, types::MODULE)?;
            let (lib, _) = qualified(qual)?;
            let m = in_store(system, &lib, |txn, _, _| {
                ile::store::load_module(txn, id).map_err(|e| e.to_string())
            })?;
            Ok(qir::print(&m.ir))
        }
        _ => Err("qir asm|check|dis ...".into()),
    }
}

/// The IR text: a source member (`LIB/SRCPF MBR`) or an xfer buffer.
fn source_text(system: &mut System, p: &Params) -> Result<String, String> {
    if p.pos.len() >= 3 {
        let (lib, file) = qualified(&p.pos[1])?;
        let member = Name::parse(&p.pos[2]).map_err(|e| e.to_string())?;
        return in_store(system, &lib, |txn, _, library| {
            let l = store::obj::library::Library::load(txn, library).map_err(|e| e.to_string())?;
            let id = l
                .lookup(txn, types::FILE, file)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("{lib}/{file} not found"))?;
            db::source::srcget(txn, id, MemberSpec::Name(member)).map_err(|e| e.to_string())
        });
    }
    let name = p
        .pos
        .get(1)
        .ok_or("qir asm LIB/SRCPF MBR MODULE(..) | qir asm NAME MODULE(..)")?;
    system
        .svc
        .xfer
        .get(name)
        .cloned()
        .ok_or_else(|| format!("{name}: no such buffer; xfer put it first"))
}

fn actgrp_of(p: &Params) -> ActGrpAttr {
    match p.upper("ACTGRP").as_deref() {
        Some("*NEW") => ActGrpAttr::New,
        Some("*CALLER") | None => ActGrpAttr::Caller,
        Some("*DFTACTGRP") => ActGrpAttr::Default,
        Some(n) => ActGrpAttr::Named(n.to_string()),
    }
}

fn diag_text(diags: &[ile::Diag]) -> String {
    let mut out = String::new();
    for d in diags {
        let _ = writeln!(out, "{} {}", d.id, d.text);
    }
    out
}

/// `crtpgm` and `crtsrvpgm`.
fn crtpgm(system: &mut System, p: &Params, kind: PgmKind) -> Result<String, String> {
    let qual = p.pos_or(0, "PGM")?;
    let (lib, name) = qualified(&qual)?;
    let modules = spec_list(p, "MODULE")?;
    if modules.is_empty() {
        return Err("MODULE(LIB/M ...) is needed".into());
    }
    let bnddirs = spec_list(p, "BNDDIR")?;
    let mut options = BindOptions::new(&name.to_string(), kind);
    options.attr.actgrp = actgrp_of(p);
    options.entry_module = p.upper("ENTMOD");
    options.allow_unresolved = p.upper("OPTION").as_deref() == Some("*UNRSLVREF");
    if kind == PgmKind::SrvPgm && p.upper("EXPORT").as_deref() == Some("*SRCFILE") {
        let src = p
            .kw("SRCFILE")
            .ok_or("SRCFILE(LIB/F) is needed with EXPORT(*SRCFILE)")?;
        let (sl, sf) = qualified(src)?;
        let member = Name::parse(&p.upper("SRCMBR").unwrap_or_else(|| name.to_string()))
            .map_err(|e| e.to_string())?;
        let text = in_store(system, &sl, |txn, _, library| {
            let l = store::obj::library::Library::load(txn, library).map_err(|e| e.to_string())?;
            let id = l
                .lookup(txn, types::FILE, sf)
                .map_err(|e| e.to_string())?
                .ok_or_else(|| format!("{sl}/{sf} not found"))?;
            db::source::srcget(txn, id, MemberSpec::Name(member)).map_err(|e| e.to_string())
        })?;
        let parsed = ile::parse_binder_source(&text).map_err(|e| e.to_string())?;
        options.export = ExportSpec::Source(parsed, text);
    }
    let text = p.text("TEXT");
    let what = if kind == PgmKind::Pgm {
        "Program"
    } else {
        "Service program"
    };
    let diags = in_store(system, &lib, |txn, c, library| {
        ile::store::crtpgm(txn, c, library, &modules, &bnddirs, options, &text)
            .map(|(_, d)| d)
            .map_err(|e| e.to_string())
    })?;
    Ok(format!(
        "{}{what} {lib}/{name} created.\n",
        diag_text(&diags)
    ))
}

/// `updpgm LIB/PGM MODULE(...)`.
fn updpgm(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "PGM")?;
    let (lib, _) = qualified(&qual)?;
    let modules = spec_list(p, "MODULE")?;
    let (id, _) =
        find(system, &qual, types::PGM).or_else(|_| find(system, &qual, types::SRVPGM))?;
    let diags = in_store(system, &lib, |txn, c, _| {
        ile::store::updpgm(txn, c, id, &modules).map_err(|e| e.to_string())
    })?;
    if let Some(j) = system.svc.ile.job.as_mut() {
        j.act.loaded.clear();
    }
    Ok(format!("{}{qual} updated.\n", diag_text(&diags)))
}

/// `crtbnddir LIB/B [TEXT('..')]`.
fn crtbnddir(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "BNDDIR")?;
    let (lib, name) = qualified(&qual)?;
    let text = p.text("TEXT");
    in_store(system, &lib, |txn, c, library| {
        ile::store::crtbnddir(txn, c, library, name, &text).map_err(|e| e.to_string())
    })?;
    Ok(format!("Binding directory {lib}/{name} created.\n"))
}

/// `addbnddire LIB/B OBJ((LIB/N *MODULE) (LIB/N *SRVPGM))`.
fn addbnddire(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "BNDDIR")?;
    let (lib, _) = qualified(&qual)?;
    let obj = p
        .kw("OBJ")
        .ok_or("OBJ((LIB/NAME *MODULE|*SRVPGM) ...) is needed")?;
    let mut entries = Vec::new();
    for group in obj.split(')') {
        let g = group.trim().trim_start_matches('(').trim();
        if g.is_empty() {
            continue;
        }
        let mut w = g.split_whitespace();
        let qn = w.next().ok_or("an entry needs LIB/NAME")?;
        let (elib, ename) = qn.split_once('/').map_or(("*LIBL", qn), |(l, n)| (l, n));
        let kind = match w.next().map(str::to_ascii_uppercase).as_deref() {
            Some("*MODULE") => BndEntryKind::Module,
            _ => BndEntryKind::SrvPgm,
        };
        entries.push(BndDirEntry {
            name: ename.to_ascii_uppercase(),
            lib: elib.to_ascii_uppercase(),
            kind,
            defer: false,
        });
    }
    let n = entries.len();
    let (id, _) = find(system, &qual, types::BNDDIR)?;
    in_store(system, &lib, |txn, _, _| {
        ile::store::addbnddire(txn, id, entries, None).map_err(|e| e.to_string())
    })?;
    Ok(format!(
        "{n} entr{} added to {qual}.\n",
        if n == 1 { "y" } else { "ies" }
    ))
}

/// The parameter values of a `call` line: `'text'` as characters,
/// numbers as `packed(15,5)`, `x'..'` as bytes.
fn call_parms(rest: &str) -> Result<Vec<Vec<u8>>, String> {
    let mut out = Vec::new();
    let chars: Vec<char> = rest.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        if chars[i] == '\''
            || (chars[i].eq_ignore_ascii_case(&'x') && chars.get(i + 1) == Some(&'\''))
        {
            let hex = chars[i] != '\'';
            i += if hex { 2 } else { 1 };
            let start = i;
            while i < chars.len() && chars[i] != '\'' {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            i += 1;
            if hex {
                let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
                let bytes = (0..clean.len() / 2)
                    .map(|k| {
                        u8::from_str_radix(&clean[2 * k..2 * k + 2], 16).map_err(|e| e.to_string())
                    })
                    .collect::<Result<Vec<u8>, String>>()?;
                out.push(bytes);
            } else {
                out.push(ccsid::to_ebcdic(&text).map_err(|_| format!("{text}: not CP037"))?);
            }
            continue;
        }
        let start = i;
        while i < chars.len() && !chars[i].is_whitespace() {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        let d = qdecimal::Decimal::parse(&word).map_err(|_| format!("{word}: not a number"))?;
        let p = qdecimal::Packed::new(15, 5).expect("valid");
        out.push(
            p.encode(&d, qdecimal::Rounding::Truncate)
                .map_err(|e| e.to_string())?,
        );
    }
    Ok(out)
}

/// `call LIB/PGM [parms]`.
fn call(system: &mut System, line: &str) -> Result<String, String> {
    let rest = line.trim_start()[4..].trim_start();
    let (qual, parms) = rest
        .find(char::is_whitespace)
        .map_or((rest, ""), |i| (&rest[..i], &rest[i..]));
    if qual.is_empty() {
        return Err("call LIB/PGM [parms]".into());
    }
    let (lib, name) = qual
        .split_once('/')
        .map_or(("*LIBL", qual), |(l, n)| (l, n));
    let (lib, name) = (lib.to_ascii_uppercase(), name.to_ascii_uppercase());
    let mut buffers = call_parms(parms)?;
    let addrs: Vec<u64> = buffers.iter_mut().map(|b| b.as_mut_ptr() as u64).collect();
    let (result, extra) = super::run_in_job(system, |j| {
        let before = j.console.len();
        let r = j.call_program(&name, &lib, &addrs);
        let lines: Vec<String> = j.console.drain(before..).collect();
        let pending = j.clear();
        (r, lines, pending)
    })?;
    let (r, lines, pending) = result;
    let mut out = String::new();
    for l in lines {
        let _ = writeln!(out, "{l}");
    }
    match r {
        Ok(()) => {}
        Err(c) => {
            let _ = writeln!(out, "{}: {lib}/{name} could not be called.", c.msgid());
        }
    }
    if let Some(e) = pending {
        if e.msgid == "CEE9903" {
            let _ = writeln!(out, "Program ended by CEETREC.");
        } else {
            let _ = writeln!(out, "Exception {} left for the caller.", e.msgid);
        }
    }
    for (i, b) in buffers.iter().enumerate() {
        if b.iter().all(u8::is_ascii_graphic) {
            continue;
        }
        let _ = writeln!(out, "Parm {}: {}", i + 1, parm_text(b));
    }
    out.push_str(&extra);
    Ok(out)
}

/// A parameter after the call: its text when it is text, else hex.
fn parm_text(b: &[u8]) -> String {
    let text = ccsid::from_ebcdic(b);
    if text.chars().all(|c| c.is_ascii_graphic() || c == ' ') {
        format!("'{}'", text.trim_end())
    } else {
        b.iter().fold(String::new(), |mut s, x| {
            let _ = write!(s, "{x:02X}");
            s
        })
    }
}

/// `dsppgm`, `dspsrvpgm`, `dspmod`, `dsppgmref`, `dspbnddir`.
fn display(system: &mut System, p: &Params, type_code: u16) -> Result<String, String> {
    let qual = p.pos_or(0, "OBJ")?;
    let (lib, name) = qualified(&qual)?;
    if type_code == u16::MAX {
        let (id, _) =
            find(system, &qual, types::PGM).or_else(|_| find(system, &qual, types::SRVPGM))?;
        let pg = in_store(system, &lib, |txn, _, _| {
            ile::store::load_program(txn, id).map_err(|e| e.to_string())
        })?;
        return Ok(ile::dsp::dsppgmref(&pg));
    }
    let (id, _) = find(system, &qual, type_code)?;
    in_store(system, &lib, |txn, _, _| match type_code {
        types::MODULE => ile::store::load_module(txn, id)
            .map(|m| ile::dsp::dspmod(&m))
            .map_err(|e| e.to_string()),
        types::BNDDIR => ile::store::load_bnddir(txn, id)
            .map(|d| ile::dsp::dspbnddir(&name.to_string(), &d))
            .map_err(|e| e.to_string()),
        types::SRVPGM => ile::store::load_program(txn, id)
            .map(|pg| ile::dsp::dspsrvpgm(&pg))
            .map_err(|e| e.to_string()),
        _ => ile::store::load_program(txn, id)
            .map(|pg| ile::dsp::dsppgm(&pg))
            .map_err(|e| e.to_string()),
    })
}

/// A job display, creating the job when needed.
fn with_job(system: &mut System, f: impl FnOnce(&mut mi::Job) -> String) -> Result<String, String> {
    super::run_in_job(system, f).map(|(s, extra)| format!("{s}{extra}"))
}

/// `rclactgrp NAME|*ELIGIBLE`.
fn rclactgrp(system: &mut System, args: &[&str]) -> Result<String, String> {
    let name = args
        .first()
        .map(|s| s.to_ascii_uppercase())
        .ok_or("rclactgrp NAME|*ELIGIBLE")?;
    with_job(system, |j| {
        if name == "*ELIGIBLE" {
            j.rclactgrp_eligible();
            "Eligible activation groups reclaimed.\n".to_string()
        } else {
            match j.rclactgrp(&name) {
                Ok(()) => format!("Activation group {name} reclaimed.\n"),
                Err(e) => format!("{e}\n"),
            }
        }
    })
}

/// `dspmsgq [*PGM|*EXT|QSYSOPR]`.
fn dspmsgq(system: &mut System, args: &[&str]) -> Result<String, String> {
    let which = args
        .first()
        .map_or_else(|| "*EXT".to_string(), |s| s.to_ascii_uppercase());
    let q = match which.as_str() {
        "*PGM" | "*SAME" => QueueRef::Same,
        "QSYSOPR" | "*SYSOPR" => QueueRef::SysOpr,
        _ => QueueRef::Ext,
    };
    with_job(system, |j| j.msgq_text(q))
}

/// `rpy KEY REPLY`: answer an inquiry on the external queue, or queue
/// the reply for the next one.
fn rpy(system: &mut System, args: &[&str]) -> Result<String, String> {
    let (key, reply) = match args {
        [k, r] => (k.to_string(), r.to_ascii_uppercase()),
        [r] => (String::new(), r.to_ascii_uppercase()),
        _ => return Err("rpy [KEY] REPLY".into()),
    };
    with_job(system, |j| {
        let k = u32::from_str_radix(&key, 16).ok();
        if let Some(k) = k
            && let Some(m) = j.ext_msgq.msgs.iter_mut().find(|m| m.key == k)
        {
            m.reply = Some(reply.clone());
            return format!("Reply {reply} recorded for message {key}.\n");
        }
        j.inquiry_replies.push_back(reply.clone());
        format!("Reply {reply} queued for the next inquiry.\n")
    })
}

/// `wrkmsgd LIB/MSGF`: the identifiers of a message file.
fn wrkmsgd(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "MSGF")?;
    let (lib, _) = qualified(&qual)?;
    let (id, _) = find(system, &qual, types::MSGF)?;
    in_store(system, &lib, |txn, _, _| {
        let f = msgf::store::load(txn, id).map_err(|e| e.to_string())?;
        let mut out = format!("Message file {qual}: {} descriptions\n", f.msgs.len());
        for d in &f.msgs {
            let _ = writeln!(out, "  {} {:02} {}", d.id, d.severity, d.text);
        }
        Ok(out)
    })
}

/// `dspmsgd ID [MSGF(LIB/F)]`.
fn dspmsgd(system: &mut System, p: &Params) -> Result<String, String> {
    let id = p.pos_or(0, "MSGID")?.to_ascii_uppercase();
    let msgf = p.kw("MSGF").map_or_else(
        || {
            format!(
                "QSYS/{}",
                match id.get(..3) {
                    Some("CEE") => "QCEEMSG",
                    Some("SQL") => "QSQLMSG",
                    _ => "QCPFMSG",
                }
            )
        },
        str::to_string,
    );
    let (lib, fname) = qualified(&msgf)?;
    let (fid, _) = find(system, &msgf, types::MSGF)?;
    in_store(system, &lib, |txn, _, _| {
        let f = msgf::store::load(txn, fid).map_err(|e| e.to_string())?;
        let d = f
            .find(&id)
            .ok_or_else(|| format!("{id} is not in {msgf}"))?;
        Ok(msgf::store::dspmsgd(&fname.to_string(), d))
    })
}

/// `strdbg LIB/PGM`.
fn strdbg(system: &mut System, p: &Params) -> Result<String, String> {
    let qual = p.pos_or(0, "PGM")?;
    let (lib, name) = qual
        .split_once('/')
        .map_or(("*LIBL", qual.as_str()), |(l, n)| (l, n));
    let (lib, name) = (lib.to_ascii_uppercase(), name.to_ascii_uppercase());
    with_job(system, |j| match j.strdbg(&name, &lib) {
        Ok(key) => format!("{key} is under debug; its next call runs the debug view.\n"),
        Err(e) => format!("{e}\n"),
    })
}

/// `addbkp N [PGM(LIB/PGM)]` and `rmvbkp N|*ALL [PGM(..)]`.
fn addbkp(system: &mut System, p: &Params, add: bool) -> Result<String, String> {
    let n = p.pos_or(0, "STMT")?;
    let pgm = p.upper("PGM");
    with_job(system, |j| {
        let program = pgm.unwrap_or_else(|| {
            j.debug
                .as_ref()
                .and_then(|d| d.programs.last().cloned())
                .unwrap_or_default()
        });
        if program.is_empty() {
            return "No program is under debug; strdbg first or give PGM().\n".into();
        }
        if add {
            match n.parse::<u32>() {
                Ok(stmt) => {
                    j.addbkp(&program, stmt);
                    format!("Breakpoint at {program} statement {stmt}.\n")
                }
                Err(_) => "addbkp N\n".into(),
            }
        } else {
            let gone = match n.parse::<u32>() {
                Ok(stmt) => j.rmvbkp(&program, Some(stmt)),
                Err(_) => j.rmvbkp(&program, None),
            };
            format!("{gone} breakpoint(s) removed.\n")
        }
    })
}
