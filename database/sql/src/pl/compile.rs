// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The body to the IR. One entry procedure; variables are automatic
//! storage with a `SMALLINT` indicator each; after every SQL statement
//! the SQLSTATE's class decides: `00` carries on, anything else goes to
//! the innermost handler for it, or (an exception no handler takes)
//! out of the routine with the SQLCA.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use super::rewrite::{self, Names};
use super::types::{HostType, host_type};
use super::{Catalog, Role, Signature};
use crate::ast::{
    AssignSource, Compound, CursorSource, Expr, GetDiag, HandlerCond, HandlerKind, Ident, Literal,
    ObjectName, ParmMode, PlCursor, PlExpr, PlKind, PlSql, PlStmt, ReturnTo, Signal, SignalValue,
    Statement, TypeSpec,
};
use crate::embed::{Desc, Op};
use crate::error::{Result, SqlError};

mod array;

fn unsupported(what: &str) -> SqlError {
    SqlError::Unsupported { what: what.into() }
}

fn undefined(name: &str) -> SqlError {
    SqlError::Coded {
        code: -312,
        state: "42618",
        what: format!("variable {name} is not declared or not usable here"),
    }
}

/// A variable or parameter.
#[derive(Clone, Debug)]
struct Var {
    place: String,
    ind: String,
    ty: HostType,
    sql: TypeSpec,
    constant: bool,
    /// An array: its element type and largest cardinality; the storage
    /// holds its handle (`crate::embed` arrays).
    array: Option<(TypeSpec, u64)>,
}

/// A cursor of a compound statement.
#[derive(Clone, Debug)]
struct Cur {
    decl: PlCursor,
}

/// A handler in effect.
#[derive(Clone, Debug)]
struct Hand {
    kind: HandlerKind,
    conds: Vec<HandlerCond>,
    entry: String,
    /// Statements that went to it: their numbers and where they carry on.
    resumes: Vec<(u32, String)>,
}

/// One level of names: a compound statement, a `FOR` loop, the routine.
#[derive(Default)]
struct Frame {
    label: Option<String>,
    vars: Vec<(String, usize)>,
    conds: HashMap<String, String>,
    cursors: HashMap<String, Cur>,
    handlers: Vec<Hand>,
    /// Whether this frame's handlers take conditions now (not while
    /// its own handlers run).
    active: bool,
    /// An `ATOMIC` compound: its savepoint.
    atomic: Option<String>,
}

/// A statement `LEAVE` and `ITERATE` reach.
struct Target {
    label: String,
    exit: String,
    iterate: Option<String>,
    /// The frames around it: those above are left by `LEAVE`.
    depth: usize,
}

#[allow(clippy::struct_excessive_bools)] // the generator's modes
struct Gen<'a> {
    sig: &'a Signature,
    cat: &'a mut dyn Catalog,
    consts: Vec<String>,
    autos: Vec<String>,
    body: Vec<String>,
    open: bool,
    labels: u32,
    temps: u32,
    stmts: u32,
    vars: Vec<Var>,
    frames: Vec<Frame>,
    targets: Vec<Target>,
    /// Labels of labelled statements, for `GOTO`.
    gotos: HashMap<String, String>,
    sqlstate_var: Option<usize>,
    sqlcode_var: Option<usize>,
    fail: String,
    done: String,
    for_count: u32,
    /// The current block has `%pca`.
    pca: bool,
    /// `ATOMIC` compounds so far.
    atomics: u32,
    /// Markers stay bare (a `CALL`'s arguments, which may be outputs).
    uncast: bool,
    /// Every marker is cast to its variable's type (a result set's
    /// query: its columns describe as the primary's do).
    typed: bool,
}

impl Names for Gen<'_> {
    fn variable(&self, name: &str) -> Option<usize> {
        self.frames.iter().rev().find_map(|f| {
            f.vars
                .iter()
                .rev()
                .find(|(n, _)| n == name)
                .map(|(_, k)| *k)
        })
    }

    fn type_of(&self, k: usize) -> Option<String> {
        let v = &self.vars[k];
        if v.array.is_some() {
            None
        } else {
            Some(super::types::type_text(&v.sql))
        }
    }

    fn cast_type(&self, k: usize) -> Option<String> {
        if self.uncast {
            None
        } else if self.typed {
            self.type_of(k)
        } else {
            super::types::marker_type(&self.vars[k].sql)
        }
    }

    fn element_type(&self, k: usize) -> Option<String> {
        self.vars[k]
            .array
            .as_ref()
            .map(|(t, _)| super::types::type_text(t))
    }

    fn qualified(&self, label: &str, name: &str) -> Option<usize> {
        self.frames
            .iter()
            .rev()
            .find(|f| f.label.as_deref() == Some(label))
            .and_then(|f| f.vars.iter().find(|(n, _)| n == name).map(|(_, k)| *k))
    }
}

/// Compile a routine body to the IR text of its module.
///
/// # Errors
///
/// A statement the compiler refuses (`SQL0084` and the specific codes:
/// an undeclared variable `SQL0312`, a label not found `SQL0778`).
pub fn compile(sig: &Signature, body: &PlStmt, cat: &mut dyn Catalog) -> Result<String> {
    let mut g = Gen {
        sig,
        cat,
        consts: Vec::new(),
        autos: Vec::new(),
        body: Vec::new(),
        open: false,
        labels: 0,
        temps: 0,
        stmts: 0,
        vars: Vec::new(),
        frames: Vec::new(),
        targets: Vec::new(),
        gotos: HashMap::new(),
        sqlstate_var: None,
        sqlcode_var: None,
        fail: String::new(),
        done: String::new(),
        for_count: 0,
        pca: false,
        atomics: 0,
        uncast: false,
        typed: false,
    };
    g.fail = g.label();
    g.done = g.label();
    g.collect_gotos(body);
    g.routine(body)?;
    Ok(g.module())
}

