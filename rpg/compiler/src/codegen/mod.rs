// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Code generation: the checked module as IR text. Global storage is
//! static; a structure is an IR struct type with a hidden whole-structure
//! field `all_` beside its subfields, so the structure as character data
//! and its subfields are both places. A cycle-main module's entry
//! procedure runs the RPG cycle around the detail and total
//! calculations; subprocedures are procedures with automatic storage.
//! Statements check their operands as they are generated, so one walk
//! gives the diagnostics and the code.

pub mod bif;
pub mod cycle;
pub mod dbio;
pub mod emit;
pub mod exc;
pub mod expr;
pub mod output;
pub mod stmt;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;

use rpgfront::Diag;
use rpgfront::ast::{Expr, Module, Pos, Stmt};

use crate::ctl::{Ctl, Options};
use crate::diag::ids;
use crate::files::{Dev, File};
use crate::names::mangle;
use crate::syms::{Kind, Storage, SymId, Table};
use crate::types::Ty;

use emit::Body;

/// A subroutine as generation knows it.
#[derive(Clone, Debug, Default)]
pub struct Subr {
    /// Its entry label.
    pub label: String,
    /// The return-point slot.
    pub slot: String,
    /// Return labels, in call order.
    pub returns: Vec<String>,
    /// The line index of the `ENDSR` switch to fill.
    pub switch_line: Option<usize>,
    /// The label `LEAVESR` and `ENDSR` reach.
    pub end: String,
    /// Defined.
    pub defined: bool,
}

/// An open structured operation.
#[derive(Clone, Debug)]
pub enum Open {
    /// `IF`: the label the next `ELSE`/`ELSEIF` starts at, the end.
    If {
        /// Where the false branch goes.
        next: String,
        /// The end.
        end: String,
    },
    /// A loop: where `ITER` goes, where `LEAVE` goes, and what the end
    /// does.
    Loop {
        /// `ITER` target.
        iter: String,
        /// `LEAVE` target.
        leave: String,
        /// How the loop ends.
        end: LoopEnd,
    },
    /// `SELECT`: the label of the next `WHEN` test, the end.
    Select {
        /// Where the next test goes.
        next: Option<String>,
        /// The end.
        end: String,
    },
    /// `MONITOR`: the region name, the end, the handlers seen.
    Monitor {
        /// Region name.
        name: String,
        /// After the group.
        end: String,
        /// Whether the body has been closed (first `ON-ERROR` seen).
        closed: bool,
        /// The patch index of the `monitor` line.
        line: usize,
        /// Handlers: selectors and labels.
        handlers: Vec<(String, String)>,
    },
    /// `BEGSR`.
    Subr(String),
}

/// How a loop's end works.
#[derive(Clone, Debug)]
pub enum LoopEnd {
    /// Go back to the top (`DOW`, the condition at the top).
    Top(String),
    /// Test at the bottom (`DOU`): the condition and the body label.
    Bottom(Box<Expr>, String),
    /// `FOR`: add the step to the index (or subtract it, `DOWNTO`),
    /// then go to the top.
    For {
        /// The top label.
        top: String,
        /// The index place.
        index: String,
        /// The step's temporary; 1 when there is none.
        step: Option<String>,
        /// `DOWNTO`.
        down: bool,
    },
    /// Fixed-form `DOUxx`: the comparison at the bottom.
    BottomCmp(String, Box<Expr>, Box<Expr>, String),
    /// Fixed-form `DO`: counter, increment, top.
    Counted {
        /// The counter place.
        counter: String,
        /// The top label.
        top: String,
    },
}

