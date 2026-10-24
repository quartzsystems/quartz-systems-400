// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The commands a case's `setup:` and `cmd:` run, done in memory:
//! `CRTRPGMOD` (a member compiled as a module), `CRTSRVPGM` and
//! `CRTPGM` (modules bound, `BNDSRVPGM` and `BNDDIR` honoured),
//! `CRTBNDRPG` (a program another one calls),
//! `CRTBNDDIR` and `ADDBNDDIRE`. What the oracle's library would hold,
//! the kit's objects hold.

use std::collections::HashMap;

use ile::binder::{Resolved, SrvPgmInfo};
use ile::object::bnddir::{BndDir, BndDirEntry, BndEntryKind};
use ile::{ActGrpAttr, BindInput, BindOptions, CreateData, ModuleObject, PgmKind, ProgramObject};

use crate::qir::CLOCK;

/// The objects a case made.
#[derive(Default)]
pub struct Objects {
    /// Modules by name.
    pub modules: HashMap<String, ModuleObject>,
    /// Service programs by name.
    pub srvpgms: HashMap<String, ProgramObject>,
    /// Binding directories by name.
    pub bnddirs: HashMap<String, BndDir>,
    /// Programs by name.
    pub programs: HashMap<String, ProgramObject>,
}

/// The value of `KWD(...)` in a command, upper case, or empty.
#[must_use]
pub fn kwd(cmd: &str, kw: &str) -> String {
    let up = cmd.to_ascii_uppercase();
    let key = format!("{kw}(");
    let Some(at) = up.find(&key).filter(|&i| i == 0 || !up.as_bytes()[i - 1].is_ascii_alphanumeric()) else {
        return String::new();
    };
    let rest = &up[at + key.len()..];
    let mut depth = 1;
    let mut end = rest.len();
    for (i, c) in rest.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    end = i;
                    break;
                }
            }
            _ => {}
        }
    }
    rest[..end].trim().to_string()
}

/// `LIB/NAME` or `NAME`: the name.
fn obj_name(q: &str) -> String {
    q.rsplit('/').next().unwrap_or(q).trim().to_string()
}

/// Names listed in a parameter (`MODULE(L/A L/B)`).
pub fn names_of(list: &str) -> Vec<String> {
    list.split_whitespace().map(obj_name).filter(|n| !n.is_empty()).collect()
}

/// A module's IR into a module object.
pub fn module_object(ir: &str, name: &str) -> Result<ModuleObject, String> {
    let m = qir::parse(ir).map_err(|e| format!("{name}: {e}"))?;
    ModuleObject::compile(m, CreateData::default(), CLOCK).map_err(|e| format!("{name}: {e}"))
}

impl Objects {
    /// A service program as the binder sees it.
    fn srvpgm_info(p: &ProgramObject) -> SrvPgmInfo {
        SrvPgmInfo {
            name: p.name.clone(),
            lib: "QGPL".into(),
            exports: p.exports.iter().map(|x| (x.name.clone(), x.kind)).collect(),
            signature: p.signature().unwrap_or(ile::binder::signature::Signature([0; 16])),
        }
    }

    /// Bind modules into a program or service program, with the
    /// directories named and the service programs given.
    pub fn bind(
        &self,
        name: &str,
        kind: PgmKind,
        modules: Vec<ModuleObject>,
        bnddirs: &[String],
        srvpgms: &[String],
        actgrp: ActGrpAttr,
    ) -> Result<ProgramObject, String> {
        let mut dirs: Vec<BndDir> = Vec::new();
        if !srvpgms.is_empty() {
            let mut d = BndDir::default();
            for s in srvpgms {
                d.add(
                    BndDirEntry {
                        name: s.clone(),
                        lib: "*LIBL".into(),
                        kind: BndEntryKind::SrvPgm,
                        defer: false,
                    },
                    None,
                );
            }
            dirs.push(d);
        }
        // A directory the setup did not make is not found (IBM's own,
        // Q-named, hold nothing the kit binds).
        for b in bnddirs {
            match self.bnddirs.get(b) {
                Some(d) => dirs.push(d.clone()),
                None if b.starts_with('Q') => {}
                None => return Err(format!("binding directory {b} not found")),
            }
        }
        let lookup = |e: &BndDirEntry| -> Option<Resolved> {
            match e.kind {
                BndEntryKind::SrvPgm => self.srvpgms.get(&e.name).map(|p| Resolved::SrvPgm(Self::srvpgm_info(p))),
                BndEntryKind::Module => self.modules.get(&e.name).map(|m| Resolved::Module(m.clone(), "QGPL".into())),
            }
        };
        let mut opts = BindOptions::new(name, kind);
        opts.attr.actgrp = actgrp;
        ile::bind(BindInput {
            modules: modules.into_iter().map(|m| (m, "QGPL".to_string())).collect(),
            bnddirs: dirs,
            lookup: &lookup,
            options: opts,
        })
        .map(|(p, _)| p)
        .map_err(|e| format!("{name}: {e}"))
    }

