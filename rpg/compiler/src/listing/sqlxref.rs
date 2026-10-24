// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The precompile listing's cross-reference: the program's names as
//! the precompiler reads them (with their SQL types), the names its
//! statements use (columns, tables, cursors, statements, labels, host
//! variables) and the columns of each table a statement describes.
//! IBM's precompiler reads the cursor declarations first; an entry's
//! place among entries of one name is the order it was made in.

use rpgfront::ast::{Def, DefKind, Expr, Lit};
use rpgsqlpre::listing::XrefEntry;
use rpgsqlpre::scan::{self, Kind as TokKind, Tok};

use super::xref::ebcdic_key;
use crate::compile::Compiled;
use crate::files::{ExtFiles, ExtField};
use crate::syms::Kind;
use crate::types::Ty;

/// What an entry is, for finding it again.
#[derive(Clone, Copy, PartialEq, Eq)]
enum What {
    /// A program name: its scope.
    Rpg(usize),
    Column,
    Table,
    Cursor,
    Statement,
    Label,
    /// A host variable the program does not declare.
    Undefined,
    /// A column of a table a statement describes.
    Described,
}

struct Entry {
    name: String,
    define: Option<u32>,
    attr: String,
    refs: Vec<u32>,
    what: What,
    /// A column's qualifier; a subfield's structure.
    qual: String,
}

struct Xref<'a> {
    c: &'a Compiled,
    entries: Vec<Entry>,
    record_of: &'a dyn Fn(u32) -> u32,
    described: Vec<String>,
}

/// SQL words that are not names.
const KEYWORDS: &[&str] = &[
    "SELECT", "FROM", "WHERE", "ORDER", "BY", "GROUP", "HAVING", "INTO", "VALUES", "SET", "UPDATE", "DELETE", "INSERT", "AND",
    "OR", "NOT", "IN", "LIKE", "BETWEEN", "IS", "NULL", "AS", "DESC", "ASC", "JOIN", "ON", "INNER", "LEFT", "RIGHT", "OUTER",
    "FULL", "CROSS", "CASE", "WHEN", "THEN", "ELSE", "END", "DISTINCT", "FOR", "OF", "CURRENT", "ROWS", "ROW", "FETCH",
    "FIRST", "ONLY", "WITH", "HOLD", "CURSOR", "DECLARE", "SCROLL", "OPEN", "CLOSE", "PREPARE", "EXECUTE", "IMMEDIATE",
    "USING", "DESCRIPTOR", "DESCRIBE", "INPUT", "COMMIT", "ROLLBACK", "GET", "DIAGNOSTICS", "CONDITION", "NEXT", "PRIOR",
    "LAST", "RELATIVE", "BEFORE", "AFTER", "ABSOLUTE", "CONCAT", "ALL", "UNION", "EXCEPT", "INTERSECT", "EXISTS", "ANY",
    "SOME", "INDICATOR", "INT", "INTEGER", "SMALLINT", "BIGINT", "DECIMAL", "DEC", "NUMERIC", "CHAR", "CHARACTER",
    "VARCHAR", "DATE", "TIME", "TIMESTAMP", "FLOAT", "DOUBLE", "REAL", "WORK", "HOLD", "OPTIMIZE", "READ", "UPDATE",
    "DEFAULT", "TRUE", "FALSE", "LIMIT", "OFFSET", "WHENEVER", "SQLERROR", "SQLWARNING", "FOUND", "GO", "TO", "GOTO",
    "CONTINUE", "OPTION", "INCLUDE", "NAMES", "LABELS",
];

fn keyword(w: &str) -> bool {
    KEYWORDS.contains(&w)
}

/// A host variable's SQL type, as the precompiler describes it.
fn host_type(ty: &Ty) -> String {
    match ty {
        Ty::Int(3 | 5) => "SMALL INTEGER PRECISION(4,0)".into(),
        Ty::Int(20) => "BIGINT PRECISION(18,0)".into(),
        Ty::Int(_) => "INTEGER PRECISION(9,0)".into(),
        Ty::Uns(d) => format!("DECIMAL({d},0)"),
        Ty::Packed(d, s) => format!("DECIMAL({d},{s})"),
        Ty::Zoned(d, s) => format!("NUMERIC({d},{s})"),
        Ty::Bin(d, s) => format!("NUMERIC({d},{s})"),
        Ty::Float(4) => "FLOAT(24)".into(),
        Ty::Float(_) => "FLOAT(53)".into(),
        Ty::Char(n) => format!("CHARACTER({n})"),
        Ty::VarChar(n, _) => format!("VARCHAR({n})"),
        Ty::Date(..) => "DATE(8)".into(),
        Ty::Time(..) => "TIME(8)".into(),
        Ty::Timestamp(_) => "TIMESTAMP(26)".into(),
        _ => String::new(),
    }
}

