// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Code generation: the program as one IR module with one entry
//! procedure. Every statement is a block of its own (`Sn`), so labels
//! and `GOTO` are branches; `DO` groups, loops and `SELECT` keep a
//! stack of frames; `MONMSG` wraps a statement's code in a monitor
//! region whose handlers run the `EXEC` command or go on.

mod cmds;
mod expr;

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use cmd::analyze::Env;

use crate::ast::{Expr, Kind, Lit, Program, Stg, Stmt, Target, VarType};
use crate::diag::{Diag, ids};
use crate::sem::{Tables, Ty, Var};

/// Options that shape the module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// The module name.
    pub name: String,
    /// `ACTGRP`: `*NEW`, `*CALLER` or a name.
    pub actgrp: String,
    /// `DFTACTGRP(*YES)`.
    pub dftactgrp: bool,
    /// `DBGVIEW`: `*NONE`, `*STMT`, `*SOURCE`, `*LIST`, `*ALL`.
    pub dbgview: String,
    /// `LOG`: `*JOB`, `*YES`, `*NO`.
    pub log: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            name: "MAIN".into(),
            actgrp: "*CALLER".into(),
            dftactgrp: false,
            dbgview: "*STMT".into(),
            log: "*JOB".into(),
        }
    }
}

/// A frame of the structure stack.
#[derive(Clone, Debug)]
enum Frame {
    /// A `DO` group: where `ENDDO` goes instead of the next statement
    /// (a `SELECT`'s end, the join past an `ELSE`), the label of the
    /// `IF`'s false branch and the `ELSE` statement to compile there.
    Do {
        exit: Option<String>,
        else_label: Option<String>,
    },
    /// `DOWHILE`, `DOUNTIL`, `DOFOR`: the top and the end.
    Loop {
        top: String,
        end: String,
        label: Option<String>,
        until: Option<Expr>,
        dofor: Option<(String, Expr)>,
    },
    /// `SELECT`: its end and where the next `WHEN` test goes.
    Select { end: String, next: Option<String> },
    /// `SUBR`: the label past it.
    Subr { past: String },
}

/// The generator.
pub struct Gen<'a, 'e> {
    t: &'a Tables,
    env: &'a mut Env<'e>,
    opts: &'a Options,
    body: String,
    decls: Vec<String>,
    types: Vec<String>,
    files: Vec<String>,
    externs: Vec<String>,
    temps: usize,
    labels: usize,
    in_block: bool,
    frames: Vec<Frame>,
    pgm_monmsg: Vec<Monitor>,
    subr_returns: Vec<String>,
    /// Diagnostics.
    pub diags: Vec<Diag>,
    file_open: HashMap<usize, String>,
    has_subr: bool,
    consumed: HashSet<usize>,
    /// Varying temps: exact-length intermediate results.
    varying: HashSet<String>,
}

/// Generate the module text.
pub fn module(p: &Program, t: &Tables, env: &mut Env<'_>, opts: &Options) -> (String, Vec<Diag>) {
    let mut g = Gen {
        t,
        env,
        opts,
        body: String::new(),
        decls: Vec::new(),
        types: Vec::new(),
        files: Vec::new(),
        externs: Vec::new(),
        temps: 0,
        labels: 0,
        in_block: false,
        frames: Vec::new(),
        pgm_monmsg: Vec::new(),
        subr_returns: Vec::new(),
        diags: Vec::new(),
        file_open: HashMap::new(),
        has_subr: p.stmts.iter().any(|s| matches!(s.kind, Kind::Subr(_))),
        consumed: HashSet::new(),
        varying: HashSet::new(),
    };
    g.program(p);
    let text = g.assemble();
    (text, g.diags)
}

impl Gen<'_, '_> {
    /// Emit an instruction into the current block, starting one when
    /// none is open.
    fn ins(&mut self, s: &str) {
        if !self.in_block {
            let l = self.fresh("B");
            self.label(&l);
        }
        let _ = writeln!(self.body, "  {s}");
    }

    /// Emit a terminator; the block ends.
    fn term(&mut self, s: &str) {
        if !self.in_block {
            let l = self.fresh("B");
            self.label(&l);
        }
        let _ = writeln!(self.body, "  {s}");
        self.in_block = false;
    }

