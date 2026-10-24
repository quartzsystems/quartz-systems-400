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
    /// `OPTION(*SRCSTMT)`: statements are numbered by their sequence
    /// numbers.
    #[must_use]
    pub fn srcstmt(&self) -> bool {
        self.ctl.option.iter().any(|o| o.eq_ignore_ascii_case("*SRCSTMT"))
    }

    /// The sequence number of a source line, as a statement number.
    #[must_use]
    pub fn stmt_seq(&self, line: u32) -> String {
        self.read.lines.iter().find(|l| l.no == line).map(|l| l.seq.clone()).unwrap_or_default()
    }

    /// The listing's number of a source line: generated lines (`=D`,
    /// `=I`, `=O`) are numbered too, lines conditional compilation
    /// leaves out are not.
    #[must_use]
    pub fn listing_line(&self, line: u32) -> u32 {
        let skipped = self
            .read
            .lines
            .iter()
            .filter(|l| l.no < line && (l.class == rpgfront::reader::Class::Skipped || crate::listing::unnumbered(l)))
            .count();
        line + self.generated.iter().filter(|(at, _)| *at <= line).map(|(_, n)| n).sum::<u32>()
            - u32::try_from(skipped).unwrap_or(0)
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
        if let Some(n) = f.kws.iter().find(|k| k.name == "INFDS").and_then(|k| decl::word_arg(k, 0)) {
            t.infds.push(n.to_ascii_uppercase());
        }
        let mut fi = files::file(f, scope, ext, diags);
        fi.ir_index = u32::try_from(files.len()).unwrap_or(0);
        // Every format ignored: the file is dropped.
        if fi.ext.as_ref().is_some_and(|e| !e.formats.is_empty() && e.formats.iter().all(|r| !crate::extfiles::kept(&fi, &r.name))) {
            diags.push(Diag::new(crate::diag::ids::ALL_FORMATS_IGNORED, 40, f.at).arg(fi.name.clone()));
            t.dropped_files.push(fi.name.clone());
            continue;
        }
        // LIKEFILE: the parent's device, description and use; the new
        // file is qualified.
        if let Some(parent) = fi.likefile.clone() {
            match files.iter().find(|p| p.name == parent.to_ascii_uppercase()).cloned() {
                Some(p) => {
                    fi.dev = p.dev;
                    fi.ext = p.ext;
                    fi.external = p.external;
                    fi.reclen = p.reclen;
                    fi.keyed = p.keyed;
                    fi.keylen = p.keylen;
                    fi.input = p.input;
                    fi.output = p.output;
                    fi.update = p.update;
                    fi.delete = p.delete;
                    fi.qualified = true;
                }
                None => diags.push(Diag::new(crate::diag::ids::NOT_DEFINED, 30, f.at).arg(parent)),
            }
        }
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
    // A basing pointer nothing declares is declared by BASED.
    for d in defs {
        let Some(p) = d.kw("BASED").and_then(|k| decl::word_arg(k, 0)).map(|p| p.to_ascii_uppercase()) else { continue };
        let Some(id) = cx.t.find(scope, &d.name) else { continue };
        if p.contains('.') || p.starts_with('*') || cx.t.find(scope, &p).is_some() || matches!(cx.t.syms[id].storage, Storage::Based(_)) {
            continue;
        }
        let ptr = Sym {
            name: p.clone(),
            kind: Some(Kind::Field),
            ty: crate::types::Ty::Ptr,
            ir: format!("${prefix}{}", mangle(&p)),
            scope,
            storage: if scope == 0 { Storage::Static } else { Storage::Auto },
            implicit: true,
            ..Sym::default()
        };
        if let Ok(ptr) = cx.t.add(ptr) {
            cx.t.syms[id].storage = Storage::Based(ptr);
        }
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

/// A name as the source line writes it (names are upper case once
/// read).
fn written_name(read: &rpgfront::reader::Read, line: u32, name: &str) -> String {
    read.lines
        .iter()
        .find(|l| l.no == line)
        .and_then(|l| {
            let up = l.text.to_ascii_uppercase();
            let at = up.find(&name.to_ascii_uppercase())?;
            l.text.get(at..at + name.len()).map(str::to_string)
        })
        .unwrap_or_else(|| name.to_string())
}

/// Compile a member.
pub fn compile(main: Member, includes: &mut dyn Includes, ext: &mut dyn ExtFiles, opts: &Options) -> Compiled {
    let ropts = ReadOptions {
        defines: predefined(opts).into_iter().chain(opts.define.iter().cloned()).collect(),
        ..ReadOptions::default()
    };
    let (read, mut module, mut diags) = rpgfront::front(main, includes, &ropts);
    // A free-form declaration after the main calculations is out of
    // sequence: ignored, and so is the rest of its line.
    if let Some(first) = module.calcs.first().map(|s| s.at.line) {
        let late: Vec<rpgfront::ast::Pos> =
            module.defs.iter().filter(|d| d.fixed.is_none() && d.at.line > first).map(|d| d.at).collect();
        module.defs.retain(|d| d.fixed.is_some() || d.at.line <= first);
        let mut lines: Vec<u32> = Vec::new();
        for at in late {
            if !lines.contains(&at.line) {
                lines.push(at.line);
                diags.retain(|d| d.at.line != at.line);
                diags.push(Diag::new(crate::diag::ids::OUT_OF_SEQUENCE, 30, rpgfront::ast::Pos { line: at.line, col: 1 }));
            }
        }
    }
    let ctl = ctl::resolve(opts, &module.ctl, &mut diags);
    // A linear module has no main-procedure calculations, and its MAIN
    // procedure is one of its own.
    if ctl.nomain || ctl.main.is_some() {
        for s in &module.calcs {
            diags.push(Diag::new(crate::diag::ids::NO_MAIN_CALCS, 30, s.at));
        }
    }
    if let Some(m) = &ctl.main
        && !module.procs.iter().any(|p| p.name.eq_ignore_ascii_case(m))
    {
        let at = module.ctl.iter().find(|k| k.name == "MAIN").map_or_else(rpgfront::ast::Pos::default, |k| k.at);
        diags.push(Diag::new(crate::diag::ids::NOT_DEFINED, 30, at).arg(m.clone()));
    }
    if !ctl.nomain && ctl.main.is_none() && !can_end(&module) {
        diags.push(Diag::new(crate::diag::ids::NO_END, 40, rpgfront::ast::Pos::default()));
    }
    check_levels(&module, &mut diags);
    // An EXCEPT name no EXCEPT operation names.
    for o in module.outputs.iter().filter(|o| o.kind == "E" && !o.except.is_empty()) {
        let used = module
            .calcs
            .iter()
            .chain(module.procs.iter().flat_map(|p| p.calcs.iter()))
            .any(|s| s.op.code == "EXCEPT" && s.op.f2.first().and_then(|e| e.name()).is_some_and(|n| n.eq_ignore_ascii_case(&o.except)));
        if !used {
            diags.push(Diag::new(crate::diag::ids::EXCEPT_UNUSED, 0, o.at).arg(o.except.clone()));
        }
    }
    let mut t = Table::new();
    t.extbinint = ctl.extbinint;
    t.alwnull_usrctl = ctl.alwnull == "*USRCTL";
    let mut files = Vec::new();
    declare_scope(&mut t, &mut diags, &ctl, 0, "", &module.files, &module.defs, &module.inputs, ext, &mut files);
    crate::extfiles::declare_calc_fields(&mut t, &mut diags, 0, "", &module.calcs);
    bind_entry(&mut t, &mut diags, &module);
    check_main_protos(&module, &ctl, opts, &mut diags);
    let sql_opts = opts.sql.clone().or_else(|| (opts.command == Command::CrtSqlRpgi).then(rpgsqlpre::Options::default));
    if sql_opts.is_some() {
        crate::sql::declare_sqlca(&mut t, &mut diags, &ctl);
        if read.sqlda {
            crate::sql::declare_sqlda(&mut t, &mut diags, &ctl);
        }
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
        if module.procs[..i].iter().any(|q| q.name.eq_ignore_ascii_case(&pname)) {
            diags.push(Diag::new(crate::diag::ids::PROC_TWICE, 30, p.at).arg(pname.clone()));
        }
        check_proc(p, &ctl, &mut diags);
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
            let mut iface = decl::check_keywords(&mut cx, &iface, None);
            for c in &mut iface.children {
                *c = decl::check_keywords(&mut cx, c, None);
            }
            decl::nopass_order(&mut cx, &iface);
            let mut pr = decl::proto(&mut cx, &iface);
            pr.name.clone_from(&pname);
            // EXTPROC(*DCLCASE) on an unnamed interface: the procedure's
            // name as written.
            if pr.extproc.as_deref().is_some_and(|e| e.eq_ignore_ascii_case("*N")) {
                pr.extproc = Some(written_name(&read, p.at.line, &p.name));
            }
            pr
        };
        // A procedure that returns a value needs a RETURN with one.
        if proto.ret.is_some() && !p.calcs.iter().any(|s| s.op.code == "RETURN" && s.op.xf2.is_some()) {
            diags.push(Diag::new(crate::diag::ids::NO_RETURN, 30, p.at).arg(pname.clone()));
        }
        for (k, prm) in proto.parms.iter().enumerate() {
            // A file parameter: a file of the procedure like its parent,
            // its control block the caller's.
            if let Some(parent) = &prm.likefile {
                let at = iface.children.get(k).map_or(iface.at, |c| c.at);
                match files.iter().find(|f| f.name == *parent && !f.param).cloned() {
                    Some(mut f) => {
                        f.name.clone_from(&prm.name);
                        f.scope = scope;
                        f.param = true;
                        f.qualified = true;
                        f.infds = None;
                        f.likefile = Some(parent.clone());
                        f.def_line = at.line;
                        f.ir_index = u32::try_from(files.len()).unwrap_or(0);
                        let s = Sym {
                            name: prm.name.clone(),
                            kind: Some(Kind::File(files.len())),
                            ir: format!("$fcb_{}", mangle(&prm.name)),
                            scope,
                            def_line: at.line,
                            ..Sym::default()
                        };
                        if t.add(s).is_err() {
                            diags.push(Diag::new(crate::diag::ids::DUPLICATE, 30, at).arg(prm.name.clone()));
                        }
                        // Its own record formats, the parent's, qualified.
                        let copies: Vec<_> = t
                            .recfmts
                            .iter()
                            .filter(|r| r.file_name == *parent)
                            .map(|r| crate::extfiles::RecFmt {
                                file: files.len(),
                                file_name: prm.name.clone(),
                                qualified: true,
                                ..r.clone()
                            })
                            .collect();
                        t.recfmts.extend(copies);
                        files.push(f);
                    }
                    None => diags.push(Diag::new(crate::diag::ids::NOT_DEFINED, 30, at).arg(parent.clone())),
                }
                continue;
            }
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
                let at = iface.children.get(k).map_or(iface.at, |c| c.at);
                diags.push(Diag::new(crate::diag::ids::DUPLICATE, 30, at).arg(prm.name.clone()));
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
        crate::extfiles::declare_calc_fields(&mut t, &mut diags, scope, &prefix, &p.calcs);
    }
    diags.extend(crate::blocks::check(&module.calcs));
    for p in &module.procs {
        diags.extend(crate::blocks::check(&p.calcs));
    }
    crate::blocks::check_module(&module, &files, &ctl, opts.command == Command::CrtBndRpg, &mut diags);
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
    let endmod = precompiled.as_ref().is_some_and(|p| p.options.closqlcsr == "*ENDMOD");
    let (qir, mut gen_diags) = codegen::module(&module, &mut t, &files, &ctl, opts, &scopes, &sql_stmts, endmod);
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
    // (*NOEXPDDS numbers the generated lines too, though it hides them.)
    let mut generated = crate::extfiles::generated_lines(&module, &files, &t);
    if let Some(p) = &precompiled {
        let mut firsts = crate::sql::first_calcs(&module, &scopes);
        // An INCLUDE before the first calculation takes the main
        // source's declarations.
        if let (Some(inc), Some(main)) = (read.sql_includes.first(), firsts.first_mut())
            && (main.1 == 0 || inc.line < main.1)
        {
            main.1 = inc.line;
        }
        let first = |scope: usize| firsts.iter().find(|(s, _)| *s == scope).map_or(0, |(_, l)| *l);
        generated.extend(rpgsqlpre::lines::generated(p, &first, module.fully_free));
        // A member that is not fully free, included in one that is, is
        // set off with `**END-FREE` and `**FREE`.
        for inc in read.sql_includes.iter().filter(|i| module.fully_free && !i.fully_free) {
            let Some(m) = inc.member else { continue };
            let last = read.lines.iter().filter(|l| l.member == m).map(|l| l.no).max().unwrap_or(inc.line);
            generated.push((inc.line + 1, 1));
            generated.push((last + 1, 1));
        }
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

/// A procedure's own keywords, interface name and subroutines.
fn check_proc(p: &rpgfront::ast::Proc, ctl: &Ctl, diags: &mut Vec<Diag>) {
    use crate::diag::ids;
    for (i, k) in p.kws.iter().enumerate() {
        if p.kws[..i].iter().any(|x| x.name == k.name) {
            diags.push(Diag::new(ids::PROC_KEYWORD_TWICE, 20, k.at).arg(k.name.clone()));
        } else if k.name == "SERIALIZE" && !ctl.thread.eq_ignore_ascii_case("*CONCURRENT") {
            diags.push(Diag::new(ids::SERIALIZE_NOT_CONCURRENT, 20, k.at));
        }
    }
    // The interface is named *N or after its procedure.
    if let Some(pi) = &p.pi
        && !pi.name.is_empty()
        && !pi.name.eq_ignore_ascii_case("*N")
        && !pi.name.eq_ignore_ascii_case(&p.name)
    {
        diags.push(Diag::new(ids::PI_NAME_DIFFERS, 20, pi.at).arg(pi.name.to_ascii_uppercase()));
    }
    // *INZSR belongs to the main procedure.
    for s in p.calcs.iter().filter(|s| s.op.code == "BEGSR") {
        if matches!(s.op.f1.first(), Some(rpgfront::ast::Expr::Special { word, .. }) if word == "*INZSR") {
            diags.push(Diag::new(ids::INZSR_IN_PROC, 30, s.at));
        }
    }
}

/// A control level L1-L9 that no input field or resulting indicator
/// sets is not defined (RNF7030 at its first use).
fn check_levels(module: &Module, diags: &mut Vec<Diag>) {
    let mut seen: Vec<&str> = Vec::new();
    for s in &module.calcs {
        let l = s.level.as_str();
        let numbered = l.len() == 2 && l.starts_with('L') && matches!(l.as_bytes()[1], b'1'..=b'9');
        if !numbered || seen.contains(&l) {
            continue;
        }
        seen.push(l);
        let by_input = module.inputs.iter().flat_map(|i| &i.fields).any(|f| f.level.eq_ignore_ascii_case(l));
        let by_calc = module.calcs.iter().any(|c| c.op.resind.iter().any(|r| r == l));
        if !by_input && !by_calc {
            diags.push(Diag::new(crate::diag::ids::NOT_DEFINED, 30, s.at).arg(format!("*IN{l}")));
        }
    }
}

/// The main source's prototypes against the command: in the default
/// activation group only programs are called (RNF3788), and a main
/// interface's program is the one being created (RNF3776).
fn check_main_protos(module: &Module, ctl: &Ctl, opts: &Options, diags: &mut Vec<Diag>) {
    use crate::diag::ids;
    for d in module.defs.iter().filter(|d| d.kind == DefKind::Pr) {
        if ctl.dftactgrp && d.kw("EXTPGM").is_none() {
            diags.push(Diag::new(ids::NEEDS_NO_DFTACTGRP, 30, d.at).arg(d.name.to_ascii_uppercase()));
        }
    }
    if opts.command != Command::CrtBndRpg {
        return;
    }
    let Some(pi) = module.defs.iter().find(|d| d.kind == DefKind::Pi && !d.name.is_empty() && !d.name.eq_ignore_ascii_case("*N")) else {
        return;
    };
    let ext = module
        .defs
        .iter()
        .find(|d| d.kind == DefKind::Pr && d.name.eq_ignore_ascii_case(&pi.name))
        .and_then(|d| d.kw("EXTPGM"))
        .and_then(|k| decl::str_arg(k, 0).or_else(|| decl::word_arg(k, 0)))
        .unwrap_or_else(|| pi.name.clone());
    if !ext.eq_ignore_ascii_case(&opts.name) {
        diags.push(Diag::new(ids::MAIN_PGM_DIFFERS, 10, pi.at).arg(ext.to_ascii_uppercase()));
    }
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