/// A column's SQL type.
fn column_type(f: &ExtField) -> String {
    let t = match &f.ty {
        Ty::Int(5) => "SMALL INTEGER PRECISION(4,0)".into(),
        Ty::Int(20) => "BIGINT PRECISION(18,0)".into(),
        Ty::Int(_) => "INTEGER PRECISION(9,0)".into(),
        // An SQL integer column is binary to RPG.
        Ty::Bin(d, 0) if *d <= 4 => "SMALL INTEGER PRECISION(4,0)".into(),
        Ty::Bin(d, 0) if *d <= 9 => "INTEGER PRECISION(9,0)".into(),
        Ty::Bin(_, 0) => "BIGINT PRECISION(18,0)".into(),
        Ty::Packed(d, s) => format!("DECIMAL({d},{s})"),
        Ty::Zoned(d, s) => format!("NUMERIC({d},{s})"),
        Ty::Char(n) => format!("CHARACTER({n}) CCSID 37"),
        Ty::VarChar(n, _) => format!("VARCHAR({n}) CCSID 37"),
        Ty::Date(..) => "DATE(10)".into(),
        Ty::Time(..) => "TIME(8)".into(),
        Ty::Timestamp(_) => "TIMESTAMP(26)".into(),
        Ty::Float(4) => "FLOAT(24)".into(),
        Ty::Float(_) => "FLOAT(53)".into(),
        _ => String::new(),
    };
    let null = if f.nullable { "" } else { " (NOT NULL)" };
    format!("{t} COLUMN{null}")
}

/// A named constant's SQL type.
fn const_type(e: Option<&Expr>) -> String {
    match e {
        Some(Expr::Lit(Lit::Num(n), _)) => {
            let n = n.trim().trim_start_matches(['-', '+']);
            match n.split_once('.') {
                Some((i, f)) => format!("DECIMAL({},{})", i.len() + f.len(), f.len()),
                None if n.len() <= 4 => "SMALL INTEGER PRECISION(4,0)".into(),
                None if n.len() <= 9 => "INTEGER PRECISION(9,0)".into(),
                None => format!("DECIMAL({},0)", n.len()),
            }
        }
        Some(Expr::Lit(Lit::Char(s), _)) => format!("CHARACTER({})", s.chars().count()),
        _ => String::new(),
    }
}

