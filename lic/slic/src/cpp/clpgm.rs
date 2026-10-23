// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CRTCLMOD`, `CRTBNDCL`, `CRTCLPGM` and `RTVCLSRC` in the storage
//! service: the source member read, the definitions of the commands
//! the source names collected, the declared files described, the
//! program compiled to IR, its listing spooled to `QSYSPRT`, the
//! module and the program created.

use std::fmt::Write as _;

use cmd::analyze::{CmdSource, Env, MemSource, Setting};
use cmd::block::Parms;
use ile::store::ObjSpec;
use ile::{ActGrpAttr, BindOptions, CreateData, ModuleObject, PgmKind};
use qsname::{LibSpec, MemberSpec, Name};
use stg::proto::{CmdMsg, CmdReply};
use store::obj::library::Library;
use store::obj::{JobHandle, ops, types};
use sysapi::Provider;

use super::direct::{in_store, lib_or, member_text};
use super::{done, escape};
use crate::ipl::system::{System, now_unix};
use crate::svc::ile::StoreProvider;

/// A shipped CL program compiled against the shipped definitions
/// alone: the IR text, for `ship_program`.
pub fn compile_shipped(name: &str, source: &str) -> Result<String, String> {
    let mut mem = MemSource::default();
    for (n, _) in sys_cpps::defs::DEFS {
        if let Ok(o) = sys_cpps::install::object(n, 0) {
            mem.put(o.def);
        }
    }
    let mut env = Env::new(&mut mem);
    env.setting = Setting {
        interactive: false,
        program: true,
        exec: false,
        module: false,
        syntax_only: false,
    };
    let opts = cl::Options {
        name: name.to_string(),
        srcfile: "QCLSRC".into(),
        srclib: "QSYS".into(),
        srcmbr: name.to_string(),
        text: "Startup program".into(),
        ..cl::Options::default()
    };
    let c = cl::compile(source, &opts, &mut env, &mut cl::NoFiles);
    if !c.created {
        let why: Vec<String> = c.diags.iter().map(ToString::to_string).collect();
        return Err(why.join("; "));
    }
    Ok(c.qir)
}

/// Run one of the CL commands.
pub fn run(system: &mut System, name: &str, parms: &Parms) -> CmdReply {
    match name {
        "RTVCLSRC" => rtvclsrc(system, parms),
        _ => create(system, name, parms),
    }
}

/// Declared files, described through the APIs' provider over the
/// lent store.
struct StoreFiles<'a> {
    system: &'a mut System,
}

/// A display file's formats for `DCLF`: both buffers' layouts and
/// the indicators at their fronts. `None` when `lib/name` is not a
/// display file.
fn display_formats(system: &mut System, lib: &str, name: &str) -> Option<Vec<cl::Format>> {
    in_store(system, |txn, qsys, names| {
        let id = wm::obj::find(txn, qsys, names, lib, name, types::FILE).map_err(|e| e.to_string())?;
        let (_, h) = ops::read_header(txn, id).map_err(|e| e.to_string())?;
        if ccsid::decode_trimmed(&h.attribute) != crate::svc::db::dspf::ATTR {
            return Ok(None);
        }
        let df = dspf::decode(&wm::obj::read(txn, id).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        Ok(Some(cl_formats(&df)))
    })
    .ok()
    .flatten()
}

/// The CL compiler's view of a display file's record formats.
pub fn cl_formats(df: &dspf::DisplayFile) -> Vec<cl::Format> {
    use dspf::TypeClass;
    df.records
        .iter()
        .map(|r| cl::Format {
            name: r.name.clone(),
            len: r.layout.output.len,
            in_len: r.layout.input.len,
            option_inds: r.layout.output.indicators.clone(),
            response_inds: r.layout.input.indicators.clone(),
            fields: r
                .fields
                .iter()
                .enumerate()
                .filter(|(_, f)| f.name.is_some())
                .map(|(i, f)| cl::FieldDef {
                    name: f.name.clone().unwrap_or_default(),
                    ty: match f.ty.class {
                        TypeClass::Zoned => 'S',
                        TypeClass::Packed => 'P',
                        TypeClass::Binary => 'B',
                        TypeClass::Float => 'F',
                        TypeClass::Date => 'L',
                        TypeClass::Time => 'T',
                        TypeClass::Timestamp => 'Z',
                        TypeClass::Char | TypeClass::Graphic => 'A',
                    },
                    pos: r.layout.output.field(i).map_or(0, |b| b.offset + 1),
                    len: f.bytes,
                    digits: if f.ty.is_numeric() { f.length } else { 0 },
                    decimals: u32::from(f.decimals),
                    in_pos: r.layout.input.field(i).map(|b| b.offset + 1),
                })
                .collect(),
        })
        .collect()
}

impl cl::Files for StoreFiles<'_> {
    fn file(&mut self, name: &str, lib: &str) -> Option<(bool, Vec<cl::Format>)> {
        let lib = lib_or(lib);
        if let Some(formats) = display_formats(self.system, &lib, name) {
            return Some((true, formats));
        }
        if crate::svc::ile::lend(self.system).is_err() {
            return None;
        }
        let mut p = StoreProvider::new(self.system.lent.clone());
        let info = p.file(&lib, name);
        let formats = p.formats(&lib, name);
        crate::svc::ile::restore(self.system);
        let info = info?;
        let formats = formats
            .into_iter()
            .map(|f| cl::Format {
                name: f.name,
                len: f.rec_len,
                fields: f
                    .fields
                    .into_iter()
                    .map(|x| cl::FieldDef {
                        name: x.name,
                        ty: x.ty,
                        pos: x.pos,
                        len: x.len,
                        digits: x.digits,
                        decimals: x.decimals,
                        in_pos: None,
                    })
                    .collect(),
                ..cl::Format::default()
            })
            .collect();
        Some((info.attr == "DSPF", formats))
    }
}