    /// Start a block with `l`; an open block falls through to it.
    fn label(&mut self, l: &str) {
        if self.in_block {
            let _ = writeln!(self.body, "  br {l}");
        }
        let _ = writeln!(self.body, "{l}:");
        self.in_block = true;
    }

    /// A new label.
    fn fresh(&mut self, hint: &str) -> String {
        self.labels += 1;
        format!("{hint}{}", self.labels)
    }

    /// A temporary of a type.
    fn temp(&mut self, ty: &Ty) -> String {
        self.temps += 1;
        let name = format!("$_t{}", self.temps);
        let qt = match ty {
            Ty::Char(n) => format!("char({})", (*n).max(1)),
            Ty::Dec(d, s) => format!("packed({},{})", (*d).max(1), s),
            Ty::Lgl => "ind".into(),
            Ty::Ptr => "ptr".into(),
        };
        self.decls.push(format!("  auto {name}: {qt}"));
        name
    }

    /// A varying-length character temporary.
    fn temp_varying(&mut self, n: u32) -> String {
        self.temps += 1;
        let name = format!("$_v{}", self.temps);
        self.decls
            .push(format!("  auto {name}: varchar({})", n.max(1)));
        self.varying.insert(name.clone());
        name
    }

    /// A character operand as fixed storage, for an instruction that
    /// takes the bytes as they lie: a varying temp is copied, padded.
    pub(super) fn fixed(&mut self, s: String, ty: &Ty) -> String {
        if !self.varying.contains(&s) {
            return s;
        }
        let n = match ty {
            Ty::Char(n) => *n,
            _ => 1,
        };
        let t = self.temp(&Ty::Char(n));
        self.ins(&format!("mov {t}, {s}"));
        t
    }

    /// A new register name.
    fn reg(&mut self, hint: &str) -> String {
        self.temps += 1;
        format!("%{hint}{}", self.temps)
    }

    fn err(&mut self, line: usize, id: &str, text: impl Into<String>) {
        self.diags.push(Diag::at(line, id, text));
    }

    fn warn(&mut self, line: usize, id: &str, text: impl Into<String>) {
        self.diags.push(Diag::warn(line, id, text));
    }

    /// The statement label of statement `n`.
    fn stmt_label(n: usize) -> String {
        format!("S{n}")
    }

    fn program(&mut self, p: &Program) {
        // Program-level monitors: the MONMSGs before the first command.
        let first = self.t.first_exec.unwrap_or(usize::MAX);
        for s in &p.stmts {
            if let (Kind::MonMsg(ids, cmp, exec), true) = (&s.kind, s.n < first) {
                self.pgm_monmsg.push((ids.clone(), cmp.clone(), exec.clone()));
            }
        }
        self.declare_vars();
        self.label("L0");
        self.init_vars();
        let mut i = 0;
        while i < p.stmts.len() {
            let s = &p.stmts[i];
            if self.consumed.contains(&s.n) {
                i += 1;
                continue;
            }
            // The monitors right after a statement are its own.
            let mut monitors = Vec::new();
            let mut j = i + 1;
            while let Some(m) = p.stmts.get(j) {
                match &m.kind {
                    Kind::MonMsg(ids, cmp, exec) if s.n >= first => {
                        monitors.push((ids.clone(), cmp.clone(), exec.clone()));
                        j += 1;
                    }
                    _ => break,
                }
            }
            let next_else = p
                .stmts
                .get(j)
                .filter(|e| matches!(e.kind, Kind::Else(_)) && !self.consumed.contains(&e.n));
            self.statement(s, &monitors, next_else);
            i = j;
        }
        if self.in_block {
            self.term("ret");
        }
        if let Some(f) = self.frames.last() {
            let what = match f {
                Frame::Do { .. } | Frame::Loop { .. } => "a DO group",
                Frame::Select { .. } => "a SELECT",
                Frame::Subr { .. } => "a subroutine",
            };
            let line = p.stmts.last().map_or(1, |s| s.line);
            self.err(line, ids::STRUCTURE, format!("{what} is not ended"));
        }
        if self.has_subr {
            self.subr_dispatch();
        }
    }