impl Xref<'_> {
    fn add(&mut self, name: &str, define: Option<u32>, attr: String, what: What, qual: &str) -> usize {
        self.entries.push(Entry {
            name: name.to_ascii_uppercase(),
            define,
            attr,
            refs: Vec::new(),
            what,
            qual: qual.to_string(),
        });
        self.entries.len() - 1
    }

    fn rec(&self, line: u32) -> u32 {
        (self.record_of)(line)
    }

    /// The program's definitions in a scope; `proc` names the
    /// procedure they are in.
    fn defs(&mut self, defs: &[Def], scope: usize, proc: &str, consts: bool) {
        let where_ = if proc.is_empty() { String::new() } else { format!(" IN RPG PROCEDURE {proc}") };
        for d in defs {
            let line = self.rec(d.at.line);
            let name = d.name.to_ascii_uppercase();
            if name.is_empty() || name == "*N" {
                continue;
            }
            let sym = self.c.table.find(scope, &name).map(|id| &self.c.table.syms[id]);
            match d.kind {
                DefKind::Pr => {
                    self.add(&name, Some(line), String::new(), What::Rpg(scope), "");
                }
                DefKind::Const => {
                    let attr = format!("{} CONSTANT{where_}", const_type(d.value.as_ref()));
                    self.add(&name, Some(line), attr.trim_start().to_string(), What::Rpg(scope), "");
                }
                DefKind::Ds => {
                    let named: Vec<&Def> = d.children.iter().filter(|s| !s.name.is_empty() && s.name != "*N").collect();
                    let dim = sym.map_or(0, |s| s.dim);
                    let attr = if named.is_empty() {
                        let len = sym.map_or(0, |s| s.ty.size(&|i| self.c.table.ds_len(i)));
                        format!("CHARACTER({len})")
                    } else if dim > 0 {
                        format!("ARRAY({dim}) STRUCTURE")
                    } else {
                        "STRUCTURE".into()
                    };
                    self.add(&name, Some(line), format!("{attr}{where_}"), What::Rpg(scope), "");
                    let di = match sym.map(|s| &s.kind) {
                        Some(Some(Kind::Ds(di))) => Some(*di),
                        _ => None,
                    };
                    for s in named {
                        let sname = s.name.to_ascii_uppercase();
                        let ty = di.and_then(|di| self.c.table.subfield(di, &sname)).map(|id| self.c.table.syms[id].clone());
                        let attr = ty.as_ref().map_or_else(String::new, |t| array(t.dim, &host_type(&t.ty)));
                        self.add(&sname, Some(self.rec(s.at.line)), format!("{attr} IN {name}"), What::Rpg(scope), &name);
                    }
                }
                DefKind::Standalone | DefKind::Parm => {
                    let Some(s) = sym else { continue };
                    let ty = host_type(&s.ty);
                    if ty.is_empty() {
                        continue;
                    }
                    let constant = if consts && d.kw("CONST").is_some() { " CONSTANT" } else { "" };
                    let attr = format!("{}{constant}{where_}", array(s.dim, &ty));
                    self.add(&name, Some(line), attr, What::Rpg(scope), "");
                }
                _ => {}
            }
        }
    }

    /// The fields of externally described files, prefixed, defined
    /// where the file is declared.
    fn ext_fields(&mut self) {
        let c = self.c;
        for f in &c.files {
            let Some(ext) = &f.ext else { continue };
            let line = self.rec(f.def_line);
            for fm in &ext.formats {
                for x in &fm.fields {
                    let name = crate::extfiles::prefixed(&x.name, f.prefix.as_ref());
                    let scope = What::Rpg(f.scope);
                    if self.entries.iter().any(|e| e.name == name && e.what == scope) {
                        continue;
                    }
                    // A binary field of a file lists as DECIMAL.
                    let attr = match x.ty {
                        Ty::Bin(d, s) => format!("DECIMAL({d},{s})"),
                        _ => host_type(&x.ty),
                    };
                    self.add(&name, Some(line), attr, scope, "");
                }
            }
        }
    }

    /// The entry a host variable names in a scope.
    fn host(&mut self, scope: usize, name: &str, line: u32) {
        let up = name.to_ascii_uppercase();
        let (base, sub) = match up.split_once('.') {
            Some((a, b)) => (a.to_string(), Some(b.to_string())),
            None => (up.clone(), None),
        };
        let found = |e: &Entry, s: usize| matches!(e.what, What::Rpg(x) if x == s);
        let pick = |want: &str, qual: &str, s: usize| {
            self.entries.iter().position(|e| e.name == want && e.qual == qual && found(e, s))
        };
        let at = match &sub {
            Some(f) => pick(f, &base, scope).or_else(|| pick(f, &base, 0)),
            None => pick(&base, "", scope).or_else(|| pick(&base, "", 0)),
        };
        let r = self.rec(line);
        match at {
            Some(i) => self.entries[i].refs.push(r),
            None => {
                let i = self.entries.iter().position(|e| e.name == up && e.what == What::Undefined).unwrap_or_else(|| {
                    self.add(&up, None, String::new(), What::Undefined, "")
                });
                self.entries[i].refs.push(r);
            }
        }
    }

    /// A reference to a name of a kind (made the first time).
    fn named(&mut self, name: &str, what: What, attr: &str, line: u32) {
        let up = name.to_ascii_uppercase();
        let r = self.rec(line);
        let i = self.entries.iter().position(|e| e.name == up && e.what == what).unwrap_or_else(|| self.add(&up, None, attr.into(), what, ""));
        self.entries[i].refs.push(r);
    }

    /// A column: unqualified, it is the first column entry of its name;
    /// qualified, the one of its qualifier.
    fn column(&mut self, name: &str, qual: &str, line: u32) {
        let r = self.rec(line);
        let i = if qual.is_empty() {
            self.entries.iter().position(|e| e.name == name && e.what == What::Column)
        } else {
            self.entries.iter().position(|e| e.name == name && e.what == What::Column && e.qual == qual)
        };
        let i = i.unwrap_or_else(|| {
            let attr = if qual.is_empty() { "COLUMN".to_string() } else { format!("COLUMN IN {qual}") };
            self.add(name, None, attr, What::Column, qual)
        });
        self.entries[i].refs.push(r);
    }

    /// A table's columns, once, defined on `line`.
    fn describe(&mut self, ext: &mut dyn ExtFiles, table: &str, line: u32) {
        if self.described.iter().any(|t| t == table) {
            return;
        }
        let Some(f) = ext.describe(table, "*LIBL") else { return };
        self.described.push(table.to_string());
        let r = self.rec(line);
        if let Some(fm) = f.formats.first() {
            for x in &fm.fields {
                self.add(&x.name, Some(r), format!("{} IN {table}", column_type(x)), What::Described, table);
            }
        }
    }

    /// One statement's names.
    fn statement(&mut self, ext: &mut dyn ExtFiles, src: &rpgsqlpre::Source, ok: bool, syntax: bool) {
        let toks = scan::tokens(&src.text);
        // A fixed-form statement refers from its first line.
        let fixed = !self.c.read.lines.iter().find(|l| l.no == src.line).is_some_and(|l| l.fully_free || l.class == rpgfront::reader::Class::Free);
        let line = |t: &Tok| if fixed { src.line } else { src.line + scan::line_of(&src.text, t.start) };
        let word = |i: usize| toks.get(i).and_then(Tok::word).unwrap_or("");
        let verb = word(0).to_string();
        let mut skip = vec![false; toks.len()];
        let mut tables: Vec<(String, u32)> = Vec::new();
        let mut corr: Vec<String> = Vec::new();
        match verb.as_str() {
            "SET" if word(1) == "OPTION" => return,
            "INCLUDE" | "COMMIT" | "ROLLBACK" => return,
            "WHENEVER" => {
                if let Some(i) = toks.iter().position(|t| t.is("TO") || t.is("GOTO")) {
                    let label = word(i + 1).to_string();
                    if !label.is_empty() {
                        let define = tag_line(self.c, &label).map(|l| self.rec(l));
                        let r = self.rec(line(&toks[i + 1]));
                        let k = self
                            .entries
                            .iter()
                            .position(|e| e.name == label && e.what == What::Label)
                            .unwrap_or_else(|| self.add(&label, define, "LABEL".into(), What::Label, ""));
                        self.entries[k].refs.push(r);
                    }
                }
                return;
            }
            _ => {}
        }
        // A statement that is not one: its host variables alone (one in
        // error is read up to the error).
        let known = ["SELECT", "INSERT", "UPDATE", "DELETE", "VALUES", "SET", "DECLARE", "OPEN", "FETCH", "CLOSE", "PREPARE", "EXECUTE", "DESCRIBE", "GET"];
        if syntax && !known.contains(&verb.as_str()) {
            for t in &toks {
                if let TokKind::Host(h) = &t.kind {
                    self.host(src.scope, h, line(t));
                }
            }
            return;
        }
        // Cursors and statements by name.
        let mut i = 0;
        while i < toks.len() {
            let w = word(i);
            let prev = if i > 0 { word(i - 1) } else { "" };
            if verb == "DECLARE" && i == 1 {
                let name = w.to_string();
                let for_at = toks.iter().position(|t| t.is("FOR"));
                let target = for_at.map(|f| word(f + 1).to_string()).unwrap_or_default();
                let attr = if !target.is_empty() && target != "SELECT" && target != "WITH" {
                    format!("CURSOR FOR {target}")
                } else {
                    "CURSOR".into()
                };
                self.add(&name, Some(self.rec(src.line)), attr, What::Cursor, "");
                skip[i] = true;
                if let Some(f) = for_at
                    && !target.is_empty()
                    && target != "SELECT"
                    && target != "WITH"
                {
                    self.named(&target, What::Statement, "STATEMENT", line(&toks[f + 1]));
                    skip[f + 1] = true;
                }
            } else if (matches!(verb.as_str(), "OPEN" | "CLOSE") && i == 1)
                || (prev == "OF" && i >= 2 && word(i - 2) == "CURRENT")
                || (verb == "FETCH" && i >= 1 && !keyword(w) && !w.is_empty() && !w.starts_with(|c: char| c.is_ascii_digit()) && !matches!(toks[i].kind, TokKind::Host(_)) && toks.get(i.wrapping_sub(1)).is_some_and(|t| !t.sym('.')) && cursor_pos(&toks) == Some(i))
            {
                self.named(w, What::Cursor, "CURSOR", line(&toks[i]));
                skip[i] = true;
            } else if (matches!(verb.as_str(), "PREPARE" | "DESCRIBE") && i == stmt_name_at(&toks)) || (verb == "EXECUTE" && i == 1 && w != "IMMEDIATE") {
                self.named(w, What::Statement, "STATEMENT", line(&toks[i]));
                skip[i] = true;
            }
            i += 1;
        }
        if verb == "GET" {
            for t in &toks {
                if let TokKind::Host(h) = &t.kind {
                    self.host(src.scope, h, line(t));
                }
            }
            return;
        }
        // Tables and their correlation names.
        for k in 0..toks.len() {
            let w = word(k);
            let after_from = k > 0 && (toks[k - 1].is("FROM") || toks[k - 1].is("JOIN") || (toks[k - 1].is("INTO") && verb == "INSERT") || (toks[k - 1].is("UPDATE") && k == 1));
            let in_list = k > 0 && toks[k - 1].sym(',') && from_list(&toks, k);
            if (after_from || in_list) && !w.is_empty() && !keyword(w) && !skip[k] {
                tables.push((w.to_string(), line(&toks[k])));
                skip[k] = true;
                self.named(w, What::Table, "TABLE", line(&toks[k]));
                let mut n = k + 1;
                if toks.get(n).is_some_and(|t| t.is("AS")) {
                    n += 1;
                }
                if let Some(cw) = toks.get(n).and_then(Tok::word)
                    && !keyword(cw)
                    && !cw.starts_with(|c: char| c.is_ascii_digit())
                {
                    corr.push(cw.to_string());
                    skip[n] = true;
                }
            }
        }
        // Columns and host variables, in text order.
        let select_list = select_lists(&toks);
        for k in 0..toks.len() {
            match &toks[k].kind {
                TokKind::Host(h) => self.host(src.scope, h, line(&toks[k])),
                TokKind::Word(w) => {
                    if skip[k] || keyword(w) || w.starts_with(|c: char| c.is_ascii_digit()) {
                        continue;
                    }
                    if toks.get(k + 1).is_some_and(|t| t.sym('(')) {
                        continue;
                    }
                    if toks.get(k + 1).is_some_and(|t| t.sym('.')) {
                        // A qualifier.
                        if corr.contains(w) || tables.iter().any(|(t, _)| t == w) {
                            self.named(w, What::Table, "TABLE", line(&toks[k]));
                        }
                        continue;
                    }
                    let qual = (k >= 2 && toks[k - 1].sym('.')).then(|| word(k - 2).to_string());
                    match qual {
                        Some(q) if select_list[k] => {
                            let _ = q;
                        }
                        Some(q) => self.column(w, &q, line(&toks[k])),
                        None => self.column(w, "", line(&toks[k])),
                    }
                }
                _ => {}
            }
        }
        if ok && !syntax {
            for (t, at) in tables {
                self.describe(ext, &t, at);
            }
        }
    }
}

