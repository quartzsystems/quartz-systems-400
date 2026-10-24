// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The parser: the reader's lines become items (a fixed-form
//! specification with its continuation lines, a free-form statement,
//! a compile-time data section, an embedded SQL block), and the items
//! become the module. A statement that does not parse is reported and
//! skipped; the parse goes on to the next item, so one compile reports
//! every error it can.

pub mod expr;
pub mod fixed;
pub mod free;
pub mod kw;
pub mod ops;

use crate::ast::{CtData, Def, DefKind, FileDecl, Module, Pos, Proc, Stmt};
use crate::diag::{Diag, ids};
use crate::lex::{self, Segment, Tok};
use crate::reader::{Class, Line, Read};

/// A fixed-form specification and its continuation lines.
#[derive(Clone, Debug)]
pub struct FixedSpec {
    /// The letter.
    pub form: char,
    /// Indexes into the reader's lines, the first one first.
    pub lines: Vec<usize>,
}

/// A free-form statement: its tokens without the semicolon.
#[derive(Clone, Debug)]
pub struct FreeStmt {
    /// The tokens.
    pub toks: Vec<Tok>,
    /// Where it starts.
    pub at: Pos,
    /// Where it ends.
    pub end: Pos,
}

/// One unit of parsing.
#[derive(Clone, Debug)]
pub enum Item {
    /// A fixed-form specification.
    Fixed(FixedSpec),
    /// A free-form statement.
    Free(FreeStmt),
    /// Compile-time data.
    Ct(CtData),
    /// An embedded SQL block of fixed form, as its lines.
    Sql(Vec<usize>),
}

/// The items of a read source.
pub fn items(read: &Read, diags: &mut Vec<Diag>) -> Vec<Item> {
    let lines = &read.lines;
    let mut out: Vec<Item> = Vec::new();
    let mut free: Vec<usize> = Vec::new();
    let mut pending_cond: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let l = &lines[i];
        match &l.class {
            Class::Free => {
                // Free form in fixed-format source ends at column 80.
                if !l.fully_free && !rpgspecs::cut(&l.text, 81, 100).trim().is_empty() {
                    diags.push(Diag::new(ids::FREE_COMMENT, 20, l.pos(81)));
                }
                free.push(i);
                i += 1;
                continue;
            }
            Class::Blank | Class::Comment | Class::Skipped | Class::FreeMarker | Class::Directive(_) => {
                i += 1;
                continue;
            }
            _ => {}
        }
        flush_free(lines, &mut free, &mut out, diags);
        match &l.class {
            Class::Spec(form) => {
                let form = *form;
                if form == 'C' && is_cond_only(&l.text) {
                    pending_cond.push(i);
                    i += 1;
                    continue;
                }
                let mut spec = FixedSpec {
                    form,
                    lines: std::mem::take(&mut pending_cond),
                };
                spec.lines.push(i);
                i += 1;
                // Continuation lines.
                let mut name_cont = matches!(form, 'D' | 'P') && is_name_continuation(&l.text);
                while i < lines.len() {
                    let n = &lines[i];
                    if matches!(n.class, Class::Blank | Class::Comment | Class::Skipped) {
                        i += 1;
                        continue;
                    }
                    if n.class != Class::Spec(form) {
                        break;
                    }
                    let cont = match form {
                        'F' => rpgspecs::cut(&n.text, 7, 43).trim().is_empty(),
                        'D' | 'P' => {
                            if name_cont {
                                name_cont = is_name_continuation(&n.text);
                                true
                            } else {
                                rpgspecs::cut(&n.text, 7, 43).trim().is_empty()
                            }
                        }
                        'C' => {
                            // The operation is on the item's first line that has one.
                            let op = spec
                                .lines
                                .iter()
                                .map(|&k| fixed::opcode_of(&lines[k].text))
                                .find(|o| !o.is_empty())
                                .unwrap_or_default();
                            ops::extended(&op) && rpgspecs::cut(&n.text, 7, 35).trim().is_empty()
                        }
                        // Field lines and AND/OR lines of a record; a record
                        // line may leave the file name blank, but has its
                        // sequence (I) or type (O).
                        'I' | 'O' => {
                            let rel = rpgspecs::cut(&n.text, 16, 18).trim().to_ascii_uppercase();
                            let tail = if form == 'I' { (17, 20) } else { (17, 17) };
                            rpgspecs::cut(&n.text, 7, 15).trim().is_empty()
                                && (rel == "AND"
                                    || rel == "OR"
                                    || rpgspecs::cut(&n.text, tail.0, tail.1).trim().is_empty()
                                        && rpgspecs::cut(&n.text, 16, 16).trim().is_empty())
                        }
                        _ => false,
                    };
                    if !cont {
                        break;
                    }
                    spec.lines.push(i);
                    i += 1;
                }
                out.push(Item::Fixed(spec));
            }
            Class::CtHeader(kind, name) => {
                let mut ct = CtData {
                    kind: kind.clone(),
                    name: name.clone(),
                    records: Vec::new(),
                    at: Pos { line: l.no, col: 1 },
                };
                i += 1;
                while i < lines.len() && lines[i].class == Class::CtData {
                    ct.records.push(lines[i].text.clone());
                    i += 1;
                }
                out.push(Item::Ct(ct));
            }
            Class::Sql => {
                let mut block = vec![i];
                i += 1;
                let opened = lines[block[0]].text.to_ascii_uppercase().contains("/EXEC SQL");
                if opened {
                    while i < lines.len() {
                        if lines[i].class == Class::Sql {
                            block.push(i);
                            let end = lines[i].text.to_ascii_uppercase().contains("/END-EXEC");
                            i += 1;
                            if end {
                                break;
                            }
                        } else if matches!(lines[i].class, Class::Blank | Class::Comment | Class::Skipped) {
                            i += 1;
                        } else {
                            break;
                        }
                    }
                }
                out.push(Item::Sql(block));
            }
            Class::CtData => i += 1,
            _ => i += 1,
        }
    }
    flush_free(lines, &mut free, &mut out, diags);
    out
}

