// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compiler's driver: a source member in, the IR, the listing and
//! the diagnostics out. The IR is verified before it is handed back;
//! a module the verifier refuses is a compiler error, never an object.

use rpgfront::ast::{Def, DefKind, Module};
use rpgfront::reader::{Options as ReadOptions, Read};
use rpgfront::{Diag, Includes, Member};

use crate::ctl::{self, Command, Ctl, Options};
use crate::decl::{self, Cx};
use crate::files::{self, ExtFiles, File};
use crate::codegen;
use crate::listing;
use crate::names::mangle;
use crate::syms::{Kind, Proto, Storage, Sym, Table};

/// What a compile produced.
#[derive(Clone, Debug, Default)]
pub struct Compiled {
    /// The IR text; empty when nothing is created.
    pub qir: String,
    /// The IR as generated, kept even when it is refused.
    pub ir: String,
    /// The listing, one record per line with its forms control first.
    pub listing: Vec<String>,
    /// Every diagnostic, sorted.
    pub diags: Vec<Diag>,
    /// Whether the object is created.
    pub created: bool,
    /// The highest severity.
    pub max_severity: u8,
    /// The expanded source.
    pub read: Read,
    /// The syntax tree.
    pub module: Module,
    /// The symbols.
    pub table: Table,
    /// The files.
    pub files: Vec<File>,
    /// The options in force.
    pub ctl: Ctl,
    /// Lines the listing generates: before each source line, how many.
    pub generated: Vec<(u32, u32)>,
}

impl Compiled {
    /// The listing's number of a source line: generated lines (`=D`,
    /// `=I`, `=O`) are numbered too.
    #[must_use]
    pub fn listing_line(&self, line: u32) -> u32 {
        line + self.generated.iter().filter(|(at, _)| *at <= line).map(|(_, n)| n).sum::<u32>()
    }
}

/// The condition names every compile defines: the command and the
/// releases up to the one QS/400 claims (`docs/RPG.md`).
#[must_use]
pub fn predefined(o: &Options) -> Vec<String> {
    let mut v = vec!["*ILERPG".to_string()];
    match o.command {
        Command::CrtBndRpg => v.push("*CRTBNDRPG".into()),
        Command::CrtRpgMod => v.push("*CRTRPGMOD".into()),
        Command::CrtSqlRpgi => {}
    }
    for r in crate::RELEASES {
        v.push(format!("*{r}"));
    }
    v
}

/// Declare one scope's files and definitions.
fn declare_scope(
    t: &mut Table,
    diags: &mut Vec<Diag>,
    ctl: &Ctl,
    scope: usize,
    prefix: &str,
    files_in: &[rpgfront::ast::FileDecl],
    defs: &[Def],
    inputs: &[rpgfront::ast::Input],
    ext: &mut dyn ExtFiles,
    files: &mut Vec<File>,
) {
    let first = files.len();
    for f in files_in {
        let mut fi = files::file(f, scope, ext, diags);
        fi.ir_index = u32::try_from(files.len()).unwrap_or(0);
        let s = Sym {
            name: fi.name.clone(),
            kind: Some(Kind::File(files.len())),
            scope,
            def_line: fi.def_line,
            ..Sym::default()
        };
        if t.add(s).is_err() {
            diags.push(Diag::new(crate::diag::ids::DUPLICATE, 30, f.at).arg(fi.name.clone()));
        }
        files.push(fi);
    }
    crate::extfiles::declare_files(t, diags, scope, prefix, files, first, inputs);
    crate::extfiles::read_extnames(t, defs, ext);
    let mut cx = Cx {
        t,
        diags,
        scope,
        prefix: prefix.to_string(),
        datfmt: ctl.datfmt,
        timfmt: ctl.timfmt,
    };
    for d in defs {
        decl::declare(&mut cx, d);
    }
}