/// The definitions the source may name, as the compiler's source:
/// every word that could be a command name, looked up through the
/// job's library list.
fn definitions(system: &mut System, source: &str) -> Result<MemSource, String> {
    let mut names: Vec<String> = Vec::new();
    for line in source.lines() {
        if line.trim_start().starts_with("/*") {
            continue;
        }
        for word in line.split(|c: char| c.is_whitespace() || c == '(' || c == ')' || c == ':') {
            let w = word.trim_start_matches('?').to_ascii_uppercase();
            let ok = !w.is_empty()
                && w.len() <= 21
                && w.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                && w.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '/' || c == '_');
            if ok && !names.contains(&w) {
                names.push(w);
            }
        }
    }
    let mut mem = MemSource::default();
    in_store(system, |txn, qsys, ctx_names| {
        let mut src = cmd::store::StoreSource {
            txn,
            qsys,
            names: ctx_names.clone(),
        };
        for n in &names {
            let (lib, name) = n
                .split_once('/')
                .map_or((None, n.as_str()), |(l, nn)| (Some(l), nn));
            if let Ok(Some((o, _))) = src.lookup(name, lib) {
                mem.cmds.push((name.to_string(), o));
            }
        }
        Ok(())
    })?;
    Ok(mem)
}

/// The library an object goes to, by name: `*CURLIB` resolved.
fn target_library(system: &mut System, lib: &str) -> Result<String, String> {
    if !lib.is_empty() && lib != "*CURLIB" && lib != "*LIBL" {
        return Ok(lib.to_string());
    }
    in_store(system, |txn, _qsys, names| {
        let id = names
            .curlib
            .ok_or_else(|| "CPF2110 no current library".to_string())?;
        let h = ops::describe(txn, id).map_err(|e| e.to_string())?;
        Ok(h.name.to_string())
    })
}