/// The generator.
pub struct Gen<'a> {
    /// The symbols.
    pub t: &'a mut Table,
    /// The files.
    pub files: &'a [File],
    /// The control options.
    pub ctl: &'a Ctl,
    /// The command options.
    pub opts: &'a Options,
    /// Diagnostics.
    pub diags: Vec<Diag>,
    /// The procedure being generated.
    pub b: Body,
    /// The scope of names.
    pub scope: usize,
    /// Open structures.
    pub open: Vec<Open>,
    /// Subroutines of the procedure.
    pub subrs: HashMap<String, Subr>,
    /// External declarations (`extern pgm`, `extern proc`).
    pub externs: BTreeMap<String, String>,
    /// `TAG` labels.
    pub tags: HashMap<String, String>,
    /// Where `RETURN` goes in the main procedure.
    pub main_exit: Option<String>,
    /// The procedure's return type.
    pub ret: Option<Ty>,
    /// The statement being generated.
    pub at: Pos,
    /// Statement numbers already marked.
    pub marked: BTreeSet<u32>,
    /// Emit `debug.stmt` and `debug.var`.
    pub debug: bool,
    /// Monitors made.
    pub monitors: u32,
    /// Names of the module's own procedures.
    pub local_procs: BTreeSet<String>,
    /// Page counters the output uses (`PAGE`, `PAGE1`...).
    pub pages: BTreeSet<String>,
    /// The module's output specifications.
    pub outputs: Vec<rpgfront::ast::Output>,
    /// The module's compile-time data sections.
    pub ctdata: Vec<rpgfront::ast::CtData>,
    /// Names reported as not defined.
    pub undefined: BTreeSet<String>,
    /// Statements reported for an undefined operand.
    pub undefined_at: BTreeSet<u32>,
    /// Where the reference being resolved is.
    pub ref_at: Pos,
    /// Main-procedure subroutines are procedures (`EXSR` calls them).
    pub subr_procs: bool,
    /// Generating a subroutine's procedure.
    pub in_subr_proc: bool,
    /// The subroutine procedures made, as IR text.
    pub subr_bodies: Vec<String>,
    /// Subroutines with a procedure.
    pub defined_subrs: BTreeSet<String>,
    /// Subroutines called.
    pub called_subrs: BTreeSet<String>,
    /// Generating an operation inside its `(E)` region.
    pub in_error_extender: bool,
    /// Inside a procedure with `ON-EXIT`: where `RETURN` goes, and the
    /// temporary holding the value.
    pub on_exit: Option<(String, Option<String>)>,
    /// `KLIST` names and their `KFLD` fields.
    pub klists: HashMap<String, Vec<Expr>>,
    /// The module's input specifications.
    pub inputs: Vec<rpgfront::ast::Input>,
    /// Overflow output is being generated (no fetch inside it).
    pub in_overflow: bool,
    /// Record formats an operation names.
    pub used_formats: BTreeSet<usize>,
}

impl Gen<'_> {
    /// A name that is not defined: RNF7030 at its first reference,
    /// RNF7503 for each statement that uses it.
    pub fn undefined_name(&mut self, name: &str, at: Pos) {
        let at = if at.line == 0 { self.at } else { at };
        if self.undefined.insert(name.to_string()) {
            self.diags.push(Diag::new(ids::NOT_DEFINED, 30, at).arg(name.to_string()));
        }
        if self.undefined_at.insert(self.at.line) {
            self.diags.push(Diag::new(ids::UNDEFINED_OPERAND, 30, at));
        }
    }
}

impl Gen<'_> {
    /// Report a diagnostic at the current statement.
    pub fn err(&mut self, id: &'static str, sev: u8, arg: impl Into<String>) {
        self.diags.push(Diag::new(id, sev, self.at).arg(arg));
    }

    /// The IR text of a symbol's type.
    #[must_use]
    pub fn ir_ty(&self, t: &Ty) -> String {
        self.t.ir_type(t)
    }

    /// Mark the start of a statement: the runtime keeps the statement
    /// running for the PSDS, dumps and the debugger.
    pub fn mark(&mut self, at: Pos) {
        self.at = at;
        if self.marked.insert(at.line) {
            self.b.ins(format!("debug.stmt {}", at.line));
        }
    }
}

/// The IR struct type of every structure, in dependency order.
fn struct_types(t: &Table) -> Vec<String> {
    let mut out = Vec::new();
    let mut done: BTreeSet<String> = BTreeSet::new();
    for ds in &t.ds {
        if !done.insert(ds.ir_type.clone()) {
            continue;
        }
        let mut s = format!("type {} = struct({}) {{ all_: char({}) @0", ds.ir_type, ds.len.max(1), ds.len.max(1));
        for &f in &ds.subfields {
            let sym = &t.syms[f];
            if sym.name.is_empty() || sym.name == "*N" || matches!(sym.ty, Ty::Unknown) {
                continue;
            }
            let _ = write!(s, ", {}: {} @{}", mangle(&sym.name), t.ir_storage(sym), sym.offset);
        }
        s.push_str(" }");
        out.push(s);
    }
    out
}