/// A `C` line that holds only conditioning (no operation): it belongs
/// to the operation that follows.
fn is_cond_only(text: &str) -> bool {
    rpgspecs::cut(text, 12, 80).trim().is_empty() && !rpgspecs::cut(text, 7, 11).trim().is_empty()
}

/// A `D` or `P` line that holds only a name ending in `...`.
fn is_name_continuation(text: &str) -> bool {
    let t = rpgspecs::cut(text, 7, 80);
    let t = t.trim();
    t.ends_with("...") && !t.contains(' ')
}

/// The line of the semicolon that ended the last free-form statement.
fn last_semi_line(out: &[Item]) -> Option<u32> {
    match out.last() {
        Some(Item::Free(s)) => Some(s.end.line),
        _ => None,
    }
}

/// Tokenize a run of free-form lines and cut it at semicolons.
fn flush_free(lines: &[Line], run: &mut Vec<usize>, out: &mut Vec<Item>, diags: &mut Vec<Diag>) {
    if run.is_empty() {
        return;
    }
    let areas: Vec<(u32, u16, String)> = run
        .iter()
        .map(|&k| {
            let (col, text) = lines[k].free_area();
            (lines[k].no, col, text)
        })
        .collect();
    let segs: Vec<Segment<'_>> = areas
        .iter()
        .map(|(line, col, text)| Segment {
            line: *line,
            col: *col,
            text: text.as_str(),
        })
        .collect();
    let toks = lex::lex(&segs, diags);
    let last = areas.last().map_or(Pos::default(), |(l, c, t)| Pos {
        line: *l,
        col: *c + u16::try_from(t.len()).unwrap_or(0),
    });
    let mut cur: Vec<Tok> = Vec::new();
    let mut drop_line: Option<u32> = None;
    for t in toks {
        if let Some(l) = drop_line {
            if t.at.line == l {
                continue;
            }
            drop_line = None;
        }
        if t.kind != lex::Kind::Semi && cur.is_empty() && last_semi_line(out) == Some(t.at.line) {
            diags.push(Diag::new(ids::AFTER_STATEMENT, 30, t.at));
            drop_line = Some(t.at.line);
            continue;
        }
        if t.kind == lex::Kind::Semi {
            if !cur.is_empty() {
                let at = cur[0].at;
                out.push(Item::Free(FreeStmt {
                    toks: std::mem::take(&mut cur),
                    at,
                    end: t.at,
                }));
            }
            continue;
        }
        cur.push(t);
    }
    if !cur.is_empty() {
        let at = cur[0].at;
        let end = cur.last().map_or(last, |t| t.at);
        diags.push(Diag::new(ids::NO_SEMICOLON, 30, end));
        out.push(Item::Free(FreeStmt { toks: cur, at, end }));
    }
    run.clear();
}

