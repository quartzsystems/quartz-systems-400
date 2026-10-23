// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ILE commands the storage service runs: assembling IR, creating
//! and updating programs and binding directories, displaying objects,
//! and message descriptions. Calls, the job displays and the debugger
//! run in the job (`qsjob`).

use std::fmt::Write as _;

use ile::store::{Ctx, ObjSpec};
use ile::{
    ActGrpAttr, BindOptions, BndDirEntry, BndEntryKind, CreateData, ExportSpec, ModuleObject,
    PgmKind,
};
use qsname::{LibSpec, MemberSpec, Name};
use store::obj::ops::{self, NameContext};
use store::obj::{ObjectId, types};
use store::txn::Txn;

use super::super::db::{Params, with_txn};
use crate::ipl::system::{System, now_unix};

/// The commands of this module, or none.
pub fn command(system: &mut System, cmd: &str, _args: &[&str], line: &str) -> Option<String> {
    let p = Params::parse(line);
    let r = match cmd {
        "qir" => qir_cmd(system, &p),
        "crtpgm" => crtpgm(system, &p, PgmKind::Pgm),
        "crtsrvpgm" => crtpgm(system, &p, PgmKind::SrvPgm),
        "updpgm" => updpgm(system, &p),
        "crtbnddir" => crtbnddir(system, &p),
        "addbnddire" => addbnddire(system, &p),
        "dsppgm" => display(system, &p, types::PGM),
        "dspsrvpgm" => display(system, &p, types::SRVPGM),
        "dspmod" => display(system, &p, types::MODULE),
        "dsppgmref" => display(system, &p, u16::MAX),
        "dspbnddir" => display(system, &p, types::BNDDIR),
        "wrkmsgd" => wrkmsgd(system, &p),
        "dspmsgd" => dspmsgd(system, &p),
        c if super::JOB_COMMANDS.contains(&c) => Err(format!("{c} runs in a job, not here")),
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
pub(crate) fn in_store<T>(
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

/// Create program `lib/name` from QIR text when it does not exist yet:
/// a module, bound into the program, then dropped. Whether it was made.
pub fn ship_program(
    system: &mut System,
    lib: &str,
    qir_text: &str,
    text: &str,
) -> Result<bool, String> {
    let module = qir::parse(qir_text).map_err(|e| format!("{e}"))?;
    let name = module.name.clone();
    if find(system, &format!("{lib}/{name}"), types::PGM).is_ok() {
        return Ok(false);
    }
    let n = Name::parse(&name).map_err(|e| e.to_string())?;
    let create = CreateData {
        compiler: "qirc".into(),
        ..CreateData::default()
    };
    let object = ModuleObject::compile(module, create, now_unix()).map_err(|e| e.to_string())?;
    let mut options = BindOptions::new(&name, PgmKind::Pgm);
    options.attr.actgrp = ActGrpAttr::Caller;
    let spec = ObjSpec {
        lib: LibSpec::Name(Name::parse(lib).map_err(|e| e.to_string())?),
        name: n,
    };
    in_store(system, lib, |txn, c, library| {
        let m = ile::store::crtmod(txn, c, library, n, &object, text).map_err(|e| e.to_string())?;
        ile::store::crtpgm(txn, c, library, &[spec], &[], options, text)
            .map_err(|e| e.to_string())?;
        ops::delete(txn, store::obj::JobHandle(0), m).map_err(|e| e.to_string())
    })?;
    Ok(true)
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
    options.attr.usrprf = match p.upper("USRPRF").as_deref() {
        None | Some("*USER") => ile::UserProfile::User,
        Some("*OWNER") => ile::UserProfile::Owner,
        Some(v) => return Err(format!("CPD0084 USRPRF({v}) is not valid")),
    };
    // Only IBM makes system-state programs; development images let the
    // harness make one to test state (`SECURITY.md` §6).
    match p.upper("STATE").as_deref() {
        None | Some("*USER") => {}
        Some("*SYSTEM") if system.boot.svc => {
            options.attr.user_state = false;
            options.attr.user_domain = false;
        }
        Some(v) => return Err(format!("CPD0084 STATE({v}) is not valid here")),
    }
    let mut note = String::new();
    options.attr.useadpaut = match p.upper("USEADPAUT").as_deref() {
        None | Some("*YES") => {
            let ok = useadpaut_allowed(system);
            if !ok {
                note = "QUSEADPAUT: you are not on its list, so the program does not use adopted authority.\n".into();
            }
            ok
        }
        Some("*NO") => false,
        Some(v) => return Err(format!("CPD0084 USEADPAUT({v}) is not valid")),
    };
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
    let adopts = options.attr.usrprf == ile::UserProfile::Owner;
    let diags = in_store(system, &lib, |txn, c, library| {
        ile::store::crtpgm(txn, c, library, &modules, &bnddirs, options, &text)
            .map(|(_, d)| d)
            .map_err(|e| e.to_string())
    })?;
    if adopts {
        let owner = crate::stgsvc::security::actor().map_or_else(|| "QSYS".to_string(), |p| p.user);
        let ty = if kind == PgmKind::Pgm { "*PGM" } else { "*SRVPGM" };
        sec::audit::record(
            sec::audit::Event::new(sec::audit::Kind::Pa)
                .set("ETYP", "A")
                .object(&name.to_string(), &lib, ty)
                .set("OWNR", owner)
                .set("UPRF", "*OWNER"),
        );
    }
    Ok(format!(
        "{}{note}{what} {lib}/{name} created.\n",
        diag_text(&diags)
    ))
}

/// `QUSEADPAUT`: when it names an authorization list, only users with
/// `*USE` to the list create programs that use adopted authority.
fn useadpaut_allowed(system: &mut System) -> bool {
    let list = crate::stgsvc::security::sysval_text(system, "QUSEADPAUT");
    let list = list.trim();
    if list.is_empty() || list.eq_ignore_ascii_case("*NONE") {
        return true;
    }
    let Some(s) = system.storage.as_mut() else {
        return true;
    };
    let Some(qsys) = s.qsys else {
        return true;
    };
    let Ok(mut txn) = s.store.begin() else {
        return false;
    };
    match sec::auth::autl::find(&mut txn, qsys, list) {
        Ok(id) => sec::guard::allowed(&mut txn, qsys, "autl.use", id),
        Err(_) => true,
    }
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

/// `dsppgm`, `dspsrvpgm`, `dspmod`, `dsppgmref`, `dspbnddir`.
fn display(system: &mut System, p: &Params, type_code: u16) -> Result<String, String> {
    // The command's own keyword, or the dev word's `OBJ`.
    let kw = match type_code {
        types::SRVPGM => "SRVPGM",
        types::MODULE => "MODULE",
        types::BNDDIR => "BNDDIR",
        _ => "PGM",
    };
    let qual = p.pos_or(0, kw).or_else(|_| p.pos_or(0, "OBJ"))?;
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
                    Some("RNF" | "RNS") => "QRPGLEMSG",
                    Some("RNX" | "RNQ" | "RNI" | "RNT") => "QRNXMSG",
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