/// The IR place of a symbol (without indexes).
#[must_use]
pub fn place_of(t: &Table, id: SymId) -> String {
    let s = &t.syms[id];
    match (&s.kind, s.parent) {
        (Some(Kind::Subfield), Some(p)) => format!("{}.{}", place_of(t, p), mangle(&s.name)),
        _ => s.ir.clone(),
    }
}

/// The IR of a whole module.
pub fn module(
    m: &Module,
    t: &mut Table,
    files: &[File],
    ctl: &Ctl,
    opts: &Options,
    locals: &[usize],
) -> (String, Vec<Diag>) {
    let mut g = Gen {
        t,
        files,
        ctl,
        opts,
        diags: Vec::new(),
        b: Body::new(),
        scope: 0,
        open: Vec::new(),
        subrs: HashMap::new(),
        externs: BTreeMap::new(),
        tags: HashMap::new(),
        main_exit: None,
        ret: None,
        at: Pos::default(),
        marked: BTreeSet::new(),
        debug: opts.dbgview != "*NONE",
        monitors: 0,
        local_procs: m.procs.iter().map(|p| p.name.to_ascii_uppercase()).collect(),
        pages: BTreeSet::new(),
        outputs: m.outputs.clone(),
        ctdata: m.ctdata.clone(),
        undefined: BTreeSet::new(),
        undefined_at: BTreeSet::new(),
        ref_at: Pos::default(),
        subr_procs: false,
        in_subr_proc: false,
        subr_bodies: Vec::new(),
        defined_subrs: BTreeSet::new(),
        called_subrs: BTreeSet::new(),
        in_error_extender: false,
        on_exit: None,
        klists: klists(m),
        inputs: m.inputs.clone(),
        in_overflow: false,
        used_formats: BTreeSet::new(),
    };
    let mut out = String::new();
    let actgrp = if ctl.dftactgrp {
        "*CALLER".to_string()
    } else {
        match ctl.actgrp.as_str() {
            "*NEW" => "*NEW".into(),
            "*CALLER" => "*CALLER".into(),
            "*STGMDL" | "" => "QILE".into(),
            other => other.to_string(),
        }
    };
    let _ = writeln!(
        out,
        "module {} lang(RPGLE) ccsid(37) actgrp({actgrp}){}{} dbgview({})",
        mangle(&opts.name),
        if ctl.dftactgrp { " dftactgrp" } else { "" },
        if ctl.nomain { " nomain" } else { "" },
        if opts.dbgview == "*NONE" { "*NONE" } else { "*STMT" },
    );
    for ty in struct_types(g.t) {
        out.push_str(&ty);
        out.push('\n');
    }
    for f in files {
        let dev = match f.dev {
            Dev::Printer => " dev(*PRT)",
            Dev::Workstn => " dev(*DSP)",
            _ => "",
        };
        let mut line = format!(
            "file {} file({}) lib({}){dev} usage({})",
            mangle(&f.name),
            mangle(&f.obj.1),
            if f.obj.0 == "*LIBL" { "*LIBL".into() } else { mangle(&f.obj.0) },
            f.ir_usage()
        );
        if f.keyed {
            line.push_str(" keyed");
        }
        if let Some(ext) = &f.ext {
            for fm in &ext.formats {
                let _ = write!(line, " fmt({}:{})", mangle(&fm.name), emit::quote(&fm.level_id));
            }
        }
        out.push_str(&line);
        out.push('\n');
    }
    // Global storage and the compiler's own.
    out.push_str("type inds_ = struct(99) { all_: char(99) @0, x: array[99] of ind @0 }\n");
    out.push_str("static $in: inds_\n");
    for n in SPECIAL_INDS {
        let _ = writeln!(out, "static ${}: ind", n.to_ascii_lowercase());
    }
    out.push_str("static $init_done: ind\n");
    out.push_str("static $status: zoned(5,0)\n");
    out.push_str("static $errind: ind\n");
    out.push_str("static $psds_none: char(1)\n");
    out.push_str(&dbio::statics(files, g.t));
    for (id, s) in g.t.syms.iter().enumerate() {
        if s.scope != 0 || s.template || s.parent.is_some() {
            continue;
        }
        match s.kind {
            Some(Kind::Field | Kind::Ds(_)) => {}
            _ => continue,
        }
        let ty = g.t.ir_storage(s);
        let decl = match &s.storage {
            Storage::Based(p) => format!("based {}: {ty} on {}", s.ir, g.t.syms[*p].ir),
            Storage::Import => format!("import {}: {ty}", s.ir),
            Storage::Export => format!("static {}: {ty}{} export", s.ir, static_inz(s)),
            _ => format!("static {}: {ty}", s.ir),
        };
        let _ = id;
        out.push_str(&decl);
        out.push('\n');
    }
    // Procedures: the main one, then the subprocedures.
    let mut procs = String::new();
    if let Some(main) = &ctl.main {
        procs.push_str(&linear_entry(&mut g, main));
    } else if !ctl.nomain {
        procs.push_str(&main_proc(&mut g, m));
        procs.push_str(&pep(&g.opts.name, &mangle(&g.opts.name)));
    }
    if ctl.main.is_some() || ctl.nomain {
        for f in files.iter().filter(|f| f.scope == 0 && !f.template) {
            g.diags.push(rpgfront::Diag::new(ids::CLOSE_EXPLICITLY, 10, Pos { line: f.def_line, col: 7 }).arg(f.name.clone()));
        }
    }
    for (i, p) in m.procs.iter().enumerate() {
        procs.push_str(&stmt::subprocedure(&mut g, p, locals.get(i).copied().unwrap_or(0)));
    }
    procs.push_str(&init_proc(&mut g));
    for s in std::mem::take(&mut g.subr_bodies) {
        procs.push_str(&s);
    }
    dbio::file_notes(&mut g, m);
    for p in &g.pages {
        let _ = writeln!(out, "static {p}: zoned(4,0)");
    }
    for e in g.externs.values() {
        out.push_str(e);
        out.push('\n');
    }
    out.push_str(&procs);
    (out, g.diags)
}