    /// Declarations: parameters are the entry procedure's, the rest
    /// automatic variables; files and their record buffers.
    fn declare_vars(&mut self) {
        for name in &self.t.order {
            let v = &self.t.vars[name];
            if v.parm {
                continue;
            }
            match &v.stg {
                Stg::Auto => self
                    .decls
                    .push(format!("  auto {}: {}", v.place(), v.qir_type())),
                Stg::Based(p) => {
                    self.decls
                        .push(format!("  based {}: {} on ${p}", v.place(), v.qir_type()));
                }
                Stg::Defined(..) => {
                    self.decls.push(format!("  auto $_dp_{}: ptr", v.name));
                    self.decls.push(format!(
                        "  based {}: {} on $_dp_{}",
                        v.place(),
                        v.qir_type(),
                        v.name
                    ));
                }
            }
        }
        if self.has_subr {
            self.decls.push("  auto $_rs: array[64] of i32".into());
            self.decls.push("  auto $_rsp: i32 inz(0)".into());
            self.decls.push("  auto $_rix: i32 inz(0)".into());
            self.decls.push("  auto $_srv: packed(10,0)".into());
        }
        for f in &self.t.files {
            let fname = format!("F{}", f.n);
            let usage = if f.display {
                "dev(*DSP)".to_string()
            } else {
                "usage(*INPUT) nolvlchk".to_string()
            };
            let mut fmts = String::new();
            for m in &f.formats {
                let _ = write!(fmts, " fmt({})", m.name);
            }
            self.files.push(format!(
                "file {fname} file({}) lib({}) {usage}{fmts}",
                f.name, f.lib
            ));
            for (k, m) in f.formats.iter().enumerate() {
                let mut fields = Vec::new();
                for fd in &m.fields {
                    let ty = match fd.ty {
                        'P' => format!("packed({},{})", fd.digits.max(1), fd.decimals),
                        'S' => format!("zoned({},{})", fd.digits.max(1), fd.decimals),
                        'B' => {
                            if fd.len <= 2 {
                                "i16".into()
                            } else {
                                "i32".into()
                            }
                        }
                        _ => format!("char({})", fd.len.max(1)),
                    };
                    fields.push(format!("{}: {ty} @{}", fd.name, fd.pos.saturating_sub(1)));
                }
                self.types.push(format!(
                    "type R{}_{k} = struct({}) {{ {} }}",
                    f.n,
                    m.len.max(1),
                    fields.join(", ")
                ));
                self.decls
                    .push(format!("  auto $_r{}_{k}: R{}_{k}", f.n, f.n));
            }
            let open = format!("$_open{}", f.n);
            self.decls.push(format!("  auto {open}: ind inz(*OFF)"));
            self.file_open.insert(f.n, open);
        }
    }

    /// Initial values and the pointers of `STG(*DEFINED)` variables.
    fn init_vars(&mut self) {
        for name in &self.t.order {
            let v = &self.t.vars[name];
            if v.parm || v.file.is_some() {
                continue;
            }
            if let Stg::Defined(base, pos) = &v.stg {
                let r = self.reg("p");
                self.ins(&format!("{r} = addr ${base}"));
                if *pos > 1 {
                    let r2 = self.reg("p");
                    self.ins(&format!("{r2} = ptradd {r}, {}", pos - 1));
                    self.ins(&format!("store $_dp_{}, {r2}", v.name));
                } else {
                    self.ins(&format!("store $_dp_{}, {r}", v.name));
                }
            }
            if let Some(lit) = &v.value {
                self.assign_lit(v, lit);
            } else if matches!(v.stg, Stg::Auto) {
                match v.ty {
                    VarType::Char => self.ins(&format!("mov {}, *BLANKS", v.place())),
                    VarType::Dec | VarType::Int | VarType::Uint => {
                        self.ins(&format!("dec.mov {}, 0", v.place()));
                    }
                    VarType::Lgl => self.ins(&format!("store {}, *OFF", v.place())),
                    VarType::Ptr => {}
                }
            }
        }
    }