/// `CRTCLMOD`, `CRTBNDCL`, `CRTCLPGM`.
fn create(system: &mut System, cmdname: &str, parms: &Parms) -> CmdReply {
    let is_module = cmdname == "CRTCLMOD";
    let kw = if is_module { "MODULE" } else { "PGM" };
    let (name, lib) = parms.qual(kw);
    let (sf, sfl) = parms.qual("SRCFILE");
    let mbr = {
        let m = parms.name("SRCMBR");
        if m.is_empty() || m == "*PGM" || m == "*MODULE" {
            name.clone()
        } else {
            m
        }
    };
    let text = parms.text("TEXT");
    let replace = parms.yes("REPLACE") || !parms.specified("REPLACE");
    let lib_name = match target_library(system, &lib) {
        Ok(l) => l,
        Err(e) => return failed(cmdname, &e),
    };
    let source = match in_store(system, |txn, qsys, names| {
        member_text(txn, qsys, names, &sf, &sfl, &mbr)
    }) {
        Ok(s) => s,
        Err(e) => return failed(cmdname, &e),
    };
    let mut mem = match definitions(system, &source) {
        Ok(m) => m,
        Err(e) => return failed(cmdname, &e),
    };
    let (options, opts) = options_of(cmdname, parms, &name, &text, &sf, &sfl, &mbr);
    let compiled = {
        let mut env = Env::new(&mut mem);
        env.setting = Setting {
            interactive: false,
            program: true,
            exec: false,
            module: is_module,
            syntax_only: false,
        };
        let mut files = StoreFiles { system };
        cl::compile(&source, &opts, &mut env, &mut files)
    };
    let mut out = String::new();
    for d in &compiled.diags {
        let _ = writeln!(out, "{d}");
    }
    let _ = match crate::svc::security::print(system, "QSYSPRT", &compiled.listing) {
        Ok(w) => writeln!(out, "Listing: {w}"),
        Err(e) => writeln!(out, "The listing was not spooled: {e}"),
    };
    if !compiled.created {
        let what = if is_module { "module" } else { "program" };
        return CmdReply {
            text: format!("{out}CPF0001 {cmdname}: the {what} was not created.\n"),
            escape: Some(CmdMsg {
                msgid: "CPD0700".into(),
                kind: 7,
                data: cmd::error::data(&[(&name, 10), (&lib_name, 10)]),
            }),
            ..CmdReply::default()
        };
    }
    let made = Made {
        is_module,
        name,
        lib_name,
        text,
        replace,
    };
    match store_object(system, &made, &opts, &options, &source, &compiled.qir) {
        Ok(()) => {
            let (what, id) = if is_module {
                ("Module", "CPC5D0B")
            } else {
                ("Program", "CPC5D07")
            };
            done(
                format!(
                    "{out}{what} {} created in library {}.
",
                    made.name, made.lib_name
                ),
                id,
                &[(&made.name, 10), (&made.lib_name, 10)],
            )
        }
        Err(e) => failed(cmdname, &format!("{out}{e}")),
    }
}

/// The compiler's options from the command's.
fn options_of(
    cmdname: &str,
    parms: &Parms,
    name: &str,
    text: &str,
    sf: &str,
    sfl: &str,
    mbr: &str,
) -> (Vec<String>, cl::Options) {
    let options: Vec<String> = parms.list("OPTION").iter().map(|v| v.source()).collect();
    let named = |kw: &str, dft: &str| {
        let v = parms.name(kw);
        if v.is_empty() { dft.to_string() } else { v }
    };
    let opts = cl::Options {
        name: name.to_string(),
        actgrp: named("ACTGRP", "*CALLER"),
        dftactgrp: cmdname == "CRTCLPGM" || parms.yes("DFTACTGRP"),
        dbgview: named("DBGVIEW", "*STMT"),
        log: named("LOG", "*JOB"),
        xref: !options.iter().any(|o| o == "*NOXREF"),
        generate: !options.iter().any(|o| o == "*NOGEN"),
        seclvl: options.iter().any(|o| o == "*SECLVL"),
        alwrtvsrc: !parms.specified("ALWRTVSRC") || parms.yes("ALWRTVSRC"),
        text: text.to_string(),
        srcfile: sf.to_string(),
        srclib: lib_or(sfl),
        srcmbr: mbr.to_string(),
    };
    (options, opts)
}

/// What is being made.
struct Made {
    is_module: bool,
    name: String,
    lib_name: String,
    text: String,
    replace: bool,
}

