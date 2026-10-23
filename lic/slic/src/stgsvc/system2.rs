// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Panel data for the second batch of system displays (request 270):
//! programs, service programs and modules, binding directories,
//! commands, device descriptions, hardware resources and the objects
//! of a user profile. Each answers the variables and list entries the
//! job's driver (`work/qsjob/src/panels/system2.rs`) shows.

use std::fmt::Write as _;

use ile::object::{ExportKind, ImportTarget, PgmKind, ProgramObject, UserProfile};
use qsname::Name;
use sec::profile::index::Kind;
use stg::proto::Sys2Data;
use store::obj::header::{Authority as A, text_of};
use store::obj::library::Library;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use crate::hwrsrc;
use crate::ipl::system::System;
use crate::svc::db::with_txn;

type Vars = Vec<(String, String)>;
type Rows = Vec<Vec<(String, String)>>;

/// The data of display `what` for `args`.
pub fn panel(system: &mut System, what: &str, args: &[String]) -> Result<Sys2Data, String> {
    let arg = |i: usize| {
        args.get(i)
            .map(|s| s.trim().to_ascii_uppercase())
            .unwrap_or_default()
    };
    match what {
        "PGM" | "SRVPGM" => program(system, &arg(0), &arg(1), what == "SRVPGM"),
        "MODULE" => module(system, &arg(0), &arg(1)),
        "BNDDIR" => bnddir(system, &arg(0), &arg(1)),
        "CMD" => command(system, &arg(0), &arg(1)),
        "DEVD" => devds(system),
        "HDWRSC" => Ok(hardware(system, &arg(0))),
        "UPRF" => user_objects(system, &arg(0), &arg(1)),
        other => Err(format!("CPF9899 no panel data named {other}")),
    }
}

/// `(variable, value)` pairs as owned strings.
fn vars(pairs: &[(&str, String)]) -> Vars {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}

fn yes(b: bool) -> String {
    if b { "*YES" } else { "*NO" }.to_string()
}

/// `MM/DD/YY` of Unix seconds, blank for 0.
fn mdy(t: u64) -> String {
    if t == 0 {
        return String::new();
    }
    let c = sec::cmd::civil(t);
    format!("{}/{}/{}", &c[5..7], &c[8..10], &c[2..4])
}