    /// A literal into a variable.
    fn assign_lit(&mut self, v: &Var, lit: &Lit) {
        let place = v.place();
        match (v.ty, lit) {
            (VarType::Char, Lit::Str(s)) => self.ins(&format!("mov {place}, {}", expr::qstr(s))),
            (VarType::Char, Lit::Hex(b)) => self.ins(&format!("mov {place}, {}", expr::qhex(b))),
            (VarType::Char, Lit::Num(n)) => self.ins(&format!("mov {place}, {}", expr::qstr(n))),
            (VarType::Dec | VarType::Int | VarType::Uint, Lit::Num(n)) => {
                self.ins(&format!("dec.mov {place}, {n}"));
            }
            (VarType::Dec | VarType::Int | VarType::Uint, Lit::Str(s)) => {
                self.ins(&format!("cvt {place}, {}", expr::qstr(s)));
            }
            (VarType::Lgl, Lit::Str(s)) => {
                self.ins(&format!(
                    "store {place}, {}",
                    if s == "1" { "*ON" } else { "*OFF" }
                ));
            }
            (VarType::Lgl, Lit::Num(n)) => {
                self.ins(&format!(
                    "store {place}, {}",
                    if n == "1" { "*ON" } else { "*OFF" }
                ));
            }
            _ => self.err(
                v.line,
                ids::TYPE_MISMATCH,
                format!("&{}: the value is not of its type", v.name),
            ),
        }
    }

    /// One statement with its monitors; `next_else` is the `ELSE` that
    /// follows an `IF`.
    fn statement(
        &mut self,
        s: &Stmt,
        monitors: &[Monitor],
        next_else: Option<&Stmt>,
    ) {
        let l = Self::stmt_label(s.n);
        self.label(&l);
        if self.opts.dbgview != "*NONE" {
            self.ins(&format!("debug.stmt {}", s.n));
        }
        let monitored = !matches!(
            s.kind,
            Kind::Pgm(_)
                | Kind::EndPgm
                | Kind::Dcl(_)
                | Kind::DclF(_)
                | Kind::DclPrcOpt(_)
                | Kind::MonMsg(..)
                | Kind::Do
                | Kind::EndDo
                | Kind::Select
                | Kind::EndSelect
                | Kind::Subr(_)
                | Kind::EndSubr(_)
                | Kind::Goto(_)
                | Kind::Return
                | Kind::Leave(_)
                | Kind::Iterate(_)
                | Kind::If(..)
                | Kind::Else(_)
                | Kind::When(..)
                | Kind::Otherwise(_)
                | Kind::DoWhile(_)
                | Kind::DoUntil(_)
                | Kind::DoFor(..)
                | Kind::CallSubr(..)
                | Kind::RtnSubr(_)
        );
        let mut all: Vec<Monitor> = monitors.to_vec();
        if monitored {
            for (ids, cmp, exec) in &self.pgm_monmsg {
                if !all
                    .iter()
                    .any(|(own, _, _)| own.iter().any(|i| ids.contains(i)))
                {
                    all.push((ids.clone(), cmp.clone(), exec.clone()));
                }
            }
        }
        if !monitored || all.is_empty() {
            self.kind(s, next_else);
            return;
        }
        // The monitor region around the statement, the handlers after.
        let region = self.fresh("M");
        let next = self.fresh("N");
        let mut handlers: Vec<(String, Monitor)> =
            Vec::new();
        let mut sels = Vec::new();
        for (ids, cmp, exec) in &all {
            let h = self.fresh("H");
            for id in ids {
                sels.push(format!("{} -> {h}", selector(id)));
            }
            handlers.push((h, (ids.clone(), cmp.clone(), exec.clone())));
        }
        self.ins(&format!("monitor {region}, {}", sels.join(", ")));
        self.kind(s, next_else);
        self.ins(&format!("endmon {region}"));
        self.term(&format!("br {next}"));
        for (i, (h, (_, cmp, exec))) in handlers.iter().enumerate() {
            self.label(h);
            if let Some(c) = cmp {
                let later: Vec<(String, Vec<String>)> = handlers[i + 1..]
                    .iter()
                    .map(|(h, (ids, _, _))| (h.clone(), ids.clone()))
                    .collect();
                self.cmpdta(c, &later);
            }
            if let Some(e) = exec {
                self.nested(e);
            }
            if self.in_block {
                self.term(&format!("br {next}"));
            }
        }
        self.label(&next);
    }