/// A field's literal `INZ` as a static initializer: an exported field's
/// value is there before any call, a local static's from the first.
pub fn static_inz(s: &crate::syms::Sym) -> String {
    use rpgfront::ast::Lit;
    if s.dim > 0 || !matches!(s.kind, Some(Kind::Field)) {
        return String::new();
    }
    match (&s.inz, &s.ty) {
        (Some(Some(Expr::Lit(Lit::Num(n), _))), t)
            if matches!(t, Ty::Int(_) | Ty::Uns(_) | Ty::Packed(..) | Ty::Zoned(..))
                && !n.contains(['E', 'e'])
                && !(matches!(t, Ty::Int(_) | Ty::Uns(_)) && n.contains('.')) =>
        {
            format!(" inz({n})")
        }
        (Some(Some(Expr::Lit(Lit::Char(c), _))), Ty::Char(len)) if c.chars().count() <= *len as usize => {
            format!(" inz({})", emit::quote(c))
        }
        (Some(Some(Expr::Special { word, .. })), Ty::Ind) if word == "*ON" => " inz(*ON)".to_string(),
        _ => String::new(),
    }
}

/// The special indicators beside `*IN01`-`*IN99`.
pub const SPECIAL_INDS: &[&str] = &[
    "INLR", "INRT", "INMR", "IN1P", "INOA", "INOB", "INOC", "INOD", "INOE", "INOF", "INOG", "INOV",
    "INH1", "INH2", "INH3", "INH4", "INH5", "INH6", "INH7", "INH8", "INH9", "INL1", "INL2", "INL3",
    "INL4", "INL5", "INL6", "INL7", "INL8", "INL9", "INU1", "INU2", "INU3", "INU4", "INU5", "INU6",
    "INU7", "INU8", "INKA", "INKB", "INKC", "INKD", "INKE", "INKF", "INKG", "INKH", "INKI", "INKJ",
    "INKK", "INKL", "INKM", "INKN", "INKP", "INKQ", "INKR", "INKS", "INKT", "INKU", "INKV", "INKW",
    "INKX", "INKY",
];