/// `MM/DD/YY  HH:MM:SS` of Unix seconds, blank for 0.
fn mdy_hms(t: u64) -> String {
    if t == 0 {
        return String::new();
    }
    let s = t % 86_400;
    format!(
        "{}  {:02}:{:02}:{:02}",
        mdy(t),
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

/// Kilobytes, rounded up.
fn kb(bytes: u64) -> String {
    bytes.div_ceil(1024).to_string()
}

/// An object by library (a name, `*LIBL`, `*CURLIB` or blank), name
/// and type: its library's name and its id.
fn locate(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    lib: &str,
    name: &str,
    type_code: u16,
) -> Result<(String, ObjectId), String> {
    let n = Name::parse(name).map_err(|e| format!("CPD0078 {e}"))?;
    let names = crate::svc::ile::name_context(txn, qsys);
    let libs: Vec<ObjectId> = match lib {
        "" | "*LIBL" => names.libl.clone(),
        "*CURLIB" => names.curlib.into_iter().collect(),
        l => vec![
            ops::find_library(txn, qsys, Name::parse(l).map_err(|e| format!("CPD0078 {e}"))?)
                .map_err(|_| format!("CPF2110 Library {l} not found."))?
                .id,
        ],
    };
    for l in libs {
        let lb = Library::load(txn, l).map_err(|e| e.to_string())?;
        if let Some(id) = lb.lookup(txn, type_code, n).map_err(|e| e.to_string())? {
            let lname = if l == qsys {
                "QSYS".to_string()
            } else {
                ops::describe(txn, l)
                    .map(|h| h.name.to_string())
                    .unwrap_or_default()
            };
            return Ok((lname, id));
        }
    }
    let shown = if lib.is_empty() { "*LIBL" } else { lib };
    Err(format!("CPF9801 Object {name} in library {shown} not found."))
}

/// Run `f` over the object store with `QSYS`.
fn in_store<T>(
    system: &mut System,
    f: impl FnOnce(&mut Txn<'_>, ObjectId) -> Result<T, String>,
) -> Result<T, String> {
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    with_txn(&mut s.store, |txn| f(txn, qsys))
}

/// The header variables every program display shows.
fn header(txn: &mut Txn<'_>, id: ObjectId, lib: &str) -> Result<Vars, String> {
    let h = ops::describe(txn, id).map_err(|e| e.to_string())?;
    Ok(vars(&[
        ("PGM", h.name.to_string()),
        ("PGMLIB", lib.to_string()),
        ("OWNER", text_of(&h.owner)),
        ("PGMATTR", text_of(&h.attribute)),
        ("CRTDT", mdy_hms(h.created)),
    ]))
}

/// `DSPPGM` and `DSPSRVPGM`: the attributes, then the modules, the
/// service programs and the exports as lists.
fn program(system: &mut System, lib: &str, name: &str, srvpgm: bool) -> Result<Sys2Data, String> {
    let ty = if srvpgm { types::SRVPGM } else { types::PGM };
    in_store(system, |txn, qsys| {
        let (lib, id) = locate(txn, qsys, lib, name, ty)?;
        let mut v = header(txn, id, &lib)?;
        let p = ile::store::load_program(txn, id).map_err(|e| e.to_string())?;
        v.extend(program_vars(&p));
        Ok((v, program_lists(&p)))
    })
}

fn program_vars(p: &ProgramObject) -> Vars {
    let (pep, pepl) = p.entry.map_or_else(
        || ("*NONE".to_string(), String::new()),
        |(m, _)| {
            let b = &p.modules[m as usize];
            (b.module.name.clone(), b.lib.clone())
        },
    );
    let code: u64 = p
        .modules
        .iter()
        .map(|m| m.module.code.code.len() as u64)
        .sum();
    let unresolved = p
        .imports
        .iter()
        .filter(|i| matches!(i.target, ImportTarget::Unresolved))
        .count();
    let sig = p.signature().map(|s| s.to_string()).unwrap_or_default();
    vars(&[
        ("PGMTYPE", "ILE".into()),
        ("PEPMOD", pep),
        ("PEPMODL", pepl),
        ("EXPSRCF", if p.binder_source.is_some() { "*BINDER" } else { "*NONE" }.into()),
        ("EXPMBR", String::new()),
        ("ACTGRP", p.attr.actgrp.to_string()),
        ("SHRACTGRP", "*NO".into()),
        ("CURSIG", sig),
        (
            "USRPRF",
            match p.attr.usrprf {
                UserProfile::User => "*USER",
                UserProfile::Owner => "*OWNER",
            }
            .into(),
        ),
        ("USEADPAUT", yes(p.attr.useadpaut)),
        ("CCSID", "65535".into()),
        ("NBRMOD", p.modules.len().to_string()),
        ("NBRSRV", p.srvpgms.len().to_string()),
        ("NBRUNRES", unresolved.to_string()),
        ("NBRCPYR", "0".into()),
        ("ALLCRTDTA", "*NO".into()),
        ("OBSCMPR", "*NO".into()),
        ("RUNCMPR", "*NO".into()),
        ("ALWUPD", yes(p.attr.alwupd)),
        ("ALWLIBUPD", yes(p.attr.alwlibupd)),
        ("PRFDTA", "*NOCOL".into()),
        ("TSPEP", "*NO".into()),
        ("TSMOD", "*NO".into()),
        ("STGMDL", "*SNGLVL".into()),
        ("PGMSTATE", if p.attr.user_state { "*USER" } else { "*SYSTEM" }.into()),
        ("PGMDOMAIN", if p.attr.user_domain { "*USER" } else { "*SYSTEM" }.into()),
        ("OBSERV", if p.attr.observable { "*ALL" } else { "*NONE" }.into()),
        ("TGTRLS", p.attr.tgtrls.clone()),
        ("LVLCHK", yes(p.attr.lvlchk)),
        ("SIZECUR", kb(code + p.static_size)),
        ("SIZEMAX", "268435456".into()),
        ("MODCUR", p.modules.len().to_string()),
        ("MODMAX", "32767".into()),
        ("SRVCUR", p.srvpgms.len().to_string()),
        ("SRVMAX", "32766".into()),
        ("STCCUR", kb(p.static_size)),
        ("STCMAX", "16777216".into()),
        (
            "KIND",
            if p.attr.kind == PgmKind::SrvPgm { "SRVPGM" } else { "PGM" }.into(),
        ),
    ])
}

fn program_lists(p: &ProgramObject) -> Vec<(String, Rows)> {
    let mods = p
        .modules
        .iter()
        .map(|b| {
            vars(&[
                ("MODNAME", b.module.name.clone()),
                ("MODLIB", b.lib.clone()),
                ("MODATTR", b.module.ir.lang.name().to_string()),
                ("MODDATE", mdy(b.module.created)),
                ("MODOPT", "10".into()),
                ("MODDBG", yes(b.module.debug_code.is_some())),
            ])
        })
        .collect();
    let srvs = p
        .srvpgms
        .iter()
        .map(|s| {
            vars(&[
                ("SRVNAME", s.name.clone()),
                ("SRVLIB", s.lib.clone()),
                ("SRVACT", "*IMMED".into()),
                ("SRVSIG", s.signature.to_string()),
            ])
        })
        .collect();
    let procs = p
        .exports
        .iter()
        .filter(|e| e.kind != ExportKind::Data)
        .map(|e| vars(&[("PROCNAME", e.name.clone()), ("ARGOPT", "*NO".into())]))
        .collect();
    let datas = p
        .exports
        .iter()
        .filter(|e| e.kind == ExportKind::Data)
        .map(|e| vars(&[("DATANAME", e.name.clone()), ("DATASIZE", String::new())]))
        .collect();
    let sigs = p
        .signatures
        .iter()
        .map(|s| vars(&[("PROCNAME", s.to_string()), ("ARGOPT", String::new())]))
        .collect();
    vec![
        ("MODS".into(), mods),
        ("SRVS".into(), srvs),
        ("PROCEXP".into(), procs),
        ("DATAEXP".into(), datas),
        ("SIGS".into(), sigs),
    ]
}

/// `DSPMOD`: a module's attributes and procedures.
fn module(system: &mut System, lib: &str, name: &str) -> Result<Sys2Data, String> {
    in_store(system, |txn, qsys| {
        let (lib, id) = locate(txn, qsys, lib, name, types::MODULE)?;
        let mut v = header(txn, id, &lib)?;
        let m = ile::store::load_module(txn, id).map_err(|e| e.to_string())?;
        let c = &m.create;
        v.extend(vars(&[
            ("SRCF", c.src_file.clone()),
            ("SRCFLIB", c.src_lib.clone()),
            ("SRCMBR", c.src_mbr.clone()),
            ("COMPILER", c.compiler.clone()),
            ("OPTIONS", c.options.clone()),
            ("DBGVIEW", m.ir.options.dbgview.name().to_string()),
            ("DBGDATA", yes(m.debug_code.is_some())),
            ("MODACTGRP", m.ir.actgrp.to_string()),
            ("PEP", yes(m.ir.entry().is_some())),
            ("NBRPROC", m.ir.procs.len().to_string()),
            ("STCSIZE", m.code.statics.size.to_string()),
            ("CODESIZE", m.code.code.len().to_string()),
            ("CODEGEN", m.code.version.to_string()),
        ]));
        let procs = m
            .ir
            .procs
            .iter()
            .map(|p| {
                vars(&[
                    ("PROCNAME", p.name.clone()),
                    ("PROCSCOPE", if p.export { "*EXPORT" } else { "*LOCAL" }.into()),
                    ("PROCPEP", if p.entry { "*PEP" } else { "" }.into()),
                ])
            })
            .collect();
        Ok((v, vec![("PROCS".into(), procs)]))
    })
}

/// `DSPBNDDIR`: the entries in order.
fn bnddir(system: &mut System, lib: &str, name: &str) -> Result<Sys2Data, String> {
    in_store(system, |txn, qsys| {
        let (lib, id) = locate(txn, qsys, lib, name, types::BNDDIR)?;
        let d = ile::store::load_bnddir(txn, id).map_err(|e| e.to_string())?;
        let rows = d
            .entries
            .iter()
            .map(|e| {
                vars(&[
                    ("ENTOBJ", e.name.clone()),
                    ("ENTTYPE", e.kind.type_name().to_string()),
                    ("ENTLIB", e.lib.clone()),
                    ("ENTACT", if e.defer { "*DEFER" } else { "*IMMED" }.into()),
                    ("ENTDATE", String::new()),
                    ("ENTTIME", String::new()),
                ])
            })
            .collect();
        Ok((
            vars(&[("BNDDIR", name.to_string()), ("BNDLIB", lib)]),
            vec![("ENTS".into(), rows)],
        ))
    })
}

/// `DSPCMD`: a command definition's attributes.
fn command(system: &mut System, lib: &str, name: &str) -> Result<Sys2Data, String> {
    use cmd::def::Qual;
    in_store(system, |txn, qsys| {
        let (lib, id) = locate(txn, qsys, lib, name, types::CMD)?;
        let o = cmd::store::load(txn, id).map_err(|e| e.to_string())?;
        let d = &o.def;
        let name_of = |q: &Option<Qual>| q.as_ref().map_or("*NONE".to_string(), |q| q.name.clone());
        let lib_of = |q: &Option<Qual>| q.as_ref().map_or(String::new(), |q| q.lib.clone());
        // Three words to a row, ten wide, as the modes and places show.
        let rows = |words: Vec<&str>| -> Vec<String> {
            words
                .chunks(3)
                .map(|c| {
                    c.iter()
                        .fold(String::new(), |mut s, w| {
                            let _ = write!(s, "{w:<10} ");
                            s
                        })
                        .trim_end()
                        .to_string()
                })
                .collect()
        };
        let modes: Vec<String> = d.mode.names().iter().map(|m| (*m).to_string()).collect();
        let allow = rows(d.allow.names());
        let get = |v: &[String], i: usize| v.get(i).cloned().unwrap_or_default();
        Ok((
            vars(&[
                ("CMD", d.name.clone()),
                ("CMDLIB", lib),
                ("PGM", d.cpp.name.clone()),
                ("PGMLIB", d.cpp.lib.clone()),
                ("PGMSTATE", "*USER".into()),
                ("SRCF", String::new()),
                ("SRCFLIB", String::new()),
                ("SRCMBR", String::new()),
                (
                    "VLDCKR",
                    d.vldckr.as_ref().map_or("*NONE".to_string(), Qual::text),
                ),
                ("MODE1", get(&modes, 0)),
                ("MODE2", get(&modes, 1)),
                ("MODE3", get(&modes, 2)),
                ("ALLOW1", get(&allow, 0)),
                ("ALLOW2", get(&allow, 1)),
                ("ALLOW3", get(&allow, 2)),
                ("ALWLMTUSR", yes(d.alwlmtusr)),
                ("MAXPOS", d.maxpos.map_or("*NOMAX".to_string(), |m| m.to_string())),
                ("PMTFILE", name_of(&d.pmtfile)),
                ("PMTFILEL", lib_of(&d.pmtfile)),
                ("MSGTXT", if d.pmtfile.is_some() { "*DYNAMIC" } else { "" }.into()),
                ("MSGF", d.msgf.name.clone()),
                ("MSGFLIB", d.msgf.lib.clone()),
                ("CURLIB", d.curlib.clone().unwrap_or_else(|| "*NOCHG".into())),
                ("PRDLIB", d.prdlib.clone().unwrap_or_else(|| "*NOCHG".into())),
                ("HLPPNLGRP", name_of(&d.hlppnlgrp)),
                ("HLPPNLGRPL", lib_of(&d.hlppnlgrp)),
                ("HLPID", d.hlpid.clone().unwrap_or_else(|| "*NONE".into())),
                (
                    "PMTOVRPGM",
                    d.pmtovrpgm.as_ref().map_or("*NONE".to_string(), Qual::text),
                ),
                ("ENBGUI", "*NO".into()),
                (
                    "THDSAFE",
                    match d.thdsafe {
                        cmd::def::ThdSafe::No => "*NO",
                        cmd::def::ThdSafe::Cond => "*COND",
                        cmd::def::ThdSafe::Yes => "*YES",
                    }
                    .into(),
                ),
                ("MLTTHDACN", "*SYSVAL".into()),
                ("CCSID", "37".into()),
                ("HLPSCHIDX", "*NONE".into()),
                ("CMDTEXT", d.text.clone()),
            ]),
            Vec::new(),
        ))
    })
}

/// `WRKDEVD`: name, type and text of every device description.
fn devds(system: &mut System) -> Result<Sys2Data, String> {
    in_store(system, |txn, qsys| {
        let lib = Library::load(txn, qsys).map_err(|e| e.to_string())?;
        let mut rows = Vec::new();
        for (key, id) in lib
            .list(txn, Some(types::DEVD))
            .map_err(|e| e.to_string())?
        {
            let b = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
            let d = wm::obj::devd::Devd::decode(&b).map_err(|e| e.to_string())?;
            rows.push(vars(&[
                ("DEVD", key.name.to_string()),
                ("DEVTYPE", d.devtype.clone()),
                ("DEVTEXT", d.text.clone()),
            ]));
        }
        Ok((Vec::new(), vec![("DEVS".into(), rows)]))
    })
}

/// The kinds of resource a `TYPE` of `WRKHDWRSC`/`DSPHDWRSC` takes, and
/// the word for them in the title.
fn kinds(ty: &str) -> (&'static [hwrsrc::Kind], &'static str) {
    use hwrsrc::Kind as K;
    match ty {
        "*CMN" => (&[K::Bridge, K::Lan, K::Comm], "Communication"),
        "*STG" => (&[K::Storage, K::Disk, K::Optical], "Storage"),
        "*LWS" => (&[K::Keyboard], "Local Work Station"),
        "*PRC" => (&[K::Bridge], "Processor"),
        _ => (
            &[
                K::Bridge,
                K::Lan,
                K::Storage,
                K::Disk,
                K::Optical,
                K::Comm,
                K::Keyboard,
                K::Other,
            ],
            "Hardware",
        ),
    }
}

/// A resource's type as the list shows it.
fn kind_code(k: hwrsrc::Kind) -> &'static str {
    use hwrsrc::Kind as K;
    match k {
        K::Bridge => "BRDG",
        K::Lan => "LAN",
        K::Storage => "STG",
        K::Disk => "DISK",
        K::Optical => "OPT",
        K::Comm => "CMN",
        K::Keyboard => "KBD",
        K::Other => "OTH",
    }
}

/// `WRKHDWRSC`/`DSPHDWRSC`: the resources of a kind; the table is
/// read from sysfs the first time the service is asked.
fn hardware(system: &mut System, ty: &str) -> Sys2Data {
    if system.hw.resources.is_empty() {
        system.hw = hwrsrc::Table::coldplug();
    }
    let (want, word) = kinds(ty);
    let rows = system
        .hw
        .resources
        .iter()
        .filter(|r| want.contains(&r.kind))
        .map(|r| {
            vars(&[
                ("RSCNAME", r.name.clone()),
                ("RSCTYPE", kind_code(r.kind).to_string()),
                (
                    "RSCSTS",
                    match r.status {
                        hwrsrc::Status::Operational => "Operational",
                        hwrsrc::Status::NotDetected => "Not detected",
                    }
                    .into(),
                ),
                ("RSCTEXT", r.description.clone()),
            ])
        })
        .collect();
    (
        vars(&[("RSCKIND", word.to_string())]),
        vec![("RSCS".into(), rows)],
    )
}

/// ` X` where `bits` has `mask`.
fn x(bits: u32, mask: u32) -> String {
    if bits & mask == 0 { "" } else { " X" }.to_string()
}

/// `DSPUSRPRF TYPE(*OBJAUT|*OBJOWN|*OBJPGP|*CMDAUT|*DEVAUT|*GRPMBR)`.
fn user_objects(system: &mut System, user: &str, ty: &str) -> Result<Sys2Data, String> {
    let user = user.to_string();
    let ty = ty.to_string();
    crate::svc::security::in_store(system, move |txn, env| {
        let user = if user.is_empty() || user == "*CURRENT" {
            env.actor.user.clone()
        } else {
            user
        };
        if user != env.actor.user {
            let (pid, _) = sec::profile::objects::read(txn, env.qsys, &user)?;
            store::obj::guard::check_id(txn, "usrprf.display", pid)?;
        }
        let head = vec![("UPNAME".to_string(), user.clone())];
        if ty == "*GRPMBR" {
            let mut rows = Vec::new();
            for m in sec::profile::groups::members(txn, env.qsys, &user)? {
                let (_, p) = sec::profile::objects::read(txn, env.qsys, &m)?;
                rows.push(vars(&[("MBRNAME", m.clone()), ("MBRTEXT", p.text.clone())]));
            }
            return Ok((head, vec![("MBRS".into(), rows)]));
        }
        let kind = match ty.as_str() {
            "*OBJOWN" => Kind::Owned,
            "*OBJPGP" => Kind::Group,
            _ => Kind::Private,
        };
        let mut auts = Vec::new();
        let mut owns = Vec::new();
        let mut cmds = Vec::new();
        let mut devs = Vec::new();
        for (id, bits) in sec::profile::index::list(txn, env.qsys, &user, kind)? {
            let h = sec::auth::grant::header(txn, id)?;
            let bits = match kind {
                Kind::Owned => sec::auth::bits::Auth::of(h.owner_authority).0,
                _ => u32::try_from(bits).unwrap_or(0),
            };
            let lib = sec::cmd::object::library_of(txn, &h);
            let tname = types::type_by_code(id.type_code).map_or("*UNKNOWN", |t| t.name);
            let name = h.name.to_string();
            let aut = sec::auth::bits::Auth(bits)
                .display(id.type_code == types::AUTL)
                .to_string();
            match ty.as_str() {
                "*CMDAUT" if id.type_code == types::CMD => cmds.push(vars(&[
                    ("CMDNAME", name),
                    ("CMDLIB", lib),
                    ("CMDAUT", aut),
                ])),
                "*DEVAUT" if id.type_code == types::DEVD => {
                    devs.push(vars(&[("DEVNAME", name), ("DEVAUT", aut)]));
                }
                "*CMDAUT" | "*DEVAUT" => {}
                "*OBJOWN" => owns.push(vars(&[
                    ("OOBJ", name),
                    ("OOBJLIB", lib),
                    ("OOBJTYPE", tname.to_string()),
                    ("OASPDEV", "*SYSBAS".into()),
                    ("OHOLDER", String::new()),
                ])),
                _ => auts.push(vars(&[
                    ("OBJ", name),
                    ("OBJLIB", lib),
                    ("OBJTYPE", tname.to_string()),
                    ("ASPDEV", "*SYSBAS".into()),
                    ("OPR", x(bits, A::OBJOPR)),
                    ("MGT", x(bits, A::OBJMGT)),
                    ("EXIST", x(bits, A::OBJEXIST)),
                    ("ALTER", x(bits, A::OBJALTER)),
                    ("REF", x(bits, A::OBJREF)),
                    ("READ", x(bits, A::READ)),
                    ("ADD", x(bits, A::ADD)),
                    ("UPD", x(bits, A::UPD)),
                    ("DLT", x(bits, A::DLT)),
                    ("EXE", x(bits, A::EXECUTE)),
                ])),
            }
        }
        let total = auts.len() + owns.len();
        let mut v = head;
        v.push(("TOTAL".into(), total.to_string()));
        Ok((
            v,
            vec![
                ("AUTS".into(), auts),
                ("OWNS".into(), owns),
                ("CMDS".into(), cmds),
                ("DEVS".into(), devs),
            ],
        ))
    })
}