    /// Run one setup command. `compile` compiles a member as a module:
    /// member name, module name, the command; it gives the IR.
    pub fn setup(&mut self, cmd: &str, compile: &dyn Fn(&str, &str, &str) -> Result<String, String>) -> Result<(), String> {
        let up = cmd.trim().to_ascii_uppercase();
        let verb = up.split_whitespace().next().unwrap_or("").to_string();
        match verb.as_str() {
            "CRTRPGMOD" => {
                let name = obj_name(&kwd(cmd, "MODULE"));
                let mbr = kwd(cmd, "SRCMBR");
                let mbr = if mbr.is_empty() || mbr == "*MODULE" { name.clone() } else { mbr };
                let ir = compile(&mbr, &name, cmd)?;
                self.modules.insert(name.clone(), module_object(&ir, &name)?);
            }
            "CRTSQLRPGI" if kwd(cmd, "OBJTYPE").eq_ignore_ascii_case("*MODULE") => {
                let name = obj_name(&kwd(cmd, "OBJ"));
                let mbr = kwd(cmd, "SRCMBR");
                let mbr = if mbr.is_empty() || mbr == "*OBJ" { name.clone() } else { mbr };
                let ir = compile(&mbr, &name, cmd)?;
                self.modules.insert(name.clone(), module_object(&ir, &name)?);
            }
            "CRTBNDRPG" | "CRTSQLRPGI" => {
                let sql = verb == "CRTSQLRPGI";
                let name = obj_name(&kwd(cmd, if sql { "OBJ" } else { "PGM" }));
                let mbr = kwd(cmd, "SRCMBR");
                let mbr = if mbr.is_empty() || mbr == "*PGM" || mbr == "*OBJ" { name.clone() } else { mbr };
                let ir = compile(&mbr, &name, cmd)?;
                let m = qir::parse(&ir).map_err(|e| format!("{name}: {e}"))?;
                let actgrp = if m.dftactgrp {
                    ActGrpAttr::Default
                } else {
                    match &m.actgrp {
                        qir::ActGrp::New => ActGrpAttr::New,
                        qir::ActGrp::Caller => ActGrpAttr::Caller,
                        qir::ActGrp::Named(n) => ActGrpAttr::Named(n.clone()),
                    }
                };
                let mo = ModuleObject::compile(m, CreateData::default(), CLOCK).map_err(|e| format!("{name}: {e}"))?;
                let p = self.bind(&name, PgmKind::Pgm, vec![mo], &[], &[], actgrp)?;
                self.programs.insert(name, p);
            }
            "CRTSRVPGM" => {
                let name = obj_name(&kwd(cmd, "SRVPGM"));
                let mods: Vec<ModuleObject> = names_of(&kwd(cmd, "MODULE"))
                    .iter()
                    .filter_map(|m| self.modules.get(m).cloned())
                    .collect();
                let p = self.bind(
                    &name,
                    PgmKind::SrvPgm,
                    mods,
                    &names_of(&kwd(cmd, "BNDDIR")),
                    &names_of(&kwd(cmd, "BNDSRVPGM")),
                    ActGrpAttr::Caller,
                )?;
                self.srvpgms.insert(name, p);
            }
            "CRTBNDDIR" => {
                self.bnddirs.insert(obj_name(&kwd(cmd, "BNDDIR")), BndDir::default());
            }
            "ADDBNDDIRE" => {
                let dir = obj_name(&kwd(cmd, "BNDDIR"));
                let obj = kwd(cmd, "OBJ");
                let inner = obj.trim_matches(|c| c == '(' || c == ')');
                let mut w = inner.split_whitespace();
                let n = obj_name(w.next().unwrap_or(""));
                let kind = if w.next().unwrap_or("*SRVPGM") == "*MODULE" { BndEntryKind::Module } else { BndEntryKind::SrvPgm };
                self.bnddirs.entry(dir).or_default().add(
                    BndDirEntry {
                        name: n,
                        lib: "*LIBL".into(),
                        kind,
                        defer: false,
                    },
                    None,
                );
            }
            // Deleting, and anything else, has nothing to do in memory.
            _ => {}
        }
        Ok(())
    }
}