/// An array's type.
fn array(dim: u32, ty: &str) -> String {
    if dim > 0 { format!("ARRAY({dim}) {ty}") } else { ty.to_string() }
}

/// The token of `FETCH`'s cursor: the word before `INTO` or `USING`
/// or the last word, after any orientation.
fn cursor_pos(toks: &[Tok]) -> Option<usize> {
    let end = toks.iter().position(|t| t.is("INTO") || t.is("USING") || t.is("FOR")).unwrap_or(toks.len());
    (1..end).rev().find(|&i| toks[i].word().is_some_and(|w| !keyword(w) && !w.starts_with(|c: char| c.is_ascii_digit())))
}

/// `PREPARE s` and `DESCRIBE [INPUT] s`: the statement's token.
fn stmt_name_at(toks: &[Tok]) -> usize {
    if toks.get(1).is_some_and(|t| t.is("INPUT")) { 2 } else { 1 }
}

/// Whether token `k`, after a comma, is in a `FROM` list.
fn from_list(toks: &[Tok], k: usize) -> bool {
    let mut depth = 0i32;
    for i in (0..k).rev() {
        let t = &toks[i];
        if t.sym(')') {
            depth += 1;
        } else if t.sym('(') {
            if depth == 0 {
                return false;
            }
            depth -= 1;
        } else if depth == 0 && t.word().is_some_and(keyword) {
            return t.is("FROM");
        }
    }
    false
}

