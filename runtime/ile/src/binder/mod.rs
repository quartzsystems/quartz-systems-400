// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The binder: modules in, a program or service program out. Imports
//! resolve in IBM's order: the modules listed, then the binding
//! directories in order (a service program entry binds the service
//! program, a module entry copies the module in), then `QUSAPIBD`,
//! which the caller passes as the last directory.

pub mod lang;
pub mod signature;
pub mod sysproc;

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use lang::BinderSource;
use signature::Signature;

use crate::error::{IleError, Result};
use crate::object::{
    BndDir, BndDirEntry, BndEntryKind, BoundModule, Export, ExportKind, ImportRes, ImportTarget,
    ModuleObject, PgmAttr, PgmKind, ProgramObject, SrvPgmRef,
};

/// How bad a diagnostic is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Informational.
    Info,
    /// A warning; the object is still created.
    Warning,
    /// An error; the object is not created.
    Error,
}

/// A binder diagnostic, with the IBM identifier of its kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diag {
    /// `CPD5D02` and friends.
    pub id: String,
    /// Severity.
    pub severity: Severity,
    /// Text.
    pub text: String,
}

/// What a service program exports, as the binder needs it.
#[derive(Clone, Debug, PartialEq)]
pub struct SrvPgmInfo {
    /// Name.
    pub name: String,
    /// Library it was found in.
    pub lib: String,
    /// Exports in order.
    pub exports: Vec<(String, ExportKind)>,
    /// Current signature.
    pub signature: Signature,
}

/// What a binding directory entry resolved to.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum Resolved {
    /// A module to copy in.
    Module(ModuleObject, String),
    /// A service program to bind to.
    SrvPgm(SrvPgmInfo),
}

/// `EXPORT(*ALL)` or the binder source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportSpec {
    /// Every exported symbol of every module, in module order.
    All,
    /// The binder language.
    Source(BinderSource, String),
}

/// Options of `CRTPGM` and `CRTSRVPGM`.
#[derive(Clone, Debug, PartialEq)]
pub struct BindOptions {
    /// Object name.
    pub name: String,
    /// Attributes, including the kind.
    pub attr: PgmAttr,
    /// `ENTMOD`: the module with the entry procedure; the first with
    /// one when `None`.
    pub entry_module: Option<String>,
    /// Exports, for a service program.
    pub export: ExportSpec,
    /// `OPTION(*UNRSLVREF)`: create with unresolved imports.
    pub allow_unresolved: bool,
    /// `BNDSRVPGM`: service programs bound before any directory.
    pub bndsrvpgm: Vec<crate::object::BndDirEntry>,
}

impl BindOptions {
    /// Defaults for a name and kind.
    #[must_use]
    pub fn new(name: &str, kind: PgmKind) -> Self {
        Self {
            name: name.into(),
            attr: PgmAttr::new(kind),
            entry_module: None,
            export: ExportSpec::All,
            allow_unresolved: false,
            bndsrvpgm: Vec::new(),
        }
    }
}

/// The binder's input.
pub struct BindInput<'a> {
    /// Modules listed, with the library each came from.
    pub modules: Vec<(ModuleObject, String)>,
    /// Binding directories in order, `QUSAPIBD` last when it exists.
    pub bnddirs: Vec<BndDir>,
    /// Resolves a directory entry to its object.
    pub lookup: &'a dyn Fn(&BndDirEntry) -> Option<Resolved>,
    /// Options.
    pub options: BindOptions,
}

#[derive(Clone, Copy, Debug)]
enum Sym {
    Proc { module: u32, proc: u32 },
    Data { module: u32, offset: u64 },
}

struct Binder {
    modules: Vec<BoundModule>,
    exports: HashMap<(String, ExportKind), Sym>,
    diags: Vec<Diag>,
}

