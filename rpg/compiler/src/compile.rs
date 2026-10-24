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
    /// The SQL precompile, for `CRTSQLRPGI`.
    pub precompiled: Option<rpgsqlpre::Precompiled>,
    /// The precompile failed: nothing was compiled.
    pub sql_failed: bool,
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
        // A second file of the same name is ignored.
        if t.add(s).is_err() {
            diags.push(Diag::new(crate::diag::ids::FILE_TWICE, 30, f.at).arg(fi.name.clone()));
            continue;
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
        datsep: ctl.datsep.unwrap_or(ctl.datfmt.sep()),
        timsep: ctl.timsep.unwrap_or(ctl.timfmt.sep()),
    };
    for d in in_dependency_order(defs) {
        decl::declare(&mut cx, d);
    }
}

/// The definitions in source order, except that one a keyword names
/// (`LIKEDS`, `LIKE`, `DIM(constant)`..., its own or a subfield's) comes
/// before the definition naming it, as IBM allows the later declaration.
fn in_dependency_order(defs: &[Def]) -> Vec<&Def> {
    let names: Vec<String> = defs.iter().map(|d| d.name.to_ascii_uppercase()).collect();
    let needs = |d: &Def| -> Vec<usize> {
        let mut out = Vec::new();
        for x in std::iter::once(d).chain(d.children.iter()) {
            for k in &x.kws {
                for a in 0..k.args.len() {
                    if let Some(w) = decl::word_arg(k, a) {
                        let w = w.to_ascii_uppercase();
                        if let Some(i) = names.iter().position(|n| *n == w) {
                            out.push(i);
                        }
                    }
                }
            }
        }
        out
    };
    let deps: Vec<Vec<usize>> = defs.iter().map(needs).collect();
    let mut done = vec![false; defs.len()];
    let mut out = Vec::with_capacity(defs.len());
    let mut on = vec![false; defs.len()];
    for i in 0..defs.len() {
        visit(i, defs, &deps, &mut done, &mut on, &mut out);
    }
    out
}

/// Depth first from a definition: what it needs, then it. A cycle is
/// left to the declarations' own errors.
fn visit<'a>(i: usize, defs: &'a [Def], deps: &[Vec<usize>], done: &mut [bool], on: &mut [bool], out: &mut Vec<&'a Def>) {
    if done[i] || on[i] {
        return;
    }
    on[i] = true;
    for &j in &deps[i] {
        if j != i {
            visit(j, defs, deps, done, on, out);
        }
    }
    on[i] = false;
    done[i] = true;
    out.push(&defs[i]);
}

/// Compile a member.
pub fn compile(main: Member, includes: &mut dyn Includes, ext: &mut dyn ExtFiles, opts: &Options) -> Compiled {
    let ropts = ReadOptions {
        defines: predefined(opts).into_iter().chain(opts.define.iter().cloned()).collect(),
        ..ReadOptions::default()
    };
    let (read, module, mut diags) = rpgfront::front(main, includes, &ropts);
    let ctl = ctl::resolve(opts, &module.ctl, &mut diags);
    if !ctl.nomain && ctl.main.is_none() && !can_end(&module) {
        diags.push(Diag::new(crate::diag::ids::NO_END, 40, rpgfront::ast::Pos::default()));
    }
    let mut t = Table::new();
    let mut files = Vec::new();
    declare_scope(&mut t, &mut diags, &ctl, 0, "", &module.files, &module.defs, &module.inputs, ext, &mut files);
    crate::extfiles::declare_calc_fields(&mut t, 0, "", &module.calcs);
    bind_entry(&mut t, &mut diags, &module);
    let sql_opts = opts.sql.clone().or_else(|| (opts.command == Command::CrtSqlRpgi).then(rpgsqlpre::Options::default));
    if sql_opts.is_some() {
        crate::sql::declare_sqlca(&mut t, &mut diags, &ctl);
    }
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
                datsep: ctl.datsep.unwrap_or(ctl.datfmt.sep()),
                timsep: ctl.timsep.unwrap_or(ctl.timfmt.sep()),
            };
            decl::check_keywords(&mut cx, &iface);
            for c in &iface.children {
                decl::check_keywords(&mut cx, c);
            }
            let mut pr = decl::proto(&mut cx, &iface);
            pr.name.clone_from(&pname);
            pr
        };
        // A procedure that returns a value needs a RETURN with one.
        if proto.ret.is_some() && !p.calcs.iter().any(|s| s.op.code == "RETURN" && s.op.xf2.is_some()) {
            diags.push(Diag::new(crate::diag::ids::NO_RETURN, 30, p.at).arg(pname.clone()));
        }
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
                    // Each interface parameter is its prototype's.
                    for (k, (d, i)) in declared.parms.iter().zip(&proto.parms).enumerate() {
                        let known = d.ty != crate::types::Ty::Unknown && i.ty != crate::types::Ty::Unknown;
                        if known && (d.ty != i.ty || d.dim != i.dim) {
                            let at = iface.children.get(k).map_or(iface.at, |c| c.at);
                            diags.push(Diag::new(crate::diag::ids::PI_PARM_DIFFERS, 30, at).arg(i.name.clone()));
                        }
                    }
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
    diags.extend(crate::blocks::check(&module.calcs));
    for p in &module.procs {
        diags.extend(crate::blocks::check(&p.calcs));
    }
    // CRTSQLRPGI: the statements precompiled; errors above the
    // generation level end the command before the compile.
    let precompiled = sql_opts.map(|o| crate::sql::precompile(&module, &t, &scopes, ext, &o));
    if let Some(p) = &precompiled
        && !p.ok
    {
        let mut c = Compiled {
            diags: crate::sql::diags(p),
            max_severity: p.max_severity,
            read,
            module,
            ctl,
            sql_failed: true,
            ..Compiled::default()
        };
        c.listing = crate::sql::precompile_listing(&c, p, opts);
        c.precompiled = precompiled;
        return c;
    }
    let sql_stmts: Vec<rpgsqlpre::Stmt> = precompiled.as_ref().map(|p| p.stmts.clone()).unwrap_or_default();
    if let Some(p) = &precompiled {
        diags.extend(crate::sql::diags(p));
    }
    let (qir, mut gen_diags) = codegen::module(&module, &mut t, &files, &ctl, opts, &scopes, &sql_stmts);
    // A group the generator finds unbalanced follows from another
    // error (a group whose start was in error, or one IBM reports).
    let other = diags.iter().chain(&gen_diags).any(|d| d.severity >= 20 && d.msgid != crate::diag::ids::UNBALANCED);
    if other {
        gen_diags.retain(|d| d.msgid != crate::diag::ids::UNBALANCED);
    }
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
    let mut generated = crate::extfiles::generated_lines(&module, &files, &t);
    if let Some(p) = &precompiled {
        let firsts = crate::sql::first_calcs(&module, &scopes);
        let first = |scope: usize| firsts.iter().find(|(s, _)| *s == scope).map_or(0, |(_, l)| *l);
        generated.extend(rpgsqlpre::lines::generated(p, &first, module.fully_free));
    }
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
    if let Some(p) = precompiled {
        let mut l = crate::sql::precompile_listing(&c, &p, opts);
        l.append(&mut c.listing);
        c.listing = l;
        c.precompiled = Some(p);
    }
    c
}