/// Where a definition or statement goes while parsing.
pub struct Builder<'a> {
    /// The reader's lines.
    pub lines: &'a [Line],
    /// The module under construction.
    pub module: Module,
    /// The open procedure, if any.
    pub proc: Option<Proc>,
    /// The diagnostics.
    pub diags: Vec<Diag>,
    /// Whether a procedure began or ended: main-procedure statements
    /// are no longer allowed.
    pub after_proc: bool,
}

impl Builder<'_> {
    /// Add a file declaration where it belongs.
    pub fn file(&mut self, f: FileDecl) {
        match &mut self.proc {
            Some(p) => p.files.push(f),
            None => self.module.files.push(f),
        }
    }

    /// Add a definition where it belongs. A procedure interface goes
    /// to the procedure (or, globally, stands for the main procedure).
    pub fn def(&mut self, d: Def) {
        // One interface per procedure; a second is ignored.
        let twice = d.kind == DefKind::Pi
            && match &self.proc {
                Some(p) => p.pi.is_some(),
                None => self.module.defs.iter().any(|x| x.kind == DefKind::Pi),
            };
        if twice {
            self.diags.push(Diag::new(ids::PI_TWICE, 30, d.at));
            return;
        }
        match &mut self.proc {
            Some(p) if d.kind == DefKind::Pi => p.pi = Some(d),
            Some(p) => p.defs.push(d),
            None => self.module.defs.push(d),
        }
    }

    /// The definition list of the current scope, for subfields that
    /// follow a fixed-form `DS`, `PR` or `PI` line.
    pub fn last_group(&mut self) -> Option<&mut Def> {
        match &mut self.proc {
            Some(p) => {
                if p.defs.last().is_some_and(|d| matches!(d.kind, DefKind::Ds | DefKind::Pr)) {
                    p.defs.last_mut()
                } else {
                    p.pi.as_mut()
                }
            }
            None => self
                .module
                .defs
                .last_mut()
                .filter(|d| matches!(d.kind, DefKind::Ds | DefKind::Pr | DefKind::Pi)),
        }
    }

    /// Add a calculation where it belongs.
    pub fn stmt(&mut self, s: Stmt) {
        match &mut self.proc {
            Some(p) => p.calcs.push(s),
            // Between or after procedures: ignored.
            None if self.after_proc => self.diags.push(Diag::new(ids::BETWEEN_PROCS, 30, s.at)),
            None => self.module.calcs.push(s),
        }
    }

    /// Close the open procedure.
    pub fn end_proc(&mut self, at: Pos) {
        self.after_proc = true;
        if let Some(mut p) = self.proc.take() {
            p.end = at;
            self.module.procs.push(p);
        } else {
            self.diags.push(Diag::new(ids::PROC_END_UNMATCHED, 30, at));
        }
    }
}

/// Parse a read source into a module.
pub fn parse(read: &Read) -> (Module, Vec<Diag>) {
    let mut diags = Vec::new();
    let its = items(read, &mut diags);
    let mut b = Builder {
        lines: &read.lines,
        module: Module {
            fully_free: read.lines.first().is_some_and(|l| l.class == Class::FreeMarker),
            ..Module::default()
        },
        proc: None,
        diags,
        after_proc: false,
    };
    let mut k = 0;
    // The open fixed-form group (`DS`, `PR`, `PI`) takes the subfield
    // lines that follow it.
    let mut fixed_group = false;
    while k < its.len() {
        match &its[k] {
            Item::Fixed(spec) => {
                fixed_group = fixed::spec(&mut b, spec, fixed_group);
                k += 1;
            }
            Item::Free(st) => {
                fixed_group = false;
                k = free::statement(&mut b, &its, k, st);
            }
            Item::Ct(ct) => {
                b.module.ctdata.push(ct.clone());
                k += 1;
            }
            Item::Sql(lines) => {
                fixed_group = false;
                fixed::sql_block(&mut b, lines);
                k += 1;
            }
        }
    }
    if let Some(p) = &b.proc {
        let at = p.at;
        b.diags.push(Diag::new(ids::PROC_END_MISSING, 20, at));
        b.end_proc(at);
    }
    (b.module, b.diags)
}