impl Binder {
    fn add_module(&mut self, m: ModuleObject, lib: String) -> u32 {
        let index = u32::try_from(self.modules.len()).expect("few modules");
        for (pi, p) in m.ir.procs.iter().enumerate() {
            if !p.export {
                continue;
            }
            let key = (p.name.clone(), ExportKind::Proc);
            match self.exports.entry(key) {
                Entry::Occupied(_) => self.diags.push(Diag {
                    id: "CPD5D04".into(),
                    severity: Severity::Warning,
                    text: format!(
                        "procedure {} is exported by more than one module; the first is used",
                        p.name
                    ),
                }),
                Entry::Vacant(v) => {
                    v.insert(Sym::Proc {
                        module: index,
                        proc: u32::try_from(pi).expect("few procedures"),
                    });
                }
            }
        }
        for (name, off) in &m.code.data_exports {
            let key = (name.clone(), ExportKind::Data);
            match self.exports.entry(key) {
                Entry::Occupied(_) => self.diags.push(Diag {
                    id: "CPD5D04".into(),
                    severity: Severity::Warning,
                    text: format!(
                        "data {name} is exported by more than one module; the first is used"
                    ),
                }),
                Entry::Vacant(v) => {
                    v.insert(Sym::Data {
                        module: index,
                        offset: *off,
                    });
                }
            }
        }
        self.modules.push(BoundModule {
            module: m,
            lib,
            frame_offset: 0,
        });
        index
    }
}

/// Bind modules into a program or service program.
#[allow(clippy::too_many_lines)]
pub fn bind(input: BindInput<'_>) -> Result<(ProgramObject, Vec<Diag>)> {
    let opts = input.options;
    let mut b = Binder {
        modules: Vec::new(),
        exports: HashMap::new(),
        diags: Vec::new(),
    };
    for (m, lib) in input.modules {
        b.add_module(m, lib);
    }
    let mut srvpgms: Vec<SrvPgmRef> = Vec::new();
    let mut srv_exports: Vec<Vec<(String, ExportKind)>> = Vec::new();
    let mut imports: Vec<ImportRes> = Vec::new();
    // Resolve module by module; modules a directory binds in join the end.
    let mut mi = 0usize;
    while mi < b.modules.len() {
        let wanted = b.modules[mi].module.imports();
        let module = u32::try_from(mi).expect("few modules");
        for (name, kind) in wanted {
            // Already resolved for this module (a module copied in twice cannot happen).
            let target = resolve_one(
                &mut b,
                &mut srvpgms,
                &mut srv_exports,
                &input.bnddirs,
                input.lookup,
                &name,
                kind,
            );
            imports.push(ImportRes {
                module,
                name,
                kind,
                target,
            });
        }
        mi += 1;
    }
    let unresolved: Vec<String> = imports
        .iter()
        .filter(|i| i.target == ImportTarget::Unresolved && system_data_size(i.kind, &i.name).is_none())
        .map(|i| format!("{} ({})", i.name, b.modules[i.module as usize].module.name))
        .collect();
    if !unresolved.is_empty() {
        b.diags.push(Diag {
            id: "CPD5D02".into(),
            severity: if opts.allow_unresolved {
                Severity::Warning
            } else {
                Severity::Error
            },
            text: format!("imports with no export: {}", unresolved.join(", ")),
        });
        if !opts.allow_unresolved {
            return Err(IleError::Unresolved(unresolved.join(", ")));
        }
    }
    // Static area: every module's frame, 16-byte aligned.
    let mut at = 0u64;
    for m in &mut b.modules {
        m.frame_offset = at;
        at += m.module.code.statics.size.div_ceil(16) * 16;
    }
    // System data the runtime does not share: zeroed storage of the
    // program's own, after the frames.
    for i in &mut imports {
        if i.target == ImportTarget::Unresolved
            && let Some(n) = system_data_size(i.kind, &i.name)
        {
            i.target = ImportTarget::Data { offset: at };
            at += n.div_ceil(16) * 16;
        }
    }
    let static_size = at;
    // Data import offsets become program static area offsets.
    for i in &mut imports {
        if let (ExportKind::Data, ImportTarget::Proc { module, proc }) = (i.kind, &i.target) {
            // Encoded during resolution as module and var offset in `proc`.
            let off = b.modules[*module as usize].frame_offset + u64::from(*proc);
            i.target = ImportTarget::Data { offset: off };
        }
    }
    // Entry.
    let entry = match opts.attr.kind {
        PgmKind::Pgm => {
            let found = b.modules.iter().enumerate().find(|(_, m)| {
                opts.entry_module
                    .as_ref()
                    .is_none_or(|n| *n == m.module.name)
                    && m.module.ir.entry().is_some()
            });
            match found {
                Some((mi, m)) => {
                    let (pr, _) = m.module.ir.entry().expect("checked");
                    Some((u32::try_from(mi).expect("few modules"), pr.0))
                }
                None => return Err(IleError::NoEntry),
            }
        }
        PgmKind::SrvPgm => None,
    };
    // Exports and signatures.
    let (exports, signatures, binder_source, lvlchk) = match (&opts.attr.kind, &opts.export) {
        (PgmKind::Pgm, _) => (Vec::new(), Vec::new(), None, opts.attr.lvlchk),
        (PgmKind::SrvPgm, ExportSpec::All) => {
            let mut ex = Vec::new();
            for (mi, m) in b.modules.iter().enumerate() {
                for (name, kind) in m.module.exports() {
                    if let Some(e) = export_of(&b, mi, &name, kind) {
                        ex.push(e);
                    }
                }
            }
            let names: Vec<String> = ex.iter().map(|e| e.name.clone()).collect();
            (
                ex,
                vec![Signature::of_exports(&names)],
                None,
                opts.attr.lvlchk,
            )
        }
        (PgmKind::SrvPgm, ExportSpec::Source(src, text)) => {
            let cur = src.current();
            let mut ex = Vec::new();
            for name in &cur.symbols {
                let found =
                    find_export(&b, name).ok_or_else(|| IleError::NoSuchExport(name.clone()))?;
                ex.push(found);
            }
            let mut sigs = vec![cur.effective_signature()];
            sigs.extend(src.previous().map(lang::ExportBlock::effective_signature));
            (
                ex,
                sigs,
                Some(text.clone()),
                cur.lvlchk.unwrap_or(opts.attr.lvlchk),
            )
        }
    };
    let mut attr = opts.attr;
    attr.lvlchk = lvlchk;
    let diags = b.diags;
    Ok((
        ProgramObject {
            name: opts.name,
            attr,
            entry,
            modules: b.modules,
            imports,
            srvpgms,
            static_size,
            exports,
            signatures,
            binder_source,
        },
        diags,
    ))
}