/// The tokens in a `SELECT`'s result list.
fn select_lists(toks: &[Tok]) -> Vec<bool> {
    let mut out = vec![false; toks.len()];
    let mut stack: Vec<(i32, bool)> = Vec::new();
    let mut depth = 0i32;
    let mut inside = false;
    for (k, t) in toks.iter().enumerate() {
        if t.sym('(') {
            depth += 1;
        } else if t.sym(')') {
            depth -= 1;
            if stack.last().is_some_and(|(d, _)| *d > depth) {
                inside = stack.pop().is_some_and(|(_, s)| s);
            }
        }
        if t.is("SELECT") {
            stack.push((depth, inside));
            inside = true;
            continue;
        }
        if inside && (t.is("FROM") || t.is("INTO")) && stack.last().is_some_and(|(d, _)| *d == depth) {
            inside = false;
        }
        out[k] = inside;
    }
    out
}

/// The line of the `TAG` that defines a label.
fn tag_line(c: &Compiled, label: &str) -> Option<u32> {
    let calcs = c.module.calcs.iter().chain(c.module.procs.iter().flat_map(|p| p.calcs.iter()));
    calcs
        .filter(|s| s.op.code == "TAG")
        .find(|s| s.op.f1.first().and_then(|e| e.name()).is_some_and(|n| n.eq_ignore_ascii_case(label)))
        .map(|s| s.at.line)
}