    /// `MONMSG CMPDTA`: the handler keeps the message only when its data
    /// starts with the comparison data. Otherwise the statement's next
    /// monitor that names the message takes it, and failing that the
    /// message is sent again as an escape from the handler, outside
    /// the region, where no monitor applies.
    fn cmpdta(&mut self, cmp: &str, later: &[(String, Vec<String>)]) {
        let mid = self.temp(&Ty::Char(7));
        let data = self.temp(&Ty::Char(512));
        let got = self.reg("g");
        self.ins(&format!(
            "{got} = rcvmsg pgmq(*SAME) type(*ESCAPE) msgid({mid}) data({data})"
        ));
        let n = u32::try_from(cmp.len()).unwrap_or(1).max(1);
        let head = self.temp_varying(n);
        self.ins(&format!("str.subst {head}, {data}, 1, {n}"));
        let eq = self.reg("c");
        self.ins(&format!("{eq} = str.cmp.eq {head}, {}", expr::qstr(cmp)));
        let keep = self.fresh("C");
        let miss = self.fresh("X");
        self.term(&format!("brif {eq}, {keep}, {miss}"));
        self.label(&miss);
        for (h, ids) in later {
            for id in ids {
                let id = id.to_ascii_uppercase();
                let len = if id.len() == 7 && id.ends_with("0000") {
                    3
                } else if id.len() == 7 && id.ends_with("00") {
                    5
                } else {
                    id.len().min(7)
                };
                let part = self.temp_varying(7);
                self.ins(&format!("str.subst {part}, {mid}, 1, {len}"));
                let same = self.reg("c");
                self.ins(&format!("{same} = str.cmp.eq {part}, {}", expr::qstr(&id[..len])));
                let no = self.fresh("X");
                self.term(&format!("brif {same}, {h}, {no}"));
                self.label(&no);
            }
        }
        self.ins(&format!(
            "sndpgmmsg msgid({mid}) msgf(\"QCPFMSG\") data({data}) type(*ESCAPE) to(*SAME)"
        ));
        self.term(&format!("br {keep}"));
        self.label(&keep);
    }

    /// A nested statement (`THEN`, `ELSE`, `EXEC`, `OTHERWISE`): the
    /// statement number is its parent's.
    fn nested(&mut self, s: &Stmt) {
        self.kind(s, None);
    }