fn export_of(b: &Binder, mi: usize, name: &str, kind: ExportKind) -> Option<Export> {
    let module = u32::try_from(mi).expect("few modules");
    match b.exports.get(&(name.to_string(), kind))? {
        Sym::Proc { module: m, proc } if *m == module => Some(Export {
            name: name.into(),
            kind,
            module,
            index: *proc,
            offset: 0,
        }),
        Sym::Data { module: m, offset } if *m == module => Some(Export {
            name: name.into(),
            kind,
            module,
            index: 0,
            offset: b.modules[mi].frame_offset + offset,
        }),
        _ => None,
    }
}

fn find_export(b: &Binder, name: &str) -> Option<Export> {
    for kind in [ExportKind::Proc, ExportKind::Data] {
        if let Some(sym) = b.exports.get(&(name.to_string(), kind)) {
            let mi = match sym {
                Sym::Proc { module, .. } | Sym::Data { module, .. } => *module as usize,
            };
            return export_of(b, mi, name, kind);
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn resolve_one(
    b: &mut Binder,
    srvpgms: &mut Vec<SrvPgmRef>,
    srv_exports: &mut Vec<Vec<(String, ExportKind)>>,
    bnddirs: &[BndDir],
    lookup: &dyn Fn(&BndDirEntry) -> Option<Resolved>,
    name: &str,
    kind: ExportKind,
) -> ImportTarget {
    let key = (name.to_string(), kind);
    let as_target = |sym: &Sym| match (kind, sym) {
        (ExportKind::Proc, Sym::Proc { module, proc }) => ImportTarget::Proc {
            module: *module,
            proc: *proc,
        },
        // Data: carried as (module, var offset) until frames are laid out.
        (_, Sym::Data { module, offset }) => ImportTarget::Proc {
            module: *module,
            proc: u32::try_from(*offset).expect("frame fits"),
        },
        (ExportKind::Data, Sym::Proc { .. }) => ImportTarget::Unresolved,
    };
    if let Some(sym) = b.exports.get(&key) {
        return as_target(sym);
    }
    // Service programs already bound.
    for (si, ex) in srv_exports.iter().enumerate() {
        if ex.iter().any(|(n, k)| n == name && *k == kind) {
            return ImportTarget::SrvPgm {
                srvpgm: u32::try_from(si).expect("few"),
                export: name.into(),
            };
        }
    }
    for dir in bnddirs {
        for e in &dir.entries {
            match lookup(e) {
                Some(Resolved::SrvPgm(info)) => {
                    if info.exports.iter().any(|(n, k)| n == name && *k == kind) {
                        if let Some(si) = srvpgms
                            .iter()
                            .position(|s| s.name == info.name && s.lib == info.lib)
                        {
                            return ImportTarget::SrvPgm {
                                srvpgm: u32::try_from(si).expect("few"),
                                export: name.into(),
                            };
                        }
                        srvpgms.push(SrvPgmRef {
                            name: info.name.clone(),
                            lib: info.lib.clone(),
                            signature: info.signature,
                        });
                        srv_exports.push(info.exports.clone());
                        return ImportTarget::SrvPgm {
                            srvpgm: u32::try_from(srvpgms.len() - 1).expect("few"),
                            export: name.into(),
                        };
                    }
                }
                Some(Resolved::Module(m, lib)) => {
                    let has = m.exports().iter().any(|(n, k)| n == name && *k == kind);
                    let already = b
                        .modules
                        .iter()
                        .any(|x| x.module.name == m.name && x.lib == lib);
                    if has && !already {
                        b.add_module(m, lib);
                        if let Some(sym) = b.exports.get(&key) {
                            return as_target(sym);
                        }
                    }
                }
                None => {}
            }
        }
    }
    if kind == ExportKind::Proc
        && (SYSTEM_PROCS.contains(&name)
            || PRIVILEGED_PROCS.contains(&name)
            || name.starts_with(RPG_RUNTIME_PREFIX)
            || sysproc::is_system_srvpgm_proc(name)
            || REGISTRATION_PROCS.contains(&name))
    {
        return ImportTarget::System;
    }
    b.diags.push(Diag {
        id: "CPD5D02".into(),
        severity: Severity::Info,
        text: format!(
            "{} {name} is not exported by any module or service program",
            kind.name()
        ),
    });
    ImportTarget::Unresolved
}

/// Procedures the runtime provides, as `QLEAWI` exports them: resolved
/// last, after every module, service program and directory.
pub const SYSTEM_PROCS: &[&str] = &[
    "CEEHDLR", "CEEHDLU", "CEERTX", "CEEUTX", "CEESGL", "CEENCOD", "CEEDCOD", "CEEMRCR", "CEE4ABN",
    "CEETREC", "CEEDOD", "CEEGPID", "CEETSTA", "CEECRHP", "CEEDSHP", "CEEGTST", "CEEFRST",
    "CEECZST", "CEEDAYS", "CEEDATE", "CEEDATM", "CEESECS", "CEEUTC", "CEELOCT", "CEEQCEN",
    "CEESCEN", "CEEGMT", "CEEGMTO", "CEEUTCO", "CEERAN0", "CEE4RAGE",
];

/// Data exports of IBM's service programs a program may import, with
/// the bytes each takes: `QSOSRV2`'s resolver state `_res`. Each program
/// gets zeroed storage of its own; the runtime does not read it.
pub const SYSTEM_DATA: &[(&str, u64)] = &[("_res", 4096)];

/// The size of a system data import, `None` when it is not one.
fn system_data_size(kind: ExportKind, name: &str) -> Option<u64> {
    if kind != ExportKind::Data {
        return None;
    }
    SYSTEM_DATA.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, s)| *s)
}

/// The registration facility's procedures (`QUSRGFA1`, `QUSRGFA2`),
/// which the runtime provides too (`sysapi`).
pub const REGISTRATION_PROCS: &[&str] = &[
    "QusRegisterExitPoint",
    "QusDeregisterExitPoint",
    "QusAddExitProgram",
    "QusRemoveExitProgram",
    "QusRetrieveExitInformation",
];

/// The prefix of the ILE RPG runtime library's procedures (`qrpgrt`),
/// resolved as system imports like `QLEAWI`'s.
pub const RPG_RUNTIME_PREFIX: &str = "_QRNX_";

/// Procedures only system state may import: raw storage, the storage
/// service's direct calls and authority bypass. A user-state program
/// that imports one is refused at load at `QSECURITY` 40 and up
/// (`MCH6801`); no job provides them, so at 30 a call is `MCH3401`.
pub const PRIVILEGED_PROCS: &[&str] = &["_QSYSRAW", "_QSTGDIRECT", "_QAUTBYPASS"];

impl BndEntryKind {
    /// Whether an entry of this kind is looked up as a module.
    #[must_use]
    pub fn is_module(self) -> bool {
        self == Self::Module
    }
}