/// Compile a member.
pub fn compile(main: Member, includes: &mut dyn Includes, ext: &mut dyn ExtFiles, opts: &Options) -> Compiled {
    let ropts = ReadOptions {
        defines: predefined(opts).into_iter().chain(opts.define.iter().cloned()).collect(),
        ..ReadOptions::default()
    };
    let (read, module, mut diags) = rpgfront::front(main, includes, &ropts);
    let ctl = ctl::resolve(opts, &module.ctl, &mut diags);
    let mut t = Table::new();
    let mut files = Vec::new();
    declare_scope(&mut t, &mut diags, &ctl, 0, "", &module.files, &module.defs, &module.inputs, ext, &mut files);
    crate::extfiles::declare_calc_fields(&mut t, 0, "", &module.calcs);
    // A program-described printer without an overflow indicator gets
    // the first of OA to OG no other file uses.
    let used: Vec<String> = files.iter().filter_map(|f| f.oflind.clone()).collect();
    for f in files.iter_mut().filter(|f| f.dev == crate::files::Dev::Printer && !f.external && f.oflind.is_none()) {
        let pick = ["*INOA", "*INOB", "*INOC", "*INOD", "*INOE", "*INOF", "*INOG"]
            .into_iter()
            .find(|i| !used.iter().any(|u| u == i));
        if let Some(p) = pick {
            f.oflind = Some(p.to_string());
            f.auto_overflow = true;
            diags.push(Diag::new(crate::diag::ids::OFLIND_ASSIGNED, 0, rpgfront::ast::Pos { line: f.def_line, col: 7 }).arg(p));
        }
    }
    // Each procedure: its interface as parameters, then its own files
    // and definitions; a procedure with no prototype gets one from its
    // interface.
    let mut scopes = Vec::new();
    for (i, p) in module.procs.iter().enumerate() {
        let scope = t.new_scope();
        scopes.push(scope);
        let prefix = format!("L{}_", i + 1);
        let pname = p.name.to_ascii_uppercase();
        let iface = p.pi.clone().unwrap_or(Def {
            kind: DefKind::Pi,
            name: pname.clone(),
            at: p.at,
            ..Def::default()
        });
        let proto = {
            let mut cx = Cx {
                t: &mut t,
                diags: &mut diags,
                scope,
                prefix: prefix.clone(),
                datfmt: ctl.datfmt,
                timfmt: ctl.timfmt,
            };
            let mut pr = decl::proto(&mut cx, &iface);
            pr.name.clone_from(&pname);
            pr
        };
        for (k, prm) in proto.parms.iter().enumerate() {
            let s = Sym {
                name: prm.name.clone(),
                kind: Some(Kind::Param),
                ty: prm.ty.clone(),
                dim: prm.dim,
                ir: format!("$P{}_{}", i + 1, mangle(&prm.name)),
                scope,
                storage: Storage::Param,
                readonly: prm.konst,
                def_line: iface.children.get(k).map_or(iface.at.line, |c| c.at.line),
                ..Sym::default()
            };
            if t.add(s).is_err() {
                diags.push(Diag::new(crate::diag::ids::DUPLICATE, 30, iface.at).arg(prm.name.clone()));
            }
        }
        // The prototype by the procedure's name.
        match t.find_in(0, &pname) {
            Some(id) => {
                if let Some(Kind::Proto(pi)) = t.syms[id].kind.clone() {
                    let declared = t.protos[pi].clone();
                    t.protos[pi] = merge_proto(declared, &proto);
                }
            }
            None => {
                let idx = t.protos.len();
                t.protos.push(Proto {
                    def_line: iface.at.line,
                    ..proto
                });
                let _ = t.add(Sym {
                    name: pname.clone(),
                    kind: Some(Kind::Proto(idx)),
                    scope: 0,
                    def_line: p.at.line,
                    ..Sym::default()
                });
            }
        }
        declare_scope(&mut t, &mut diags, &ctl, scope, &prefix, &p.files, &p.defs, &[], ext, &mut files);
        crate::extfiles::declare_calc_fields(&mut t, scope, &prefix, &p.calcs);
    }
    let (qir, gen_diags) = codegen::module(&module, &mut t, &files, &ctl, opts, &scopes);
    diags.extend(gen_diags);
    diags.sort();
    diags.dedup();
    let max_severity = diags.iter().map(|d| d.severity).max().unwrap_or(0);
    let mut created = max_severity <= ctl.genlvl && !ctl.option.iter().any(|o| o == "*NOGEN");
    let mut qir = qir;
    if created {
        // The IR must verify; a refusal is the compiler's own error.
        match qir::parse(&qir) {
            Ok(m) => {
                if let Err(e) = qir::verify(&m) {
                    diags.push(Diag::new("RNF0001", 50, rpgfront::ast::Pos::default()).arg(e.to_string()));
                    created = false;
                }
            }
            Err(e) => {
                diags.push(Diag::new("RNF0001", 50, rpgfront::ast::Pos::default()).arg(e.to_string()));
                created = false;
            }
        }
    }
    let ir = qir.clone();
    if !created {
        qir.clear();
    }
    let max_severity = diags.iter().map(|d| d.severity).max().unwrap_or(0);
    let generated = crate::extfiles::generated_lines(&module, &files, &t);
    let mut c = Compiled {
        generated,
        qir,
        ir,
        diags,
        created,
        max_severity,
        read,
        module,
        table: t,
        files,
        ctl,
        ..Compiled::default()
    };
    c.listing = listing::listing(&c, opts);
    c
}

/// A declared prototype checked against the interface: the
/// interface's names, the prototype's options.
fn merge_proto(mut declared: Proto, iface: &Proto) -> Proto {
    for (d, i) in declared.parms.iter_mut().zip(&iface.parms) {
        if d.name.is_empty() {
            d.name.clone_from(&i.name);
        }
    }
    if declared.ret.is_none() {
        declared.ret.clone_from(&iface.ret);
    }
    declared
}