    /// The code of a statement.
    fn kind(&mut self, s: &Stmt, next_else: Option<&Stmt>) {
        match &s.kind {
            Kind::Pgm(_) | Kind::Dcl(_) | Kind::DclF(_) | Kind::DclPrcOpt(_) | Kind::MonMsg(..) => {
            }
            Kind::EndPgm | Kind::Return => self.term("ret"),
            Kind::ChgVar(target, e) => self.chgvar(s.line, target, e),
            Kind::If(cond, then) => self.if_stmt(s, cond, then.as_deref(), next_else),
            Kind::Else(then) => {
                // An ELSE with its IF is compiled there; one alone has
                // no IF and runs its command.
                if let Some(t) = then {
                    self.nested(t);
                }
            }
            Kind::Do => self.frames.push(Frame::Do {
                exit: None,
                else_label: None,
            }),
            Kind::EndDo => self.enddo(s.line, next_else),
            Kind::DoWhile(cond) => {
                let top = self.fresh("W");
                let end = self.fresh("E");
                self.label(&top);
                match self.lgl(cond) {
                    Ok(r) => {
                        let body = self.fresh("Y");
                        self.term(&format!("brif {r}, {body}, {end}"));
                        self.label(&body);
                    }
                    Err(e) => self.err(s.line, ids::EXPRESSION, e),
                }
                self.frames.push(Frame::Loop {
                    top,
                    end,
                    label: s.label.clone(),
                    until: None,
                    dofor: None,
                });
            }
            Kind::DoUntil(cond) => {
                let top = self.fresh("U");
                let end = self.fresh("E");
                self.label(&top);
                self.frames.push(Frame::Loop {
                    top,
                    end,
                    label: s.label.clone(),
                    until: Some(cond.clone()),
                    dofor: None,
                });
            }
            Kind::DoFor(var, from, to, by) => self.dofor(s, var, from, to, by),
            Kind::Leave(label) | Kind::Iterate(label) => {
                let leave = matches!(s.kind, Kind::Leave(_));
                let target = self.frames.iter().rev().find_map(|f| match f {
                    Frame::Loop {
                        top, end, label: l, ..
                    } if label.is_none() || label.as_deref() == l.as_deref() => {
                        Some(if leave { end.clone() } else { top.clone() })
                    }
                    _ => None,
                });
                match target {
                    Some(t) => self.term(&format!("br {t}")),
                    None => self.err(s.line, ids::STRUCTURE, "LEAVE and ITERATE need a loop"),
                }
            }
            Kind::Select => {
                let end = self.fresh("X");
                self.frames.push(Frame::Select { end, next: None });
            }
            Kind::When(cond, then) => self.when(s, cond, then.as_deref()),
            Kind::Otherwise(then) => {
                let (end, next) = match self.frames.last() {
                    Some(Frame::Select { end, next }) => (end.clone(), next.clone()),
                    _ => {
                        self.err(s.line, ids::STRUCTURE, "OTHERWISE needs a SELECT");
                        return;
                    }
                };
                if let Some(n) = next {
                    self.label(&n);
                    if let Some(Frame::Select { next, .. }) = self.frames.last_mut() {
                        *next = None;
                    }
                }
                match then {
                    Some(t) if matches!(t.kind, Kind::Do) => self.frames.push(Frame::Do {
                        exit: Some(end),
                        else_label: None,
                    }),
                    Some(t) => self.nested(t),
                    None => {}
                }
            }
            Kind::EndSelect => match self.frames.pop() {
                Some(Frame::Select { end, next }) => {
                    if let Some(n) = next {
                        self.label(&n);
                    }
                    self.label(&end);
                }
                _ => self.err(s.line, ids::STRUCTURE, "ENDSELECT without SELECT"),
            },
            Kind::Goto(label) => match self.t.labels.get(label) {
                Some(n) => {
                    let l = Self::stmt_label(*n);
                    self.term(&format!("br {l}"));
                }
                None => self.err(
                    s.line,
                    ids::NO_LABEL,
                    format!("label {label} is not defined"),
                ),
            },
            Kind::Subr(name) => {
                if !self.frames.is_empty() {
                    self.err(s.line, ids::STRUCTURE, "SUBR lies inside a group");
                }
                let past = self.fresh("P");
                self.term(&format!("br {past}"));
                self.label(&format!("SUBR_{name}"));
                self.frames.push(Frame::Subr { past });
            }
            Kind::EndSubr(rtn) | Kind::RtnSubr(rtn) => {
                if let Some(e) = rtn {
                    match self.num(e) {
                        Ok(v) => self.ins(&format!("dec.mov $_srv, {v}")),
                        Err(err) => self.err(s.line, ids::EXPRESSION, err),
                    }
                }
                self.subr_return();
                if matches!(s.kind, Kind::EndSubr(_)) {
                    match self.frames.pop() {
                        Some(Frame::Subr { past }) => self.label(&past),
                        _ => self.err(s.line, ids::STRUCTURE, "ENDSUBR without SUBR"),
                    }
                }
            }
            Kind::CallSubr(name, rtn) => self.callsubr(s, name, rtn.as_ref()),
            Kind::Call(target, parms) => self.call(s, target, parms),
            Kind::TfrCtl(target, parms) => {
                self.call(s, target, parms);
                self.term("ret");
            }
            Kind::CallPrc(prc, parms, rtn) => self.callprc(s, prc, parms, rtn.as_ref()),
            Kind::FileIo(op, opnid, fmt) => self.file_io(s, *op, opnid.as_deref(), fmt.as_deref()),
            Kind::Close(opnid) => self.close(s, opnid.as_deref()),
            Kind::RtvDtaAra(area, target) => self.rtvdtaara(s, area, target),
            Kind::ChgDtaAra(area, value) => self.chgdtaara(s, area, value),
            Kind::DmpClPgm => self.ins("call QSYS/QCLDMP()"),
            Kind::Command(toks) => self.command(s, toks),
        }
    }

