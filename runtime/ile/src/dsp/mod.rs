// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display providers: the text of `DSPPGM`, `DSPSRVPGM`, `DSPMOD`,
//! `DSPPGMREF` and `DSPBNDDIR`, for the service console now and UIM
//! later. Every string is ours; the field names follow the commands.

use std::fmt::Write as _;

use crate::object::{BndDir, ExportKind, ImportTarget, ModuleObject, PgmKind, ProgramObject};

fn yes_no(b: bool) -> &'static str {
    if b { "*YES" } else { "*NO" }
}

fn line(out: &mut String, label: &str, value: impl std::fmt::Display) {
    let _ = writeln!(out, "{label:<34}{value}");
}

/// `DSPMOD`.
#[must_use]
pub fn dspmod(m: &ModuleObject) -> String {
    let mut out = String::new();
    line(&mut out, "Module", &m.name);
    line(&mut out, "Source language", m.ir.lang.name());
    line(&mut out, "Compiler", &m.create.compiler);
    line(
        &mut out,
        "Source file",
        format!(
            "{}/{} {}",
            m.create.src_lib, m.create.src_file, m.create.src_mbr
        ),
    );
    line(&mut out, "Options", &m.create.options);
    line(&mut out, "Debug view", m.ir.options.dbgview.name());
    line(&mut out, "Activation group", &m.ir.actgrp);
    line(
        &mut out,
        "Program entry procedure",
        yes_no(m.ir.entry().is_some()),
    );
    line(&mut out, "Procedures", m.ir.procs.len());
    line(&mut out, "Static storage (bytes)", m.code.statics.size);
    line(&mut out, "Code (bytes)", m.code.code.len());
    line(&mut out, "Code generator version", m.code.version);
    line(&mut out, "IR format version", qir::FORMAT_VERSION);
    out.push_str("Procedures:\n");
    for p in &m.ir.procs {
        let _ = writeln!(
            out,
            "  {:<32} {:<10} {:<6} parms {}",
            p.name,
            if p.export { "*EXPORT" } else { "*LOCAL" },
            if p.entry { "*PEP" } else { "" },
            p.params.len()
        );
    }
    let exports = m.exports();
    if exports.iter().any(|(_, k)| *k == ExportKind::Data) {
        out.push_str("Exported data:\n");
        for (n, k) in exports {
            if k == ExportKind::Data {
                let _ = writeln!(out, "  {n}");
            }
        }
    }
    let imports = m.imports();
    if !imports.is_empty() {
        out.push_str("Imports:\n");
        for (n, k) in imports {
            let _ = writeln!(out, "  {n:<32} {}", k.name());
        }
    }
    if !m.ir.files.is_empty() {
        out.push_str("Files referenced:\n");
        for f in &m.ir.files {
            let _ = writeln!(
                out,
                "  {}/{} {} {}",
                f.lib,
                f.file,
                f.dev.name(),
                f.usage.name()
            );
        }
    }
    out
}