/// The procedure that sets every global to its initial value: called
/// when the program starts and again after it ended with `LR` on.
fn init_proc(g: &mut Gen<'_>) -> String {
    g.b = Body::new();
    g.scope = 0;
    let ids: Vec<SymId> = (0..g.t.syms.len())
        .filter(|&i| {
            let s = &g.t.syms[i];
            s.scope == 0
                && !s.template
                && s.parent.is_none()
                && matches!(s.kind, Some(Kind::Field | Kind::Ds(_)))
                && matches!(s.storage, Storage::Static | Storage::Export)
        })
        .collect();
    for id in ids {
        stmt::init_symbol(g, id);
    }
    g.b.ins("mov $in.all_, *ZEROS".to_string());
    g.b.ins("dec.mov $status, 0".to_string());
    g.b.ins("store $errind, *OFF".to_string());
    for p in g.pages.clone() {
        g.b.ins(format!("mov {p}, *ZEROS"));
    }
    for n in SPECIAL_INDS {
        g.b.ins(format!("store ${}, *OFF", n.to_ascii_lowercase()));
    }
    let body = std::mem::take(&mut g.b);
    body.finish("proc _qrni_init()", "ret")
}

/// The main procedure: the program entry, running the cycle in IBM's
/// order: heading and detail output (the `1P` output on the first
/// cycle), total calculations and total output, the end when `LR` is
/// on, the detail calculations, and the return when `RT` is on.
fn main_proc(g: &mut Gen<'_>, m: &Module) -> String {
    if cycle::has_primary(g) {
        return cycle::main_proc(g, m);
    }
    g.b = Body::new();
    g.scope = 0;
    g.subrs.clear();
    g.tags.clear();
    g.open.clear();
    let start = g.b.label();
    let init = g.b.label();
    g.b.ins("%first = load $init_done");
    g.b.term(format!("brif %first, {start}, {init}"));
    g.b.place(&init);
    g.b.ins("callb _qrni_init()");
    let files = g.files;
    for f in files.iter().filter(|f| f.scope == 0 && !f.usropn && !f.template) {
        let _ = g.db_open(f);
    }
    g.b.ins("store $init_done, *ON");
    g.psds_init(&g.opts.name.clone());
    g.b.place(&start);
    g.b.ins("store $in1p, *ON");
    // Where RETURN goes: the end, closing when LR is on.
    let finish = g.b.label();
    g.main_exit = Some(finish.clone());
    let (detail, total): (Vec<&Stmt>, Vec<&Stmt>) = m.calcs.iter().partition(|s| !is_total(&s.level));
    // Subroutines become procedures of their own.
    let (detail, subrs) = stmt::split_subroutines(g, &detail);
    g.subr_procs = true;
    let has_pssr = subrs.iter().any(|(n, _)| n == "*PSSR");
    for (name, body) in &subrs {
        stmt::subroutine_proc(g, name, body);
    }
    let outputs = g.outputs.clone();
    let top = g.b.label();
    g.b.place(&top);
    // With a *PSSR, the cycle is one monitor region; its handler runs
    // the subroutine and resumes at the top (re-entering the region)
    // or hands the exception on.
    let pssr_handler = g.b.label();
    if has_pssr {
        g.b.ins(format!("monitor PSSR, *ALL -> {pssr_handler}"));
    }
    g.psds_routine("*DETL");
    g.output(&outputs, output::Step::HeadingDetail);
    g.b.ins("store $in1p, *OFF");
    // Total time: calculations conditioned by LR run when it is on.
    let tot = g.b.label();
    let after_tot = g.b.label();
    g.b.ins("%lr0 = load $inlr");
    g.b.term(format!("brif %lr0, {tot}, {after_tot}"));
    g.b.place(&tot);
    g.psds_routine("*TOTC");
    stmt::calcs(g, &total);
    g.b.place(&after_tot);
    g.output(&outputs, output::Step::Total);
    let end = g.b.label();
    let detail_l = g.b.label();
    g.b.ins("%lr = load $inlr");
    g.b.term(format!("brif %lr, {end}, {detail_l}"));
    g.b.place(&detail_l);
    g.psds_routine("*DETC");
    stmt::calcs(g, &detail);
    let ret = g.b.label();
    g.b.ins("%rt = load $inrt");
    g.b.term(format!("brif %rt, {ret}, {top}"));
    // RETURN: LR decides whether the program ends.
    g.b.place(&finish);
    g.b.ins("%lr2 = load $inlr");
    g.b.term(format!("brif %lr2, {end}, {ret}"));
    if has_pssr {
        let close = g.b.label();
        g.b.place(&close);
        g.b.ins("endmon PSSR");
        g.b.term("unreachable");
        g.b.place(&pssr_handler);
        g.psds_error();
        let rc = g.b.reg();
        g.b.ins(format!("{rc} = callb {}()", Gen::subr_proc("*PSSR")));
        let again = g.b.label();
        let on = g.b.label();
        // RETURN in *PSSR: the end decided by LR, outside the region.
        let finish_out = g.b.label();
        g.b.term(format!(
            "switch {rc}, [{}: {top}, {}: {top}, {}: {finish_out}], default {on}",
            exc::rp::GETIN,
            exc::rp::OTHER,
            exc::rp::RETURN
        ));
        g.b.place(&finish_out);
        g.b.ins("%lr3 = load $inlr");
        g.b.term(format!("brif %lr3, {end}, {ret}"));
        g.b.place(&on);
        g.reraise();
        g.b.term(format!("br {again}"));
        g.b.place(&again);
        g.b.term("ret");
    }
    g.b.place(&end);
    for f in g.files.iter().filter(|f| f.scope == 0 && !f.template) {
        let r = g.b.reg();
        g.b.ins(format!("{r} = dbclose {}", mangle(&f.name)));
    }
    for name in g.called_subrs.clone() {
        if !g.defined_subrs.contains(&name) {
            g.err(ids::NOT_DEFINED, 30, name);
        }
    }
    g.b.ins("store $init_done, *OFF");
    g.b.term("ret");
    g.b.place(&ret);
    g.b.ins("store $inrt, *OFF");
    g.b.term("ret");
    stmt::finish_subrs(g);
    let body = std::mem::take(&mut g.b);
    body.finish(&format!("proc {}()", mangle(&g.opts.name)), "ret")
}