    fn enddo(&mut self, line: usize, next_else: Option<&Stmt>) {
        match self.frames.pop() {
            Some(Frame::Do { exit, else_label }) => match (exit, else_label, next_else) {
                // The IF's group ends and an ELSE follows: the group
                // skips it, the false branch runs it.
                (None, Some(e), Some(stmt)) => {
                    self.consumed.insert(stmt.n);
                    let join = self.fresh("J");
                    self.term(&format!("br {join}"));
                    self.label(&e);
                    if let Kind::Else(Some(body)) = &stmt.kind {
                        self.nested(body);
                    }
                    self.label(&join);
                }
                (Some(x), _, _) => self.term(&format!("br {x}")),
                (None, Some(e), None) => self.label(&e),
                (None, None, _) => {}
            },
            Some(Frame::Loop {
                top,
                end,
                until,
                dofor,
                ..
            }) => {
                if let Some(cond) = until {
                    match self.lgl(&cond) {
                        Ok(r) => self.term(&format!("brif {r}, {end}, {top}")),
                        Err(e) => self.err(line, ids::EXPRESSION, e),
                    }
                } else if let Some((var, by)) = dofor {
                    match self.num(&by) {
                        Ok(b) => {
                            let v = &self.t.vars[&var];
                            let (d, s) = match v.ty() {
                                Ty::Dec(d, s) => (d, s),
                                _ => (10, 0),
                            };
                            self.ins(&format!("dec.add ${var}, ${var}, {b} prec({d},{s})"));
                        }
                        Err(e) => self.err(line, ids::EXPRESSION, e),
                    }
                    self.term(&format!("br {top}"));
                } else {
                    self.term(&format!("br {top}"));
                }
                self.label(&end);
            }
            _ => self.err(line, ids::STRUCTURE, "ENDDO without DO"),
        }
    }

    fn if_stmt(&mut self, s: &Stmt, cond: &Expr, then: Option<&Stmt>, next_else: Option<&Stmt>) {
        let r = match self.lgl(cond) {
            Ok(r) => r,
            Err(e) => {
                self.err(s.line, ids::EXPRESSION, e);
                return;
            }
        };
        let yes = self.fresh("T");
        let no = self.fresh("F");
        self.term(&format!("brif {r}, {yes}, {no}"));
        self.label(&yes);
        match then {
            Some(t) if matches!(t.kind, Kind::Do) => {
                // The group runs to ENDDO, where an ELSE may follow.
                self.frames.push(Frame::Do {
                    exit: None,
                    else_label: Some(no),
                });
            }
            Some(t) => {
                self.nested(t);
                match next_else {
                    Some(e) => {
                        self.consumed.insert(e.n);
                        let join = self.fresh("J");
                        self.term(&format!("br {join}"));
                        self.label(&no);
                        if let Kind::Else(Some(body)) = &e.kind {
                            self.nested(body);
                        }
                        self.label(&join);
                    }
                    None => self.label(&no),
                }
            }
            None => self.label(&no),
        }
    }

    fn when(&mut self, s: &Stmt, cond: &Expr, then: Option<&Stmt>) {
        let (end, next) = match self.frames.last() {
            Some(Frame::Select { end, next }) => (end.clone(), next.clone()),
            _ => {
                self.err(s.line, ids::STRUCTURE, "WHEN needs a SELECT");
                return;
            }
        };
        if let Some(n) = next {
            self.label(&n);
        }
        let r = match self.lgl(cond) {
            Ok(r) => r,
            Err(e) => {
                self.err(s.line, ids::EXPRESSION, e);
                return;
            }
        };
        let yes = self.fresh("T");
        let next = self.fresh("C");
        self.term(&format!("brif {r}, {yes}, {next}"));
        if let Some(Frame::Select { next: n, .. }) = self.frames.last_mut() {
            *n = Some(next);
        }
        self.label(&yes);
        match then {
            Some(t) if matches!(t.kind, Kind::Do) => self.frames.push(Frame::Do {
                exit: Some(end),
                else_label: None,
            }),
            Some(t) => {
                self.nested(t);
                self.term(&format!("br {end}"));
            }
            None => self.term(&format!("br {end}")),
        }
    }