impl Gen<'_> {
    fn label(&mut self) -> String {
        self.labels += 1;
        format!("L{}", self.labels)
    }

    fn ins(&mut self, s: impl Into<String>) {
        if !self.open {
            let l = self.label();
            self.body.push(format!("{l}:"));
            self.open = true;
            self.pca = false;
        }
        let s = s.into();
        // The SQLCA's address, once in each block that reads it.
        if s.contains("%pca") && !s.starts_with("%pca =") && !self.pca {
            self.body.push("  %pca = addr $sqlca".into());
            self.pca = true;
        }
        self.body.push(format!("  {s}"));
    }

    fn term(&mut self, s: impl Into<String>) {
        self.ins(s);
        self.open = false;
    }

    fn place(&mut self, l: &str) {
        if self.open {
            self.body.push(format!("  br {l}"));
        }
        self.body.push(format!("{l}:"));
        self.open = true;
        self.pca = false;
    }

    fn auto(&mut self, ty: &str) -> String {
        self.temps += 1;
        let n = format!("$t{}", self.temps);
        self.autos.push(format!("  auto {n}: {ty}"));
        n
    }

    /// A string constant: its name and length in bytes.
    fn text_const(&mut self, s: &str) -> Result<(String, usize)> {
        let len = ccsid::to_ebcdic(s)
            .map_err(|_| unsupported("characters outside CCSID 37 in a routine's statements"))?
            .len();
        self.temps += 1;
        let n = format!("$k{}", self.temps);
        self.consts.push(format!(
            "const {n}: char({}) = \"{}\"",
            len.max(1),
            s.replace('"', "\"\"")
        ));
        Ok((n, len))
    }

    fn module(&self) -> String {
        let p = &self.sig.program;
        let mut out = String::new();
        let _ = writeln!(out, "module {p} lang(QIR) ccsid(37) actgrp(*CALLER)");
        out.push_str("extern proc _QRNX_SQLIN($v: bytes(1) *VARSIZE, byval $d: i64, byval $x: i64, $i: bytes(1) *VARSIZE, byval $hi: i64)\n");
        out.push_str("extern proc _QRNX_SQLOUT($v: bytes(1) *VARSIZE, byval $d: i64, byval $x: i64, $i: bytes(1) *VARSIZE, byval $hi: i64, byval $st: i64)\n");
        out.push_str("extern proc _QRNX_SQLRUN($ca: bytes(1) *VARSIZE, $t: bytes(1) *VARSIZE, byval $tl: i64, $s: bytes(1) *VARSIZE, byval $sl: i64)\n");
        let _ = writeln!(out, "const $scope: char({}) = \"{}\"", p.len(), p);
        for c in &self.consts {
            out.push_str(c);
            out.push('\n');
        }
        let mut params: Vec<String> = Vec::new();
        for (i, pm) in self.sig.parms.iter().enumerate() {
            params.push(format!("$p{}: {}", i + 1, self.vars[i].ty.qir));
            let _ = pm;
        }
        for i in 0..self.sig.parms.len() {
            params.push(format!("$n{}: i16", i + 1));
        }
        params.push("$state: char(5)".into());
        params.push("$rname: varchar(517)".into());
        params.push("$sname: varchar(128)".into());
        params.push("$diag: varchar(70)".into());
        params.push("$ca: bytes(136) *NOPASS".into());
        // A trigger's body is called by the program's entry, which maps the
        // trigger buffer onto its parameters.
        match self.sig.role {
            Role::Trigger => {
                let _ = writeln!(
                    out,
                    "proc {}({}) {{",
                    super::TRIGGER_BODY,
                    params.join(", ")
                );
            }
            Role::Procedure => {
                let _ = writeln!(out, "proc {p}({}) entry {{", params.join(", "));
            }
        }
        out.push_str("  auto $sqlca: bytes(136)\n  auto $hca: bytes(136)\n  auto $uca: bytes(136)\n  auto $cls: char(2)\n  auto $resume: i32\n  auto $rc: i32\n");
        for a in &self.autos {
            out.push_str(a);
            out.push('\n');
        }
        for l in &self.body {
            out.push_str(l);
            out.push('\n');
        }
        out.push_str("}\n");
        out
    }

    /// The labels of every labelled statement.
    fn collect_gotos(&mut self, s: &PlStmt) {
        if let Some(l) = &s.label {
            let q = self.label();
            self.gotos.insert(l.value.clone(), q);
        }
        let each = |v: &[PlStmt], g: &mut Self| v.iter().for_each(|s| g.collect_gotos(s));
        match &s.kind {
            PlKind::Compound(c) => {
                each(&c.body, self);
                for h in &c.handlers {
                    self.collect_gotos(&h.body);
                }
            }
            PlKind::If {
                branches,
                otherwise,
            } => {
                for (_, b) in branches {
                    each(b, self);
                }
                if let Some(o) = otherwise {
                    each(o, self);
                }
            }
            PlKind::Case {
                whens, otherwise, ..
            } => {
                for (_, b) in whens {
                    each(b, self);
                }
                if let Some(o) = otherwise {
                    each(o, self);
                }
            }
            PlKind::Loop(b)
            | PlKind::While(_, b)
            | PlKind::Repeat(b, _)
            | PlKind::For { body: b, .. } => each(b, self),
            _ => {}
        }
    }

    /// The entry: parameters, the body, the two ways out.
    fn routine(&mut self, body: &PlStmt) -> Result<()> {
        let mut frame = Frame {
            label: Some(self.sig.routine.clone()),
            active: true,
            ..Frame::default()
        };
        for (i, p) in self.sig.parms.iter().enumerate() {
            let ty = host_type(&p.ty)?;
            self.vars.push(Var {
                place: format!("$p{}", i + 1),
                ind: format!("$n{}", i + 1),
                ty,
                sql: p.ty.clone(),
                constant: false,
                array: None,
            });
            frame.vars.push((p.name.clone(), i));
        }
        self.frames.push(frame);
        // A trigger's transition variables, qualified by their correlation names.
        for g in &self.sig.groups {
            for &(_, k) in &g.names {
                self.vars[k].constant |= g.read_only;
            }
            self.frames.push(Frame {
                label: Some(g.label.clone()),
                vars: g.names.clone(),
                active: true,
                ..Frame::default()
            });
        }
        self.ins("mov $sqlca, *ZEROS");
        self.ins("mov [%pca +131]:char(5), \"00000\"");
        self.ins("store $rc, 0");
        for (i, p) in self.sig.parms.iter().enumerate() {
            if p.mode == ParmMode::Out {
                self.ins(format!("store $n{}, -1", i + 1));
            }
        }
        self.stmt(body)?;
        let done = self.done.clone();
        self.place(&done);
        // A normal end: the return status, SQLSTATE 00000, or the warning
        // the last statement left.
        self.ins("str.subst $cls, [%pca +131]:char(5), 1, 2");
        self.ins("%wn = str.cmp.eq $cls, \"01\"");
        let (keep, clear) = (self.label(), self.label());
        self.term(format!("brif %wn, {keep}, {clear}"));
        self.place(&clear);
        self.ins("mov $sqlca, *ZEROS");
        self.ins("mov [%pca +131]:char(5), \"00000\"");
        self.place(&keep);
        self.ins("%rcv = load $rc");
        self.ins("store [%pca +96]:i32, %rcv");
        let out = self.label();
        self.term(format!("br {out}"));
        let fail = self.fail.clone();
        self.place(&fail);
        self.term(format!("br {out}"));
        self.place(&out);
        let all: Vec<usize> = (0..self.vars.len()).collect();
        self.free_arrays(&all);
        self.ins("mov $state, [%pca +131]:char(5)");
        self.ins("%np = parms");
        let n = 2 * self.sig.parms.len() + 5;
        self.ins(format!("%hasca = cmp.ge %np, {n}"));
        let (yes, end) = (self.label(), self.label());
        self.term(format!("brif %hasca, {yes}, {end}"));
        self.place(&yes);
        self.ins("mov $ca, $sqlca");
        self.place(&end);
        self.term("ret");
        Ok(())
    }

    fn stmts(&mut self, v: &[PlStmt]) -> Result<()> {
        for s in v {
            self.stmt(s)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_lines, clippy::many_single_char_names)] // one arm per statement
    fn stmt(&mut self, s: &PlStmt) -> Result<()> {
        if let Some(l) = &s.label {
            let q = self
                .gotos
                .get(&l.value)
                .cloned()
                .unwrap_or_else(|| self.label());
            self.place(&q);
        }
        let label = s.label.as_ref().map(|l| l.value.clone());
        match &s.kind {
            PlKind::Compound(c) => self.compound(c, label)?,
            PlKind::Sql(q) => self.sql_stmt(q)?,
            PlKind::Set(assigns) => {
                for a in assigns {
                    for t in &a.targets {
                        let k = self.var(&t.value)?;
                        if self.vars[k].constant && self.sig.role == Role::Trigger {
                            // A transition variable the trigger may not set.
                            let col = t.value.rsplit('.').next().unwrap_or(&t.value);
                            return Err(undefined(col));
                        }
                        if self.vars[k].constant {
                            return Err(SqlError::Coded {
                                code: -20_376,
                                state: "428GE",
                                what: format!("{} is a constant", t.value),
                            });
                        }
                    }
                    match &a.source {
                        AssignSource::Exprs(es) if a.element.is_some() => {
                            let k = self.var(&a.targets[0].value)?;
                            self.assign_element(
                                k,
                                a.element.as_ref().expect("an element"),
                                &es[0],
                            )?;
                        }
                        AssignSource::Exprs(es) => {
                            let ks: Vec<usize> = a
                                .targets
                                .iter()
                                .map(|t| self.var(&t.value))
                                .collect::<Result<_>>()?;
                            self.assign(&ks, es)?;
                        }
                        AssignSource::Row(q) => {
                            let ks: Vec<usize> = a
                                .targets
                                .iter()
                                .map(|t| self.var(&t.value))
                                .collect::<Result<_>>()?;
                            let text = format!("VALUES ({})", q.text);
                            self.run(
                                Op::Query,
                                &text,
                                &[],
                                &ks,
                                Flags {
                                    quiet: false,
                                    ..Flags::default()
                                },
                            )?;
                        }
                    }
                }
            }
            PlKind::If {
                branches,
                otherwise,
            } => {
                let conds: Vec<&PlExpr> = branches.iter().map(|(c, _)| c).collect();
                let v = self.choose(None, &conds)?;
                let end = self.label();
                let blocks: Vec<String> = branches.iter().map(|_| self.label()).collect();
                let other = self.label();
                let cases: Vec<String> = blocks
                    .iter()
                    .enumerate()
                    .map(|(i, b)| format!("{}: {b}", i + 1))
                    .collect();
                self.term(format!(
                    "switch {v}, [{}], default {other}",
                    cases.join(", ")
                ));
                for ((_, body), b) in branches.iter().zip(&blocks) {
                    self.place(b);
                    self.with_target(label.clone(), &end, None, |g| g.stmts(body))?;
                    self.term(format!("br {end}"));
                }
                self.place(&other);
                if let Some(o) = otherwise {
                    self.with_target(label.clone(), &end, None, |g| g.stmts(o))?;
                }
                self.place(&end);
            }
            PlKind::Case {
                operand,
                whens,
                otherwise,
            } => {
                let conds: Vec<&PlExpr> = whens.iter().map(|(c, _)| c).collect();
                let v = self.choose(operand.as_ref(), &conds)?;
                let end = self.label();
                let blocks: Vec<String> = whens.iter().map(|_| self.label()).collect();
                let other = self.label();
                let cases: Vec<String> = blocks
                    .iter()
                    .enumerate()
                    .map(|(i, b)| format!("{}: {b}", i + 1))
                    .collect();
                self.term(format!(
                    "switch {v}, [{}], default {other}",
                    cases.join(", ")
                ));
                for ((_, body), b) in whens.iter().zip(&blocks) {
                    self.place(b);
                    self.with_target(label.clone(), &end, None, |g| g.stmts(body))?;
                    self.term(format!("br {end}"));
                }
                self.place(&other);
                match otherwise {
                    Some(o) => self.with_target(label.clone(), &end, None, |g| g.stmts(o))?,
                    // No branch and no ELSE: the case is not found (SQL0773).
                    None => self.raise(-773, "20000"),
                }
                self.place(&end);
            }
            PlKind::Loop(body) => {
                let (top, end) = (self.label(), self.label());
                self.place(&top);
                self.with_target(label, &end, Some(&top), |g| g.stmts(body))?;
                self.term(format!("br {top}"));
                self.place(&end);
            }
            PlKind::While(c, body) => {
                let (top, go, end) = (self.label(), self.label(), self.label());
                self.place(&top);
                let v = self.choose(None, &[c])?;
                self.ins(format!("%w = cmp.eq {v}, 1"));
                self.term(format!("brif %w, {go}, {end}"));
                self.place(&go);
                self.with_target(label, &end, Some(&top), |g| g.stmts(body))?;
                self.term(format!("br {top}"));
                self.place(&end);
            }
            PlKind::Repeat(body, c) => {
                let (top, end) = (self.label(), self.label());
                self.place(&top);
                self.with_target(label, &end, Some(&top), |g| g.stmts(body))?;
                let v = self.choose(None, &[c])?;
                self.ins(format!("%w = cmp.eq {v}, 1"));
                self.term(format!("brif %w, {end}, {top}"));
                self.place(&end);
            }
            PlKind::For {
                var,
                cursor,
                query,
                body,
            } => self.for_loop(var, cursor.as_ref(), query, body, label)?,
            PlKind::Leave(l) => {
                let t = self
                    .targets
                    .iter()
                    .rev()
                    .find(|t| t.label == l.value)
                    .ok_or_else(|| no_label(&l.value))?;
                let (exit, depth) = (t.exit.clone(), t.depth);
                self.release_from(depth);
                self.term(format!("br {exit}"));
            }
            PlKind::Iterate(l) => {
                let t = self
                    .targets
                    .iter()
                    .rev()
                    .find(|t| t.label == l.value)
                    .ok_or_else(|| no_label(&l.value))?;
                let Some(it) = t.iterate.clone() else {
                    return Err(SqlError::Coded {
                        code: -778,
                        state: "428D5",
                        what: format!("{} does not label a loop", l.value),
                    });
                };
                let depth = t.depth;
                self.release_from(depth);
                self.term(format!("br {it}"));
            }
            PlKind::Goto(l) => {
                let q = self
                    .gotos
                    .get(&l.value)
                    .cloned()
                    .ok_or_else(|| no_label(&l.value))?;
                self.term(format!("br {q}"));
            }
            PlKind::Return(e) => {
                if let Some(e) = e {
                    if self.sig.role != Role::Procedure {
                        return Err(unsupported("RETURN with a value outside a procedure"));
                    }
                    let t = self.auto("i32");
                    let ti = self.auto("i16");
                    let r = self.rewrite(
                        &format!("VALUES (CAST(({}) AS INTEGER))", e.text),
                        &HashSet::new(),
                    )?;
                    let k = self.temp_var(&t, &ti, "INTEGER")?;
                    self.run_vars(
                        Op::Query,
                        &r.text,
                        &r.vars,
                        &[k],
                        Flags {
                            quiet: true,
                            ..Flags::default()
                        },
                    )?;
                    self.ins(format!("%rv = load {t}"));
                    self.ins("store $rc, %rv");
                }
                self.release_from(0);
                let d = self.done.clone();
                self.term(format!("br {d}"));
            }
            PlKind::ReturnQuery(_) => {
                return Err(unsupported("RETURN of a query outside a table function"));
            }
            PlKind::Call { name, args } => {
                let mut texts = Vec::new();
                for a in args {
                    texts.push(a.text.clone());
                }
                let text = format!("CALL {name}({})", texts.join(", "));
                let text = if args.is_empty() {
                    format!("CALL {name}")
                } else {
                    text
                };
                self.uncast = true;
                let r = self.rewrite(&text, &HashSet::new());
                self.uncast = false;
                let r = r?;
                if r.vars.iter().any(|&k| self.vars[k].array.is_some()) {
                    // BLOCKED(rel): a CALL passes values, and an array
                    // lives in the database behind its handle.
                    return Err(unsupported("an array argument in CALL"));
                }
                self.run_vars(Op::Exec, &r.text, &r.vars, &[], Flags::default())?;
            }
            PlKind::Signal(sg) => self.signal(sg)?,
            PlKind::GetDiag(d) => self.get_diag(d)?,
            PlKind::Open { cursor, using } => self.open_cursor(cursor, using)?,
            PlKind::Fetch {
                cursor,
                orient,
                into,
            } => {
                self.cursor(&cursor.value)?;
                let ks: Vec<usize> = into
                    .iter()
                    .map(|t| self.var(&t.value))
                    .collect::<Result<_>>()?;
                let d = Desc {
                    op: Op::Fetch,
                    name: cursor.value.clone(),
                    orient: orient.trim().to_string(),
                    ..self.desc()
                };
                self.run_desc(&d, &[], &ks)?;
            }
            PlKind::Close(c) => {
                self.cursor(&c.value)?;
                let d = Desc {
                    op: Op::Close,
                    name: c.value.clone(),
                    ..self.desc()
                };
                self.run_desc(&d, &[], &[])?;
            }
            PlKind::Prepare { stmt, from } => {
                let k = self.text_value(from)?;
                let d = Desc {
                    op: Op::Prepare,
                    name: stmt.value.clone(),
                    ..self.desc()
                };
                self.run_desc(&d, &[k], &[])?;
            }
            PlKind::Execute { stmt, into, using } => {
                if !into.is_empty() {
                    return Err(unsupported("EXECUTE ... INTO in a routine"));
                }
                let ks = self.plain_vars(using, "EXECUTE ... USING")?;
                let d = Desc {
                    op: Op::Execute,
                    name: stmt.value.clone(),
                    ..self.desc()
                };
                self.run_desc(&d, &ks, &[])?;
            }
            PlKind::ExecImm(e) => {
                let k = self.text_value(e)?;
                let d = Desc {
                    op: Op::ExecImm,
                    ..self.desc()
                };
                self.run_desc(&d, &[k], &[])?;
            }
            PlKind::SetResultSets(cs) => {
                let names: Vec<String> = cs.iter().map(|c| format!("CURSOR {}", c.value)).collect();
                let text = if names.is_empty() {
                    "SET RESULT SETS NONE".to_string()
                } else {
                    format!("SET RESULT SETS {}", names.join(", "))
                };
                self.run(Op::Exec, &text, &[], &[], Flags::default())?;
            }
        }
        Ok(())
    }

    /// Run `f` with a statement `LEAVE` (and, for a loop, `ITERATE`)
    /// can reach by its label.
    fn with_target(
        &mut self,
        label: Option<String>,
        exit: &str,
        iterate: Option<&str>,
        f: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        let pushed = label.is_some();
        if let Some(l) = label {
            let depth = self.frames.len();
            self.targets.push(Target {
                label: l,
                exit: exit.to_string(),
                iterate: iterate.map(str::to_string),
                depth,
            });
        }
        let r = f(self);
        if pushed {
            self.targets.pop();
        }
        r
    }

    fn desc(&self) -> Desc {
        Desc {
            sql_naming: self.sig.naming == crate::ast::Naming::Sql,
            commit: self.sig.commit,
            atomic: self.sig.commit.is_none() && self.frames.iter().any(|f| f.atomic.is_some()),
            arrays: self.sig.role == Role::Procedure,
            ..Desc::default()
        }
    }

    /// A statement of the compiler's own (a savepoint of an `ATOMIC`
    /// compound): it leaves the SQLCA and the diagnostics as they were.
    fn internal(&mut self, text: &str) {
        let d = Desc {
            op: Op::Exec,
            text: text.to_string(),
            quiet: true,
            atomic: self.sig.commit.is_none(),
            ..self.desc()
        };
        let (t, len) = self
            .text_const(&d.encode())
            .expect("savepoint statements are CCSID 37");
        self.ins("mov $uca, $sqlca");
        self.ins(format!(
            "callb _QRNX_SQLRUN($sqlca, {t}, {len}, $scope, {})",
            self.sig.program.len()
        ));
        self.ins("mov $sqlca, $uca");
    }

    /// The outermost `ATOMIC` compound among the frames from `depth` up:
    /// its savepoint released, and with it those of the compounds in it.
    fn release_from(&mut self, depth: usize) {
        if let Some(sp) = self
            .frames
            .get(depth..)
            .and_then(|fs| fs.iter().find_map(|f| f.atomic.clone()))
        {
            self.internal(&format!("RELEASE SAVEPOINT {sp}"));
        }
    }

    /// An exception leaves the routine: every `ATOMIC` compound it is
    /// in undone.
    fn undo_all(&mut self) {
        if let Some(sp) = self.frames.iter().find_map(|f| f.atomic.clone()) {
            self.internal(&format!("ROLLBACK TO SAVEPOINT {sp}"));
            self.internal(&format!("RELEASE SAVEPOINT {sp}"));
        }
    }

    fn var(&self, name: &str) -> Result<usize> {
        let found = match name.split_once('.') {
            Some((label, v)) => self.qualified(label, v),
            None => self.variable(name),
        };
        found.ok_or_else(|| undefined(name))
    }

    fn cursor(&self, name: &str) -> Result<Cur> {
        self.frames
            .iter()
            .rev()
            .find_map(|f| f.cursors.get(name).cloned())
            .ok_or_else(|| SqlError::Coded {
                code: -504,
                state: "34000",
                what: format!("cursor {name} is not declared"),
            })
    }

    /// A variable in the routine's storage.
    fn new_var(&mut self, sql: &TypeSpec, constant: bool) -> Result<usize> {
        let ty = host_type(sql)?;
        let place = self.auto(&ty.qir);
        let ind = self.auto("i16");
        self.vars.push(Var {
            place,
            ind,
            ty,
            sql: sql.clone(),
            constant,
            array: None,
        });
        Ok(self.vars.len() - 1)
    }

    /// A variable over existing storage, of a type given as text.
    fn temp_var(&mut self, place: &str, ind: &str, ty: &str) -> Result<usize> {
        let sql = super::types::parse_type(ty)?;
        let ty = host_type(&sql)?;
        self.vars.push(Var {
            place: place.into(),
            ind: ind.into(),
            ty,
            sql,
            constant: false,
            array: None,
        });
        Ok(self.vars.len() - 1)
    }

    /// Rewrite statement text with the variables in scope as markers.
    fn rewrite<S: std::hash::BuildHasher>(
        &self,
        text: &str,
        columns: &HashSet<String, S>,
    ) -> Result<rewrite::Rewritten> {
        let text = self.max_cardinality(text)?;
        rewrite::rewrite(&text, self, columns)
    }

    /// The column names of the tables a statement names.
    fn columns_of(&mut self, st: &Statement) -> HashSet<String> {
        let mut out = HashSet::new();
        for t in crate::embed::tables(st) {
            out.extend(self.cat.columns(&t));
        }
        out
    }

    /// An SQL statement of the body.
    fn sql_stmt(&mut self, q: &PlSql) -> Result<()> {
        if matches!(q.stmt, Statement::CreateRoutine(_)) {
            return Err(unsupported("CREATE PROCEDURE or FUNCTION inside a routine"));
        }
        if matches!(
            q.stmt,
            Statement::Commit { .. } | Statement::Rollback { .. }
        ) && self.frames.iter().any(|f| f.atomic.is_some())
        {
            let what = crate::exec::statement_name(&q.stmt);
            return Err(SqlError::Coded {
                code: -775,
                state: "42910",
                what: format!("{what} in an ATOMIC compound statement"),
            });
        }
        let r = match &q.stmt {
            Statement::Insert(i) => self.insert_text(&q.text, i)?,
            st => {
                let cols = self.columns_of(st);
                self.rewrite(&q.text, &cols)?
            }
        };
        let outs: Vec<usize> = q
            .into
            .iter()
            .map(|t| self.var(&t.value))
            .collect::<Result<_>>()?;
        let op = if outs.is_empty() { Op::Exec } else { Op::Query };
        if op == Op::Exec && matches!(q.stmt, Statement::Select(_)) {
            return Err(SqlError::Coded {
                code: -84,
                state: "42612",
                what: "a query in a routine needs INTO, a cursor or a FOR loop".into(),
            });
        }
        self.run_vars(op, &r.text, &r.vars, &outs, Flags::default())
    }

    /// An `INSERT`: its column list names the table's columns, but its
    /// `VALUES` see none (a name there is a variable); a query source
    /// sees its own tables'.
    fn insert_text(&mut self, text: &str, ins: &crate::ast::Insert) -> Result<rewrite::Rewritten> {
        use crate::lex::Tok;
        let toks: Vec<Tok> = crate::lex::tokenize(text)?
            .into_iter()
            .map(|t| t.tok)
            .collect();
        let mut depth = 0i32;
        let split = toks.iter().position(|t| {
            match t {
                Tok::Sym("(") => depth += 1,
                Tok::Sym(")") => depth -= 1,
                _ => {}
            }
            depth == 0 && matches!(t, Tok::Word(w) if w == "VALUES" || w == "SELECT" || w == "WITH")
        });
        let Some(at) = split else {
            let cols = self.columns_of(&Statement::Insert(ins.clone()));
            return self.rewrite(text, &cols);
        };
        let render = |ts: &[Tok]| {
            let mut s = String::new();
            for t in ts {
                if !s.is_empty() {
                    s.push(' ');
                }
                rewrite::render(t, &mut s);
            }
            s
        };
        let target: HashSet<String> = self.cat.columns(&ins.table).into_iter().collect();
        let head = self.rewrite(&render(&toks[..at]), &target)?;
        let source: HashSet<String> = match &ins.source {
            crate::ast::InsertSource::Query(q) => {
                self.columns_of(&Statement::Select((**q).clone()))
            }
            _ => HashSet::new(),
        };
        let tail = self.rewrite(&render(&toks[at..]), &source)?;
        let mut vars = head.vars;
        vars.extend(tail.vars);
        Ok(rewrite::Rewritten {
            text: format!("{} {}", head.text, tail.text),
            vars,
        })
    }

    /// `targets = exprs`, one statement for all of them.
    fn assign(&mut self, ks: &[usize], es: &[PlExpr]) -> Result<()> {
        let mut parts = Vec::new();
        let mut outs = Vec::new();
        for (k, e) in ks.iter().zip(es) {
            if self.vars[*k].array.is_some() {
                self.assign_array(*k, e)?;
            } else if let Expr::Literal(Literal::Null) = &e.expr {
                let ind = self.vars[*k].ind.clone();
                self.ins(format!("store {ind}, -1"));
            } else {
                parts.push(format!(
                    "CAST(({}) AS {})",
                    e.text,
                    super::types::type_text(&self.vars[*k].sql)
                ));
                outs.push(*k);
            }
        }
        if parts.is_empty() {
            return Ok(());
        }
        let r = self.rewrite(&format!("VALUES ({})", parts.join(", ")), &HashSet::new())?;
        self.run_vars(
            Op::Query,
            &r.text,
            &r.vars,
            &outs,
            Flags {
                quiet: true,
                ..Flags::default()
            },
        )
    }

    /// Evaluate which of `conds` holds first (1, 2, ...; 0 for none), or
    /// with an operand which value it equals: an `i32` register.
    fn choose(&mut self, operand: Option<&PlExpr>, conds: &[&PlExpr]) -> Result<String> {
        let mut text = String::from("VALUES CASE");
        if let Some(o) = operand {
            let _ = write!(text, " ({})", o.text);
        }
        for (i, c) in conds.iter().enumerate() {
            let _ = write!(text, " WHEN ({}) THEN {}", c.text, i + 1);
        }
        text.push_str(" ELSE 0 END");
        let r = self.rewrite(&text, &HashSet::new())?;
        let t = self.auto("i32");
        let ti = self.auto("i16");
        let k = self.temp_var(&t, &ti, "INTEGER")?;
        self.run_vars(
            Op::Query,
            &r.text,
            &r.vars,
            &[k],
            Flags {
                quiet: true,
                ..Flags::default()
            },
        )?;
        self.temps += 1;
        let reg = format!("%c{}", self.temps);
        self.ins(format!("{reg} = load {t}"));
        Ok(reg)
    }

    /// A value given as a text expression (`PREPARE ... FROM`,
    /// `EXECUTE IMMEDIATE`): the variable itself, or a temporary.
    fn text_value(&mut self, e: &PlExpr) -> Result<usize> {
        if let Expr::Column(c) = &e.expr
            && c.table.is_none()
            && let Some(k) = self.variable(&c.column.value)
        {
            return Ok(k);
        }
        let t = self.auto("varchar(32740)");
        let ti = self.auto("i16");
        let k = self.temp_var(&t, &ti, "VARCHAR(32740)")?;
        let r = self.rewrite(
            &format!("VALUES (CAST(({}) AS VARCHAR(32740)))", e.text),
            &HashSet::new(),
        )?;
        self.run_vars(
            Op::Query,
            &r.text,
            &r.vars,
            &[k],
            Flags {
                quiet: true,
                ..Flags::default()
            },
        )?;
        Ok(k)
    }

    /// Expressions that must each be a variable.
    fn plain_vars(&self, es: &[PlExpr], what: &str) -> Result<Vec<usize>> {
        es.iter()
            .map(|e| match &e.expr {
                Expr::Column(c) if c.table.is_none() => self.var(&c.column.value),
                _ => Err(unsupported(&format!(
                    "an expression in {what}: name a variable"
                ))),
            })
            .collect()
    }

    /// Run a statement whose text needs no rewriting.
    fn run(&mut self, op: Op, text: &str, ins: &[usize], outs: &[usize], f: Flags) -> Result<()> {
        self.run_vars(op, text, ins, outs, f)
    }

    fn run_vars(
        &mut self,
        op: Op,
        text: &str,
        ins: &[usize],
        outs: &[usize],
        f: Flags,
    ) -> Result<()> {
        let d = Desc {
            op,
            text: text.to_string(),
            quiet: f.quiet,
            ..self.desc()
        };
        self.run_desc(&d, ins, outs)
    }

    /// The calls for one statement, then the look at its outcome.
    fn run_desc(&mut self, d: &Desc, ins: &[usize], outs: &[usize]) -> Result<()> {
        let (ins, d) = self.array_io(d, ins, outs);
        self.emit(&d, &ins, outs)
    }

    /// The calls of [`Self::run_desc`] as they are.
    fn emit(&mut self, d: &Desc, ins: &[usize], outs: &[usize]) -> Result<()> {
        for &k in ins {
            let v = self.vars[k].clone();
            self.ins(format!(
                "callb _QRNX_SQLIN({}, {}, {}, {}, 1)",
                v.place, v.ty.desc, v.ty.extra, v.ind
            ));
        }
        for &k in outs {
            let v = self.vars[k].clone();
            self.ins(format!(
                "callb _QRNX_SQLOUT({}, {}, {}, {}, 1, 0)",
                v.place, v.ty.desc, v.ty.extra, v.ind
            ));
        }
        let (t, len) = self.text_const(&d.encode())?;
        self.ins(format!(
            "callb _QRNX_SQLRUN($sqlca, {t}, {len}, $scope, {})",
            self.sig.program.len()
        ));
        self.after();
        Ok(())
    }

    /// After a statement: the special variables, then the condition.
    fn after(&mut self) {
        if let Some(k) = self.sqlstate_var {
            let p = self.vars[k].place.clone();
            let i = self.vars[k].ind.clone();
            self.ins(format!("mov {p}, [%pca +131]:char(5)"));
            self.ins(format!("store {i}, 0"));
        }
        if let Some(k) = self.sqlcode_var {
            let p = self.vars[k].place.clone();
            let i = self.vars[k].ind.clone();
            self.ins("%sc = load [%pca +12]:i32");
            self.ins(format!("store {p}, %sc"));
            self.ins(format!("store {i}, 0"));
        }
        self.ins("str.subst $cls, [%pca +131]:char(5), 1, 2");
        self.ins("%ok = str.cmp.eq $cls, \"00\"");
        let (cont, disp) = (self.label(), self.label());
        self.term(format!("brif %ok, {cont}, {disp}"));
        self.place(&disp);
        self.dispatch(&cont);
        self.place(&cont);
    }

    /// A condition is in the SQLCA: the handler for it, else on (a
    /// warning, no data) or out of the routine (an exception).
    fn dispatch(&mut self, cont: &str) {
        self.stmts += 1;
        let k = self.stmts;
        self.ins("%cw = str.cmp.eq $cls, \"01\"");
        self.ins("%cn = str.cmp.eq $cls, \"02\"");
        // Which handlers, innermost first; specific states before classes.
        let mut tests: Vec<(usize, usize, HandlerCond)> = Vec::new();
        for (fi, f) in self.frames.iter().enumerate().rev() {
            if !f.active {
                continue;
            }
            let mut specific = Vec::new();
            let mut general = Vec::new();
            for (hi, h) in f.handlers.iter().enumerate() {
                for c in &h.conds {
                    let c = match c {
                        HandlerCond::Name(n) => match self.condition(&n.value) {
                            Some(s) => HandlerCond::State(s),
                            None => continue,
                        },
                        other => other.clone(),
                    };
                    match c {
                        HandlerCond::State(_) => specific.push((fi, hi, c)),
                        _ => general.push((fi, hi, c)),
                    }
                }
            }
            tests.extend(specific);
            tests.extend(general);
        }
        for (fi, hi, c) in tests {
            let (go, next) = (self.label(), self.label());
            match &c {
                HandlerCond::State(s) => {
                    self.ins(format!("%hs = str.cmp.eq [%pca +131]:char(5), \"{s}\""));
                    self.term(format!("brif %hs, {go}, {next}"));
                }
                HandlerCond::Warning => self.term(format!("brif %cw, {go}, {next}")),
                HandlerCond::NotFound => self.term(format!("brif %cn, {go}, {next}")),
                HandlerCond::Exception => {
                    // Neither a warning nor no data.
                    let e1 = self.label();
                    self.term(format!("brif %cw, {next}, {e1}"));
                    self.place(&e1);
                    self.term(format!("brif %cn, {next}, {go}"));
                }
                HandlerCond::Name(_) => continue,
            }
            self.place(&go);
            self.ins(format!("store $resume, {k}"));
            self.ins("mov $hca, $sqlca");
            match self.frames[fi].handlers[hi].kind {
                HandlerKind::Continue => {}
                HandlerKind::Exit => self.release_from(fi + 1),
                HandlerKind::Undo => {
                    let sp = self.frames[fi]
                        .atomic
                        .clone()
                        .expect("UNDO in an ATOMIC compound");
                    self.internal(&format!("ROLLBACK TO SAVEPOINT {sp}"));
                }
            }
            let entry = self.frames[fi].handlers[hi].entry.clone();
            let rs = &mut self.frames[fi].handlers[hi].resumes;
            if !rs.iter().any(|(r, _)| *r == k) {
                rs.push((k, cont.to_string()));
            }
            self.term(format!("br {entry}"));
            self.place(&next);
        }
        // No handler: a warning or no data goes on; an exception ends the
        // routine, the ATOMIC compounds it is in undone.
        let fail = self.fail.clone();
        let (f1, f2) = (self.label(), self.label());
        self.term(format!("brif %cw, {cont}, {f1}"));
        self.place(&f1);
        self.term(format!("brif %cn, {cont}, {f2}"));
        self.place(&f2);
        self.undo_all();
        self.term(format!("br {fail}"));
    }

    /// A declared condition's SQLSTATE.
    fn condition(&self, name: &str) -> Option<String> {
        self.frames
            .iter()
            .rev()
            .find_map(|f| f.conds.get(name).cloned())
    }

    /// Put a condition into the SQLCA here and dispatch it.
    fn raise(&mut self, code: i32, state: &str) {
        self.ins(format!("store [%pca +12]:i32, {code}"));
        self.ins(format!("mov [%pca +131]:char(5), \"{state}\""));
        self.ins("str.subst $cls, [%pca +131]:char(5), 1, 2");
        let cont = self.label();
        self.dispatch(&cont);
        self.place(&cont);
    }

    /// `SIGNAL` and `RESIGNAL`: the database raises the condition, so
    /// its diagnostics are the condition's.
    fn signal(&mut self, s: &Signal) -> Result<()> {
        if s.resignal && s.state.is_none() {
            // The handler's condition again, for the handlers outside it.
            self.ins("mov $sqlca, $hca");
            if let Some(m) = &s.message {
                let t = self.auto("varchar(70)");
                let ti = self.auto("i16");
                let k = self.temp_var(&t, &ti, "VARCHAR(70)")?;
                let r = self.rewrite(
                    &format!("VALUES (CAST(({}) AS VARCHAR(70)))", m.text),
                    &HashSet::new(),
                )?;
                self.ins("mov $hca, $sqlca");
                self.run_vars(
                    Op::Query,
                    &r.text,
                    &r.vars,
                    &[k],
                    Flags {
                        quiet: true,
                        ..Flags::default()
                    },
                )?;
                self.ins("mov $sqlca, $hca");
                self.ins(format!("%ml = str.len {t}"));
                self.ins("%ml2 = cvt.i16 %ml");
                self.ins("store [%pca +16]:i16, %ml2");
                self.ins(format!("mov [%pca +18]:char(70), {t}"));
            }
            self.ins("str.subst $cls, [%pca +131]:char(5), 1, 2");
            let cont = self.label();
            self.dispatch(&cont);
            self.place(&cont);
            return Ok(());
        }
        let state = match &s.state {
            Some(SignalValue::State(st)) => format!("'{st}'"),
            Some(SignalValue::Condition(c)) => {
                let st = self.condition(&c.value).ok_or_else(|| SqlError::Coded {
                    code: -312,
                    state: "42618",
                    what: format!("condition {} is not declared", c.value),
                })?;
                format!("'{st}'")
            }
            Some(SignalValue::Variable(e)) => e.text.clone(),
            None => unreachable!(),
        };
        let mut text = format!("SIGNAL SQLSTATE {state}");
        if let Some(m) = &s.message {
            let _ = write!(text, " SET MESSAGE_TEXT = {}", m.text);
        }
        let r = self.rewrite(&text, &HashSet::new())?;
        self.run_vars(Op::Exec, &r.text, &r.vars, &[], Flags::default())
    }

    /// `GET DIAGNOSTICS`: the items as the text, the targets as outputs.
    fn get_diag(&mut self, d: &GetDiag) -> Result<()> {
        let items: Vec<String> = d.items.iter().map(|(_, i)| i.clone()).collect();
        let ks: Vec<usize> = d
            .items
            .iter()
            .map(|(t, _)| self.var(&t.value))
            .collect::<Result<_>>()?;
        let text = if d.stacked {
            format!("*STACKED,{}", items.join(","))
        } else {
            items.join(",")
        };
        let desc = Desc {
            op: Op::Diag,
            text,
            ..self.desc()
        };
        // GET DIAGNOSTICS raises nothing: only the outputs are written.
        for &k in &ks {
            let v = self.vars[k].clone();
            self.ins(format!(
                "callb _QRNX_SQLOUT({}, {}, {}, {}, 1, 0)",
                v.place, v.ty.desc, v.ty.extra, v.ind
            ));
        }
        let (t, len) = self.text_const(&desc.encode())?;
        self.ins("mov $hca, $sqlca");
        self.ins(format!(
            "callb _QRNX_SQLRUN($sqlca, {t}, {len}, $scope, {})",
            self.sig.program.len()
        ));
        self.ins("mov $sqlca, $hca");
        Ok(())
    }

    /// A handler's entry (`*STACK`): the condition it took becomes the
    /// stacked one; its end (`*UNSTACK`): none is.
    fn stack_condition(&mut self, what: &str) -> Result<()> {
        let desc = Desc {
            op: Op::Diag,
            text: what.into(),
            ..self.desc()
        };
        let (t, len) = self.text_const(&desc.encode())?;
        self.ins("mov $uca, $sqlca");
        self.ins(format!(
            "callb _QRNX_SQLRUN($sqlca, {t}, {len}, $scope, {})",
            self.sig.program.len()
        ));
        self.ins("mov $sqlca, $uca");
        Ok(())
    }

    /// `OPEN`: the cursor's query, its variables' values now.
    fn open_cursor(&mut self, name: &Ident, using: &[PlExpr]) -> Result<()> {
        let c = self.cursor(&name.value)?.decl;
        let mut d = Desc {
            op: Op::Open,
            name: name.value.clone(),
            hold: c.hold,
            scroll: c.scroll,
            returns: c.returns,
            ..self.desc()
        };
        match &c.query {
            CursorSource::Query(q) => {
                if !using.is_empty() {
                    return Err(SqlError::Coded {
                        code: -313,
                        state: "07004",
                        what: "OPEN USING of a cursor with no markers".into(),
                    });
                }
                let cols = self.columns_of(&q.stmt);
                self.typed = c.returns.is_some();
                let r = self.rewrite(&q.text, &cols);
                self.typed = false;
                let r = r?;
                d.text = r.text;
                self.run_desc(&d, &r.vars, &[])
            }
            CursorSource::Statement(s) => {
                d.stmt.clone_from(&s.value);
                let ks = self.plain_vars(using, "OPEN ... USING")?;
                self.run_desc(&d, &ks, &[])
            }
        }
    }

    /// `FOR v AS c CURSOR FOR query DO body END FOR`.
    fn for_loop(
        &mut self,
        var: &Ident,
        cursor: Option<&Ident>,
        q: &PlSql,
        body: &[PlStmt],
        label: Option<String>,
    ) -> Result<()> {
        self.for_count += 1;
        let cname = cursor.map_or_else(
            || format!("SQL_FOR_{}", self.for_count),
            |c| c.value.clone(),
        );
        let cols = self.columns_of(&q.stmt);
        let r = self.rewrite(&q.text, &cols)?;
        let described = self.cat.describe(&r.text)?;
        // The loop's columns are variables of a frame of their own.
        let mut frame = Frame {
            label: Some(var.value.clone()),
            active: false,
            ..Frame::default()
        };
        let mut ks = Vec::new();
        for (n, t) in &described {
            let k = self.new_var(t, false)?;
            frame.vars.push((n.clone(), k));
            ks.push(k);
        }
        let open = Desc {
            op: Op::Open,
            name: cname.clone(),
            text: r.text.clone(),
            ..self.desc()
        };
        self.run_desc(&open, &r.vars, &[])?;
        self.frames.push(frame);
        let (top, end) = (self.label(), self.label());
        self.place(&top);
        for &k in &ks {
            let v = self.vars[k].clone();
            self.ins(format!(
                "callb _QRNX_SQLOUT({}, {}, {}, {}, 1, 0)",
                v.place, v.ty.desc, v.ty.extra, v.ind
            ));
        }
        let fetch = Desc {
            op: Op::Fetch,
            name: cname.clone(),
            ..self.desc()
        };
        let (t, len) = self.text_const(&fetch.encode())?;
        self.ins(format!(
            "callb _QRNX_SQLRUN($sqlca, {t}, {len}, $scope, {})",
            self.sig.program.len()
        ));
        self.ins("%fc = load [%pca +12]:i32");
        self.ins("%fe = cmp.eq %fc, 100");
        let more = self.label();
        self.term(format!("brif %fe, {end}, {more}"));
        self.place(&more);
        self.after();
        let r = self.with_target(label, &end, Some(&top), |g| g.stmts(body));
        self.frames.pop();
        r?;
        self.term(format!("br {top}"));
        self.place(&end);
        let close = Desc {
            op: Op::Close,
            name: cname,
            ..self.desc()
        };
        self.run_desc(&close, &[], &[])
    }

    /// `BEGIN ... END`.
    #[allow(clippy::too_many_lines)]
    fn compound(&mut self, c: &Compound, label: Option<String>) -> Result<()> {
        let end = self.label();
        let mut frame = Frame {
            label: label.clone(),
            active: true,
            ..Frame::default()
        };
        // An ATOMIC compound is a savepoint: released at its end, rolled
        // back to by an UNDO handler or an exception leaving the routine.
        if c.atomic {
            self.atomics += 1;
            let sp = format!("\"{}{}\"", crate::exec::ATOMIC_SAVEPOINT, self.atomics);
            self.internal(&format!("SAVEPOINT {sp} ON ROLLBACK RETAIN CURSORS"));
            frame.atomic = Some(sp);
        }
        for (n, s) in &c.conditions {
            frame.conds.insert(n.value.clone(), s.clone());
        }
        for cur in &c.cursors {
            if cur.hold && cur.returns.is_some() {
                // Both are fine to IBM; kept as written.
            }
            frame
                .cursors
                .insert(cur.name.value.clone(), Cur { decl: cur.clone() });
        }
        self.frames.push(frame);
        // Variables, in order: each default sees the ones before it.
        for v in &c.vars {
            let array = match &v.array {
                Some(t) => Some(self.cat.array_type(t)?.ok_or_else(|| SqlError::Coded {
                    code: -204,
                    state: "42704",
                    what: format!("type {t} not found"),
                })?),
                None => None,
            };
            for n in &v.names {
                // The routine's own compound may not reuse a parameter's name.
                if self.frames.len() == 2 && self.sig.parms.iter().any(|p| p.name == n.value) {
                    return Err(SqlError::Coded {
                        code: -590,
                        state: "42734",
                        what: format!("name {} is not unique in the routine", n.value),
                    });
                }
                let k = self.new_var(&v.ty, v.constant)?;
                self.vars[k].array.clone_from(&array);
                match n.value.as_str() {
                    "SQLSTATE" => self.sqlstate_var = Some(k),
                    "SQLCODE" => self.sqlcode_var = Some(k),
                    _ => {}
                }
                let ind = self.vars[k].ind.clone();
                let place = self.vars[k].place.clone();
                let zero = if self.vars[k].ty.qir.starts_with("char")
                    || self.vars[k].ty.qir.contains("date")
                    || self.vars[k].ty.qir.contains("time")
                {
                    "*BLANKS"
                } else {
                    "*ZEROS"
                };
                self.ins(format!("mov {place}, {zero}"));
                self.ins(format!("store {ind}, -1"));
                if n.value == "SQLSTATE" {
                    self.ins(format!("mov {place}, \"00000\""));
                    self.ins(format!("store {ind}, 0"));
                } else if n.value == "SQLCODE" {
                    self.ins(format!("store {ind}, 0"));
                }
                if let Some(d) = &v.default {
                    self.assign(&[k], std::slice::from_ref(d))?;
                }
                self.frames
                    .last_mut()
                    .expect("frame")
                    .vars
                    .push((n.value.clone(), k));
            }
        }
        // Handlers: their entries now, their bodies after the statements.
        for h in &c.handlers {
            if h.kind == HandlerKind::Undo && !c.atomic {
                return Err(SqlError::Coded {
                    code: -780,
                    state: "428D6",
                    what: "an UNDO handler outside an ATOMIC compound statement".into(),
                });
            }
            let entry = self.label();
            self.frames.last_mut().expect("frame").handlers.push(Hand {
                kind: h.kind,
                conds: h.conds.clone(),
                entry,
                resumes: Vec::new(),
            });
        }
        let r = self.with_target(label, &end, None, |g| g.stmts(&c.body));
        r?;
        self.term(format!("br {end}"));
        // The handlers run with this compound's handlers out of effect.
        self.frames.last_mut().expect("frame").active = false;
        for (i, h) in c.handlers.iter().enumerate() {
            let entry = self.frames.last().expect("frame").handlers[i].entry.clone();
            self.place(&entry);
            self.stack_condition("*STACK")?;
            self.stmt(&h.body)?;
            self.stack_condition("*UNSTACK")?;
            match h.kind {
                HandlerKind::Continue => {
                    let resumes = self.frames.last().expect("frame").handlers[i]
                        .resumes
                        .clone();
                    if resumes.is_empty() {
                        self.term(format!("br {end}"));
                    } else {
                        self.ins("%rs = load $resume");
                        let cases: Vec<String> =
                            resumes.iter().map(|(k, l)| format!("{k}: {l}")).collect();
                        self.term(format!("switch %rs, [{}], default {end}", cases.join(", ")));
                    }
                }
                HandlerKind::Exit | HandlerKind::Undo => self.term(format!("br {end}")),
            }
        }
        let frame = self.frames.pop().expect("frame");
        let atomic = frame.atomic;
        self.place(&end);
        let mine: Vec<usize> = frame.vars.iter().map(|(_, k)| *k).collect();
        self.free_arrays(&mine);
        if let Some(sp) = atomic {
            self.internal(&format!("RELEASE SAVEPOINT {sp}"));
        }
        Ok(())
    }
}

/// Flags of a statement's descriptor.
#[derive(Clone, Copy, Default)]
struct Flags {
    quiet: bool,
    #[allow(dead_code)]
    returns: Option<ReturnTo>,
}

fn no_label(l: &str) -> SqlError {
    SqlError::Coded {
        code: -779,
        state: "42736",
        what: format!("label {l} is not found or not reachable here"),
    }
}

#[allow(dead_code)]
fn object(name: &ObjectName) -> String {
    name.to_string()
}