fn program_common(out: &mut String, p: &ProgramObject) {
    line(out, "Program", &p.name);
    line(out, "Type", p.attr.kind.type_name());
    line(out, "Activation group attribute", &p.attr.actgrp);
    line(
        out,
        "User profile",
        match p.attr.usrprf {
            crate::object::UserProfile::User => "*USER",
            crate::object::UserProfile::Owner => "*OWNER",
        },
    );
    line(out, "Use adopted authority", yes_no(p.attr.useadpaut));
    line(
        out,
        "Program state",
        if p.attr.user_state {
            "*USER"
        } else {
            "*SYSTEM"
        },
    );
    line(
        out,
        "Program domain",
        if p.attr.user_domain {
            "*USER"
        } else {
            "*SYSTEM"
        },
    );
    line(out, "Allow update", yes_no(p.attr.alwupd));
    line(
        out,
        "Allow bound *SRVPGM library update",
        yes_no(p.attr.alwlibupd),
    );
    line(
        out,
        "Observable",
        if p.attr.observable { "*ALL" } else { "*NONE" },
    );
    line(out, "Target release", &p.attr.tgtrls);
    line(out, "Code generator version", p.attr.codegen_version);
    line(out, "Static storage (bytes)", p.static_size);
    if let Some((mi, pi)) = p.entry {
        let m = &p.modules[mi as usize];
        line(
            out,
            "Program entry procedure",
            format!(
                "{} in {}",
                m.module.ir.procs[pi as usize].name, m.module.name
            ),
        );
    }
    out.push_str("Modules:\n");
    for m in &p.modules {
        let _ = writeln!(
            out,
            "  {:<10} {:<10} {:<6} frame {:>8} {:>6} bytes",
            m.module.name,
            m.lib,
            m.module.ir.lang.name(),
            m.frame_offset,
            m.module.code.statics.size
        );
    }
    if !p.srvpgms.is_empty() {
        out.push_str("Service programs:\n");
        for s in &p.srvpgms {
            let _ = writeln!(out, "  {:<10} {:<10} {}", s.name, s.lib, s.signature);
        }
    }
    out.push_str("Imports:\n");
    for i in &p.imports {
        let from = &p.modules[i.module as usize].module.name;
        let target = match &i.target {
            ImportTarget::Proc { module, proc } => format!(
                "{} in {}",
                p.modules[*module as usize].module.ir.procs[*proc as usize].name,
                p.modules[*module as usize].module.name
            ),
            ImportTarget::Data { offset } => format!("static area offset {offset}"),
            ImportTarget::SrvPgm { srvpgm, export } => {
                format!("{} in *SRVPGM {}", export, p.srvpgms[*srvpgm as usize].name)
            }
            ImportTarget::System => format!(
                "*SYSTEM ({})",
                crate::binder::sysproc::system_srvpgm(&i.name).unwrap_or("QLEAWI")
            ),
            ImportTarget::Unresolved => "*UNRESOLVED".into(),
        };
        let _ = writeln!(
            out,
            "  {:<32} {:<10} from {:<10} -> {target}",
            i.name,
            i.kind.name(),
            from
        );
    }
}

/// `DSPPGM`.
#[must_use]
pub fn dsppgm(p: &ProgramObject) -> String {
    let mut out = String::new();
    program_common(&mut out, p);
    out
}

/// `DSPSRVPGM`: the program display plus exports and signatures.
#[must_use]
pub fn dspsrvpgm(p: &ProgramObject) -> String {
    let mut out = String::new();
    program_common(&mut out, p);
    if p.attr.kind == PgmKind::SrvPgm {
        line(&mut out, "Level check", yes_no(p.attr.lvlchk));
        out.push_str("Exports:\n");
        for e in &p.exports {
            let _ = writeln!(
                out,
                "  {:<32} {:<10} {}",
                e.name,
                e.kind.name(),
                p.modules[e.module as usize].module.name
            );
        }
        out.push_str("Signatures:\n");
        for (i, s) in p.signatures.iter().enumerate() {
            let _ = writeln!(
                out,
                "  {} {s}",
                if i == 0 { "*CURRENT" } else { "*PRV    " }
            );
        }
        if let Some(src) = &p.binder_source {
            out.push_str("Binder source:\n");
            for l in src.lines() {
                let _ = writeln!(out, "  {l}");
            }
        }
    }
    out
}

/// `DSPPGMREF`: the files every bound module refers to.
#[must_use]
pub fn dsppgmref(p: &ProgramObject) -> String {
    let mut out = String::new();
    line(&mut out, "Program", &p.name);
    let mut any = false;
    for m in &p.modules {
        for f in &m.module.ir.files {
            any = true;
            let fmts: Vec<String> = f
                .formats
                .iter()
                .map(|r| match &r.level {
                    Some(l) => format!("{} {l}", r.name),
                    None => r.name.clone(),
                })
                .collect();
            let _ = writeln!(
                out,
                "  {:<10} {:<10} {:<10} {:<8} {:<8} {}",
                m.module.name,
                f.file,
                f.lib,
                "*FILE",
                f.usage.name(),
                fmts.join(", ")
            );
        }
    }
    if !any {
        out.push_str("  No objects referenced.\n");
    }
    out
}

/// `DSPBNDDIR`.
#[must_use]
pub fn dspbnddir(name: &str, d: &BndDir) -> String {
    let mut out = String::new();
    line(&mut out, "Binding directory", name);
    line(&mut out, "Entries", d.entries.len());
    for (i, e) in d.entries.iter().enumerate() {
        let _ = writeln!(
            out,
            "  {:>4} {:<10} {:<10} {:<8} {}",
            i + 1,
            e.name,
            e.lib,
            e.kind.type_name(),
            if e.defer { "*DEFER" } else { "*IMMED" }
        );
    }
    out
}