/// The cross-reference of a precompile.
pub(crate) fn entries(c: &Compiled, p: &rpgsqlpre::Precompiled, ext: &mut dyn ExtFiles, record_of: &dyn Fn(u32) -> u32) -> Vec<XrefEntry> {
    let mut x = Xref {
        c,
        entries: Vec::new(),
        record_of,
        described: Vec::new(),
    };
    let m = &c.module;
    x.defs(&m.defs, 0, "", false);
    x.ext_fields();
    for (k, p) in m.procs.iter().enumerate() {
        let name = p.name.to_ascii_uppercase();
        x.add(&name, Some(x.rec(p.at.line)), "RPG PROCEDURE".into(), What::Rpg(0), "");
        if let Some(pi) = &p.pi {
            x.defs(&pi.children, k + 1, &name, true);
        }
        x.defs(&p.defs, k + 1, &name, false);
    }
    // The statements: cursor declarations first.
    let srcs = super::sqlgen::sources(c);
    let bad: Vec<u32> = p.diags.iter().filter(|d| d.sev >= 20).map(|d| d.line).collect();
    let is_declare = |s: &rpgsqlpre::Source| {
        let up = s.text.to_ascii_uppercase();
        up.split_whitespace().next() == Some("DECLARE") && up.split_whitespace().take(5).any(|w| w == "CURSOR")
    };
    let ordered: Vec<&rpgsqlpre::Source> = srcs.iter().filter(|s| is_declare(s)).chain(srcs.iter().filter(|s| !is_declare(s))).collect();
    for s in ordered {
        let last = s.line + u32::try_from(s.text.matches('\n').count()).unwrap_or(0);
        let ok = !bad.iter().any(|l| *l >= s.line && *l <= last);
        let syntax = p.diags.iter().any(|d| d.msgid == "SQL0104" && d.line >= s.line && d.line <= last);
        x.statement(ext, s, ok, syntax);
    }
    let mut v: Vec<(Vec<u8>, usize, Entry)> = x.entries.into_iter().enumerate().map(|(k, e)| (ebcdic_key(&e.name), k, e)).collect();
    v.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    v.into_iter()
        .map(|(_, _, mut e)| {
            e.refs.sort_unstable();
            XrefEntry {
                name: e.name,
                define: e.define,
                attr: e.attr,
                refs: e.refs,
            }
        })
        .collect()
}