/// The IR compiled, the module written, the program bound.
fn store_object(
    system: &mut System,
    made: &Made,
    opts: &cl::Options,
    options: &[String],
    source: &str,
    qir_text: &str,
) -> Result<(), String> {
    let Made {
        is_module,
        name,
        lib_name,
        text,
        replace,
    } = made;
    let (is_module, replace) = (*is_module, *replace);
    let module = qir::parse(qir_text)
        .map_err(|e| format!("CPF0006 the generated IR does not parse: {e}"))?;
    qir::verify(&module).map_err(|e| format!("CPF0006 the generated IR does not verify: {e}"))?;
    let create = CreateData {
        compiler: "CLLE".into(),
        options: options.join(" "),
        src_file: opts.srcfile.clone(),
        src_lib: opts.srclib.clone(),
        src_mbr: opts.srcmbr.clone(),
    };
    let mut object =
        ModuleObject::compile(module, create, now_unix()).map_err(|e| format!("CPF0006 {e}"))?;
    if opts.alwrtvsrc {
        object.source = Some(source.lines().map(str::to_string).collect());
    }
    let n = Name::parse(name).map_err(|e| format!("CPD0078 {e}"))?;
    let library = Name::parse(lib_name).map_err(|e| format!("CPD0078 {e}"))?;
    let actgrp = if opts.dftactgrp {
        ActGrpAttr::Default
    } else {
        match opts.actgrp.as_str() {
            "*NEW" => ActGrpAttr::New,
            "*CALLER" => ActGrpAttr::Caller,
            a => ActGrpAttr::Named(a.to_string()),
        }
    };
    crate::svc::ile::commands::in_store(system, lib_name, |txn, c, lib_id| {
        let ty = if is_module { types::MODULE } else { types::PGM };
        let l = Library::load(txn, lib_id).map_err(|e| e.to_string())?;
        if let Some(old) = l.lookup(txn, ty, n).map_err(|e| e.to_string())? {
            if !replace {
                return Err(format!("CPF5D0D {name} already exists in {lib_name}"));
            }
            ops::delete(txn, JobHandle(0), old).map_err(|e| e.to_string())?;
        }
        if !is_module
            && let Some(old) = l.lookup(txn, types::MODULE, n).map_err(|e| e.to_string())?
        {
            // A module of the program's name is in the way of the
            // binder's temporary one.
            ops::delete(txn, JobHandle(0), old).map_err(|e| e.to_string())?;
        }
        let m = ile::store::crtmod(txn, c, lib_id, n, &object, text).map_err(|e| e.to_string())?;
        if is_module {
            return Ok(());
        }
        let mut options = BindOptions::new(name, PgmKind::Pgm);
        options.attr.actgrp = actgrp.clone();
        let spec = ObjSpec {
            lib: LibSpec::Name(library),
            name: n,
        };
        ile::store::crtpgm(txn, c, lib_id, &[spec], &[], options, text)
            .map_err(|e| e.to_string())?;
        ops::delete(txn, JobHandle(0), m).map_err(|e| e.to_string())
    })
}

/// `RTVCLSRC`: the source kept in the program's module, back into a
/// source member.
fn rtvclsrc(system: &mut System, parms: &Parms) -> CmdReply {
    let (name, lib) = parms.qual("PGM");
    let (sf, sfl) = parms.qual("SRCFILE");
    let mbr = {
        let m = parms.name("SRCMBR");
        if m.is_empty() || m == "*PGM" {
            name.clone()
        } else {
            m
        }
    };
    let r = in_store(system, |txn, qsys, names| {
        let found = wm::obj::find(txn, qsys, names, &lib_or(&lib), &name, types::PGM)
            .map_err(|e| format!("CPF9801 {e}"))?;
        let p = ile::store::load_program(txn, found).map_err(|e| e.to_string())?;
        let lines = p
            .modules
            .iter()
            .find_map(|m| m.module.source.clone())
            .ok_or_else(|| format!("CPF0575 program {name} keeps no source (ALWRTVSRC(*NO))"))?;
        let file = wm::obj::find(txn, qsys, names, &lib_or(&sfl), &sf, types::FILE)
            .map_err(|e| format!("CPF9801 {e}"))?;
        let m = Name::parse(&mbr).map_err(|e| format!("CPD0078 {e}"))?;
        // The member is added when the file lacks it, as RTVCLSRC does.
        let desc = db::file::describe(txn, file).map_err(|e| format!("CPF0006 {e}"))?;
        if desc.member(MemberSpec::Name(m)).is_none() {
            db::file::add_member(txn, file, m, "", Name::parse("CLLE").ok(), now_unix())
                .map_err(|e| format!("CPF0006 {e}"))?;
        }
        let text = lines.join("\n");
        db::source::srcput(
            txn,
            JobHandle(0),
            file,
            MemberSpec::Name(m),
            &text,
            Name::parse("CLLE").ok(),
            now_unix(),
        )
        .map_err(|e| format!("CPF0006 {e}"))?;
        Ok(lines.len())
    });
    match r {
        Ok(n) => done(
            format!("{n} source records retrieved into {sfl}/{sf} member {mbr}.\n"),
            "CPC0570",
            &[(&name, 10)],
        ),
        Err(e) => failed("RTVCLSRC", &e),
    }
}

/// An escape from the last line of what went wrong.
fn failed(name: &str, e: &str) -> CmdReply {
    let last = e.lines().last().unwrap_or("").trim();
    let id = super::leading_msgid(last).unwrap_or("CPF0006").to_string();
    let detail = e.trim().strip_prefix(&id).unwrap_or(e.trim()).trim();
    escape(&id, name, detail)
}