    fn dofor(&mut self, s: &Stmt, var: &str, from: &Expr, to: &Expr, by: &Expr) {
        if self.t.var(var).is_none() {
            self.err(s.line, ids::UNDECLARED, format!("&{var} is not declared"));
            return;
        }
        match self.num(from) {
            Ok(f) => self.ins(&format!("dec.mov ${var}, {f}")),
            Err(e) => self.err(s.line, ids::EXPRESSION, e),
        }
        let positive = match by {
            Expr::Lit(Lit::Num(n)) => !n.starts_with('-'),
            Expr::Neg(_) => false,
            _ => true,
        };
        let top = self.fresh("R");
        let end = self.fresh("E");
        self.label(&top);
        match self.num(to) {
            Ok(limit) => {
                let r = self.reg("c");
                let cond = if positive { "gt" } else { "lt" };
                self.ins(&format!("{r} = dec.cmp.{cond} ${var}, {limit}"));
                let body = self.fresh("Y");
                self.term(&format!("brif {r}, {end}, {body}"));
                self.label(&body);
            }
            Err(e) => self.err(s.line, ids::EXPRESSION, e),
        }
        self.frames.push(Frame::Loop {
            top,
            end,
            label: s.label.clone(),
            until: None,
            dofor: Some((var.to_string(), by.clone())),
        });
    }

    /// `CHGVAR`.
    fn chgvar(&mut self, line: usize, target: &Target, e: &Expr) {
        let Some(v) = self.t.var(target.var()).cloned() else {
            self.err(
                line,
                ids::UNDECLARED,
                format!("&{} is not declared", target.var()),
            );
            return;
        };
        let r = match target {
            Target::Var(_) => self.assign(&v, e),
            Target::Sst(_, start, len) => self.assign_sst(&v, start, len, e),
            Target::Bin(_, start, len) => self.assign_bin(&v, start, len, e),
        };
        if let Err(err) = r {
            self.err(line, ids::TYPE_MISMATCH, err);
        }
    }

    /// `RTNSUBR` and `ENDSUBR`: pop the return index and go to the
    /// dispatcher.
    fn subr_return(&mut self) {
        let sp = self.reg("sp");
        self.ins(&format!("{sp} = load $_rsp"));
        let sp2 = self.reg("sp");
        self.ins(&format!("{sp2} = sub {sp}, 1"));
        self.ins(&format!("store $_rsp, {sp2}"));
        let idx = self.reg("ix");
        self.ins(&format!("{idx} = load $_rs[{sp2}]"));
        self.ins(&format!("store $_rix, {idx}"));
        self.term("br SUBR_RET");
    }

    /// The dispatcher every subroutine returns through.
    fn subr_dispatch(&mut self) {
        self.label("SUBR_RET");
        let ix = self.reg("ix");
        self.ins(&format!("{ix} = load $_rix"));
        let arms: Vec<String> = self
            .subr_returns
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{i}: {l}"))
            .collect();
        self.term(&format!(
            "switch {ix}, [{}], default SUBR_BAD",
            arms.join(", ")
        ));
        self.label("SUBR_BAD");
        self.term("unreachable");
    }

    /// Finish the module text.
    fn assemble(&self) -> String {
        let mut m = String::new();
        let _ = writeln!(
            m,
            "module {} lang(CLLE) ccsid(37) actgrp({}){} dbgview({})",
            self.opts.name,
            self.opts.actgrp,
            if self.opts.dftactgrp {
                " dftactgrp"
            } else {
                ""
            },
            self.opts.dbgview
        );
        for t in &self.types {
            let _ = writeln!(m, "{t}");
        }
        for f in &self.files {
            let _ = writeln!(m, "{f}");
        }
        for e in &self.externs {
            let _ = writeln!(m, "{e}");
        }
        let parms: Vec<String> = self
            .t
            .parms
            .iter()
            .filter_map(|p| self.t.vars.get(p))
            .map(|v| format!("{}: {}", v.place(), v.qir_type()))
            .collect();
        let _ = writeln!(m, "proc {}({}) entry {{", self.opts.name, parms.join(", "));
        for d in &self.decls {
            let _ = writeln!(m, "{d}");
        }
        m.push_str(&self.body);
        m.push_str("}\n");
        m
    }
}

/// A `MONMSG`: its identifiers, `CMPDTA` and `EXEC`.
type Monitor = (Vec<String>, Option<String>, Option<Box<Stmt>>);

/// A `MONMSG` identifier as a monitor selector: `CPF0000` is every
/// `CPF` message, `CPF2200` every `CPF22nn`.
fn selector(id: &str) -> String {
    let id = id.to_ascii_uppercase();
    if id.len() == 7 {
        if id.ends_with("0000") {
            return format!("\"{}*\"", &id[..3]);
        }
        if id.ends_with("00") {
            return format!("\"{}*\"", &id[..5]);
        }
    }
    id
}