/// The program entry procedure, `_QRNP_PEP_name`, as IBM names it: it
/// calls the main procedure, so an escape the main procedure sends to
/// its caller, or one it leaves unhandled, ends at a control boundary
/// of its own.
fn pep(name: &str, main: &str) -> String {
    format!("proc _QRNP_PEP_{}() entry {{\nL0:\n  callb {main}()\n  ret\n}}\n", mangle(name))
}

/// The program entry of a linear-main module: global storage and files
/// on the first call, then the main procedure; nothing is closed when
/// it returns.
fn linear_entry(g: &mut Gen<'_>, main: &str) -> String {
    g.b = Body::new();
    g.scope = 0;
    let start = g.b.label();
    let init = g.b.label();
    g.b.ins("%first = load $init_done");
    g.b.term(format!("brif %first, {start}, {init}"));
    g.b.place(&init);
    g.b.ins("callb _qrni_init()");
    for f in g.files.iter().filter(|f| f.scope == 0 && !f.usropn && !f.template) {
        let r = g.b.reg();
        g.b.ins(format!("{r} = dbopen {}", mangle(&f.name)));
    }
    g.b.ins("store $init_done, *ON");
    g.psds_init(&g.opts.name.clone());
    g.b.place(&start);
    let up = main.to_ascii_uppercase();
    if !g.local_procs.contains(&up) {
        g.err(ids::NOT_DEFINED, 30, up.clone());
    }
    g.b.ins(format!("callb {}()", mangle(&up)));
    g.b.term("ret");
    let body = std::mem::take(&mut g.b);
    body.finish(&format!("proc _QRNP_PEP_{}() entry", mangle(&g.opts.name)), "ret")
}

/// The `KLIST`s of every procedure: each name and the `KFLD` fields
/// after it.
fn klists(m: &Module) -> HashMap<String, Vec<Expr>> {
    let mut out = HashMap::new();
    let all = m.calcs.iter().chain(m.procs.iter().flat_map(|p| p.calcs.iter()));
    let mut cur: Option<String> = None;
    for s in all {
        match s.op.code.as_str() {
            "KLIST" => {
                cur = s.op.f1.first().and_then(|e| e.name().map(str::to_string));
                if let Some(n) = &cur {
                    out.insert(n.clone(), Vec::new());
                }
            }
            "KFLD" => {
                if let (Some(n), Some(r)) = (&cur, s.op.result.first()) {
                    out.entry(n.clone()).or_insert_with(Vec::new).push(r.clone());
                }
            }
            _ => cur = None,
        }
    }
    out
}

/// Whether a control level puts a calculation at total time.
fn is_total(level: &str) -> bool {
    level == "LR" || level == "L0" || level.len() == 2 && level.starts_with('L') && level[1..].parse::<u8>().is_ok()
}