/// Whether a cycle main procedure can end: a primary file, `RETURN`
/// in its calculations, or `LR` (or `*IN`) set anywhere.
fn can_end(m: &Module) -> bool {
    use rpgfront::ast::{Expr, Stmt, Step};
    fn ind(e: &Expr) -> bool {
        match e {
            Expr::Special { word, index, .. } => word == "*INLR" || word == "*IN" || index.as_deref().is_some_and(ind),
            Expr::Ref { steps, .. } => steps.iter().any(|s| matches!(s, Step::Args(a) if a.iter().any(ind))),
            Expr::Bif { args, .. } | Expr::List(args, _) => args.iter().any(ind),
            Expr::Unary(_, a, _) => ind(a),
            Expr::Binary(_, a, b, _) | Expr::Assign(_, a, b, _) => ind(a) || ind(b),
            _ => false,
        }
    }
    fn sets_lr(s: &Stmt) -> bool {
        let o = &s.op;
        s.level == "LR"
            || o.resind.iter().any(|r| r == "LR")
            || o.f1.iter().chain(&o.f2).chain(&o.result).chain(o.xf2.iter()).chain(o.more.iter().map(|(_, e)| e)).any(ind)
    }
    let primary = m.files.iter().any(|f| f.fixed.as_ref().is_some_and(|c| c.designation == "P"));
    primary
        || m.calcs.iter().any(|s| s.op.code == "RETURN" || sets_lr(s))
        || m.procs.iter().flat_map(|p| p.calcs.iter()).any(sets_lr)
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

/// The program's parameter fields become based on pointers the program
/// entry procedure sets to its caller's storage.
fn bind_entry(t: &mut Table, diags: &mut Vec<Diag>, module: &rpgfront::Module) {
    let mut names = t.entry_names.clone();
    names.extend(crate::codegen::calls::entry_names(module));
    for (i, n) in names.iter().enumerate() {
        let Some(id) = t.find(0, n) else {
            diags.push(Diag::new(crate::diag::ids::NOT_DEFINED, 30, rpgfront::ast::Pos::default()).arg(n.clone()));
            continue;
        };
        let ptr = t.add_hidden(crate::syms::Sym {
            name: format!("_EP{}", i + 1),
            kind: Some(crate::syms::Kind::Field),
            ty: crate::types::Ty::Ptr,
            ir: format!("$_ep{}", i + 1),
            storage: crate::syms::Storage::Static,
            ..crate::syms::Sym::default()
        });
        t.syms[id].storage = crate::syms::Storage::Based(ptr);
        t.entry.push(id);
        t.entry_ptrs.push(ptr);
    }
}
