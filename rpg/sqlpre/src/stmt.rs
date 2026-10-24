// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One statement: its kind from its first words, its host variables,
//! the engine's text, the checks.

use std::collections::HashMap;

use sql::embed::{Desc, Op};
use sql::{Naming, SqlError};

use crate::scan::{self, Edits, HostTok, Kind, Tok, find_top, host_at, host_list, line_of};
use crate::{Cursor, Diag, Env, HostRef, Options, Rows, Source, Stmt, Whenever};

/// Schemas the system provides: the precompiler does not look for
/// their tables.
const SYSTEM_SCHEMAS: &[&str] = &["SYSIBM", "QSYS2", "SYSIBMADM", "QSYS", "SYSTOOLS"];

pub(crate) struct State<'a> {
    pub env: &'a mut dyn Env,
    pub opts: Options,
    pub cursors: HashMap<String, Cursor>,
    pub whenever: Whenever,
    pub diags: Vec<Diag>,
    pub first: bool,
}

/// A statement being read.
struct Cx<'s> {
    src: &'s Source,
    toks: Vec<Tok>,
}

impl Cx<'_> {
    fn line_at(&self, byte: usize) -> u32 {
        self.src.line + line_of(&self.src.text, byte)
    }

    fn tok_line(&self, i: usize) -> u32 {
        self.toks.get(i).map_or(self.src.line, |t| self.line_at(t.start))
    }

    fn host_ref(&self, h: &HostTok) -> HostRef {
        HostRef {
            name: h.name.clone(),
            ind: h.ind.clone(),
            line: self.tok_line(h.first),
        }
    }

    fn word(&self, i: usize) -> &str {
        self.toks.get(i).and_then(Tok::word).unwrap_or("")
    }

    fn end_byte(&self, i: usize) -> usize {
        self.toks.get(i).map_or(self.src.text.len(), |t| t.start)
    }
}

/// The text with every host variable from token `from` to `to` (and
/// its indicator) as `?`, the spans in `remove` taken out; the input
/// host variables in order.
fn rewrite(
    cx: &Cx<'_>,
    fields: &dyn Fn(&str) -> u32,
    from: usize,
    to: usize,
    remove: &[(usize, usize)],
    extra: &mut Edits,
) -> (String, Vec<HostTok>) {
    let mut inputs = Vec::new();
    let mut i = from;
    while i < to {
        let in_removed = remove.iter().any(|&(a, b)| i >= a && i < b);
        if !in_removed && let Some(h) = host_at(&cx.toks, i) {
            let s = cx.toks[h.first].start;
            let e = cx.toks[h.next - 1].end;
            // A structure stands for its subfields.
            let marks = vec!["?"; fields(&h.name).max(1) as usize].join(", ");
            extra.replace(s, e, &marks);
            i = h.next;
            inputs.push(h);
            continue;
        }
        i += 1;
    }
    for &(a, b) in remove {
        let s = cx.toks[a].start;
        let e = cx.toks.get(b).map_or(cx.src.text.len(), |t| t.start);
        extra.replace(s, e, " ");
    }
    let start = cx.toks.get(from).map_or(cx.src.text.len(), |t| t.start);
    let end = cx.end_byte(to);
    let body = extra.apply_range(&cx.src.text, start, end);
    // Keep the line breaks before the range, so lines still count.
    let lead = "\n".repeat(cx.src.text[..start].matches('\n').count());
    (format!("{lead}{body}"), inputs)
}

impl State<'_> {
    /// The fields a host variable stands for: a structure's subfields.
    fn fields(&self, cx: &Cx<'_>, name: &str) -> u32 {
        self.env.host(cx.src.scope, name).map_or(1, |i| i.fields.max(1))
    }

    fn diag(&mut self, line: u32, msgid: &'static str, sev: u8, args: Vec<String>) {
        self.diags.push(Diag { line, msgid, sev, args });
    }

    fn naming(&self) -> Naming {
        if self.opts.sql_naming() { Naming::Sql } else { Naming::Sys }
    }

    /// Host variables must be ones the compiler knows.
    fn check_hosts(&mut self, scope: usize, hosts: &[HostRef]) {
        for h in hosts {
            for n in std::iter::once(&h.name).chain(h.ind.iter()) {
                if self.env.host(scope, n).is_none() {
                    self.diag(h.line, "SQL0312", 30, vec![n.clone()]);
                }
            }
        }
    }

    /// Parse the engine's text: a syntax error is SQL0104; the tables
    /// it names must be found (SQL1103, a warning).
    fn check_text(&mut self, cx: &Cx<'_>, text: &str) {
        match sql::parse(text, self.naming()) {
            Ok(st) => {
                if let sql::Statement::Update(sql::ast::Update { where_clause: sql::ast::Where::All, .. }) = &st {
                    self.diag(cx.src.line, "SQL0088", 0, vec!["UPDATE".into()]);
                }
                if let sql::Statement::Delete(sql::ast::Delete { where_clause: sql::ast::Where::All, .. }) = &st {
                    self.diag(cx.src.line, "SQL0088", 0, vec!["DELETE".into()]);
                }
                for t in sql::embed::tables(&st) {
                    let schema = t.schema.as_ref().map(|s| s.value.clone());
                    if schema.as_deref().is_some_and(|s| SYSTEM_SCHEMAS.contains(&s)) {
                        continue;
                    }
                    if !self.env.table(schema.as_deref(), &t.name.value) {
                        let at = cx.toks.iter().position(|k| k.is(&t.name.value)).unwrap_or(0);
                        let line = cx.tok_line(at);
                        let lib = schema.unwrap_or_else(|| "*LIBL".into());
                        self.diag(line, "SQL1103", 10, vec![t.name.value.clone(), lib]);
                    }
                }
            }
            Err(SqlError::Syntax { line, col, found, .. }) => {
                let at = cx.src.line + line.saturating_sub(1);
                self.diag(at, "SQL0104", 30, vec![at.to_string(), col.to_string(), found]);
            }
            Err(SqlError::ThreePartName { name }) => {
                self.diag(cx.src.line, "SQL0104", 30, vec![cx.src.line.to_string(), "1".into(), name]);
            }
            Err(_) => {}
        }
    }

    /// A syntax error at token `i`.
    fn bad(&mut self, cx: &Cx<'_>, i: usize) {
        let found = cx.toks.get(i).map_or_else(|| "end of statement".to_string(), |t| cx.src.text[t.start..t.end].to_string());
        let at = cx.toks.get(i).map_or(cx.src.text.len(), |t| t.start);
        let col = at - cx.src.text[..at].rfind('\n').map_or(0, |p| p + 1) + 1;
        let line = cx.tok_line(i);
        self.diag(line, "SQL0104", 30, vec![line.to_string(), col.to_string(), found]);
    }

    fn base(&self, cx: &Cx<'_>, verb: &str, op: Op) -> Stmt {
        let last = cx.src.line + line_of(&cx.src.text, cx.src.text.trim_end().len());
        Stmt {
            line: cx.src.line,
            last_line: last,
            scope: cx.src.scope,
            verb: verb.to_string(),
            desc: Desc {
                op,
                sql_naming: self.opts.sql_naming(),
                ..Desc::default()
            },
            whenever: self.whenever.clone(),
            ds: true,
            ..Stmt::default()
        }
    }

    /// One statement; `None` when it only acts at compile time and the
    /// listing generates nothing for it.
    pub fn statement(&mut self, src: &Source) -> Option<Stmt> {
        let cx = Cx {
            src,
            toks: scan::tokens(&src.text),
        };
        let w0 = cx.word(0).to_string();
        let w1 = cx.word(1).to_string();
        if cx.toks.is_empty() {
            self.bad(&cx, 0);
            return None;
        }
        match w0.as_str() {
            "SELECT" => Some(self.select_into(&cx)),
            "VALUES" => Some(self.values_into(&cx)),
            "SET" if w1 == "OPTION" => {
                self.set_option(&cx);
                None
            }
            "SET" if cx.toks.get(1).is_some_and(|t| matches!(t.kind, Kind::Host(_)) || t.sym('(')) => Some(self.set_hosts(&cx)),
            "DECLARE" => {
                self.declare(&cx);
                None
            }
            "OPEN" => Some(self.open(&cx)),
            "FETCH" => Some(self.fetch(&cx)),
            "CLOSE" => {
                let mut s = self.base(&cx, "CLOSE", Op::Close);
                s.desc.name = cx.word(1).to_string();
                if !self.cursors.contains_key(&s.desc.name) {
                    self.diag(cx.tok_line(1), "SQL0504", 30, vec![s.desc.name.clone()]);
                }
                Some(s)
            }
            "PREPARE" => Some(self.prepare(&cx)),
            "EXECUTE" if w1 == "IMMEDIATE" => Some(self.dynamic(&cx, 2, Op::ExecImm, "EXECUTE IMMEDIATE")),
            "EXECUTE" => Some(self.execute(&cx)),
            "GET" if w1 == "DIAGNOSTICS" => Some(self.diagnostics(&cx)),
            "WHENEVER" => {
                self.whenever_stmt(&cx);
                None
            }
            "INCLUDE" | "BEGIN" | "END" => None,
            _ => Some(self.exec(&cx, &w0)),
        }
    }

    /// `SELECT ... INTO :a, :b FROM ...`.
    fn select_into(&mut self, cx: &Cx<'_>) -> Stmt {
        let mut s = self.base(cx, "SELECT", Op::Query);
        let n = cx.toks.len();
        let into = find_top(&cx.toks, 1, |t| t.is("INTO"));
        let Some(into) = into else {
            self.diag(cx.src.line, "SQL0104", 30, vec!["SELECT".into()]);
            return s;
        };
        let from = find_top(&cx.toks, into, |t| t.is("FROM")).unwrap_or(n);
        s.outputs = host_list(&cx.toks, into + 1, from).iter().map(|h| cx.host_ref(h)).collect();
        let (text, ins) = rewrite(cx, &|h| self.fields(cx, h), 0, n, &[(into, from)], &mut Edits::default());
        s.inputs = ins.iter().map(|h| cx.host_ref(h)).collect();
        self.check_hosts(cx.src.scope, &s.outputs);
        self.check_hosts(cx.src.scope, &s.inputs);
        self.check_text(cx, &text);
        s.desc.text = flat(&text);
        s
    }

    /// `VALUES expr INTO :a` and `VALUES (a, b) INTO :a, :b`.
    fn values_into(&mut self, cx: &Cx<'_>) -> Stmt {
        let mut s = self.base(cx, "VALUES", Op::Query);
        let n = cx.toks.len();
        let Some(into) = find_top(&cx.toks, 1, |t| t.is("INTO")) else {
            return self.exec(cx, "VALUES");
        };
        s.outputs = host_list(&cx.toks, into + 1, n).iter().map(|h| cx.host_ref(h)).collect();
        let mut e = Edits::default();
        if !cx.toks.get(1).is_some_and(|t| t.sym('(')) {
            let a = cx.toks[1].start;
            e.replace(a, a, "(");
            let b = cx.toks[into].start;
            e.replace(b, b, ")");
        }
        let (text, ins) = rewrite(cx, &|h| self.fields(cx, h), 0, n, &[(into, n)], &mut e);
        s.inputs = ins.iter().map(|h| cx.host_ref(h)).collect();
        self.check_hosts(cx.src.scope, &s.outputs);
        self.check_hosts(cx.src.scope, &s.inputs);
        self.check_text(cx, &text);
        s.desc.text = flat(&text);
        s
    }

    /// `SET :a = e1, :b = e2` and `SET (:a, :b) = (e1, e2)`: a `VALUES`.
    fn set_hosts(&mut self, cx: &Cx<'_>) -> Stmt {
        let mut s = self.base(cx, "SET", Op::Query);
        let n = cx.toks.len();
        let mut e = Edits::default();
        let mut outs = Vec::new();
        let mut remove = Vec::new();
        if cx.toks[1].sym('(') {
            let close = find_top(&cx.toks, 2, |t| t.sym(')')).unwrap_or(n);
            outs = host_list(&cx.toks, 2, close);
            let eq = find_top(&cx.toks, close, |t| t.sym('=')).unwrap_or(n);
            e.replace(cx.toks[0].start, cx.toks[0].end, "VALUES");
            remove.push((1, eq + 1));
        } else {
            // Each `:hv =` at the top level starts an assignment.
            let mut i = 1;
            let mut first = true;
            while i < n {
                let Some(h) = host_at(&cx.toks, i) else {
                    self.bad(cx, i);
                    break;
                };
                let eq = h.next;
                if !cx.toks.get(eq).is_some_and(|t| t.sym('=')) {
                    self.bad(cx, eq);
                    break;
                }
                let end = find_top(&cx.toks, eq + 1, |t| t.sym(',')).unwrap_or(n);
                let at = cx.toks[h.first].start;
                let to = cx.toks[eq].end;
                if first {
                    e.replace(cx.toks[0].start, to, "VALUES (");
                    remove.push((1, 1));
                    let _ = at;
                } else {
                    e.replace(cx.toks[i - 1].start, to, ",");
                }
                outs.push(h);
                first = false;
                i = end + 1;
            }
            let last = cx.src.text.trim_end().len();
            e.replace(last, last, ")");
        }
        s.outputs = outs.iter().map(|h| cx.host_ref(h)).collect();
        // The targets are not inputs.
        let mut skip: Vec<(usize, usize)> = outs.iter().map(|h| (h.first, h.next)).collect();
        skip.extend(remove.iter().copied().filter(|r| r.0 < r.1));
        let (text, ins) = rewrite(cx, &|h| self.fields(cx, h), 0, cx.toks.len(), &skip, &mut e);
        s.inputs = ins.iter().map(|h| cx.host_ref(h)).collect();
        self.check_hosts(cx.src.scope, &s.outputs);
        self.check_hosts(cx.src.scope, &s.inputs);
        self.check_text(cx, &text);
        s.desc.text = flat(&text);
        s
    }

    /// `SET OPTION name = value, ...`.
    fn set_option(&mut self, cx: &Cx<'_>) {
        if !self.first {
            self.diag(cx.src.line, "SQL0199", 30, vec!["SET OPTION".into()]);
        }
        let n = cx.toks.len();
        let mut i = 2;
        while i < n {
            let name = cx.word(i).to_string();
            if !cx.toks.get(i + 1).is_some_and(|t| t.sym('=')) {
                self.bad(cx, i + 1);
                return;
            }
            let end = find_top(&cx.toks, i + 2, |t| t.sym(',')).unwrap_or(n);
            let value: String = cx.toks[i + 2..end]
                .iter()
                .map(|t| match &t.kind {
                    Kind::Word(w) => w.clone(),
                    Kind::Str(s) => s.clone(),
                    Kind::Sym(c) => c.to_string(),
                    Kind::Delim(d) | Kind::Host(d) => d.clone(),
                })
                .collect();
            self.diag(cx.src.line, "SQL5066", 0, vec![name.clone()]);
            let o = &mut self.opts;
            match name.as_str() {
                "COMMIT" => o.commit = value,
                "NAMING" => o.naming = value,
                "DATFMT" => o.datfmt = value,
                "DATSEP" => o.datsep = value,
                "TIMFMT" => o.timfmt = value,
                "TIMSEP" => o.timsep = value,
                "CLOSQLCSR" => o.closqlcsr = value,
                _ => o.other.push((name, value)),
            }
            i = end + 1;
        }
    }

    /// `DECLARE c [SCROLL] CURSOR [WITH HOLD] FOR query|stmt` and
    /// `DECLARE s STATEMENT`.
    fn declare(&mut self, cx: &Cx<'_>) {
        let n = cx.toks.len();
        let name = cx.word(1).to_string();
        if cx.word(2) == "STATEMENT" {
            return;
        }
        let mut c = Cursor::default();
        let mut i = 2;
        while i < n && !cx.toks[i].is("CURSOR") {
            if cx.toks[i].is("SCROLL") && !(i > 0 && cx.toks[i - 1].is("NO")) {
                c.scroll = true;
            }
            i += 1;
        }
        if i >= n {
            self.bad(cx, i);
            return;
        }
        i += 1;
        while i < n && !cx.toks[i].is("FOR") {
            if cx.toks[i].is("HOLD") && cx.toks[i - 1].is("WITH") {
                c.hold = true;
            }
            i += 1;
        }
        let q = i + 1;
        if q >= n {
            self.bad(cx, q);
            return;
        }
        let w = cx.word(q);
        if matches!(w, "SELECT" | "WITH" | "VALUES") || cx.toks[q].sym('(') {
            let (text, ins) = rewrite(cx, &|h| self.fields(cx, h), q, n, &[], &mut Edits::default());
            c.inputs = ins.iter().map(|h| cx.host_ref(h)).collect();
            self.check_hosts(cx.src.scope, &c.inputs);
            self.check_text(cx, &text);
            c.text = flat(&text);
        } else {
            c.stmt = w.to_string();
        }
        self.cursors.insert(name, c);
    }

    /// `OPEN c [USING :a, ...]`.
    fn open(&mut self, cx: &Cx<'_>) -> Stmt {
        let mut s = self.base(cx, "OPEN", Op::Open);
        let name = cx.word(1).to_string();
        s.desc.name.clone_from(&name);
        let Some(c) = self.cursors.get(&name).cloned() else {
            self.diag(cx.tok_line(1), "SQL0504", 30, vec![name]);
            return s;
        };
        s.desc.text = c.text;
        s.desc.stmt = c.stmt;
        s.desc.hold = c.hold;
        s.desc.scroll = c.scroll;
        s.inputs = c.inputs;
        if let Some(u) = find_top(&cx.toks, 2, |t| t.is("USING")) {
            let us: Vec<HostRef> = host_list(&cx.toks, u + 1, cx.toks.len()).iter().map(|h| cx.host_ref(h)).collect();
            self.check_hosts(cx.src.scope, &us);
            s.inputs.extend(us);
        }
        s
    }

    /// `FETCH [orientation] [FROM] c [FOR n ROWS] [INTO :a, ...]`.
    fn fetch(&mut self, cx: &Cx<'_>) -> Stmt {
        let mut s = self.base(cx, "FETCH", Op::Fetch);
        let n = cx.toks.len();
        let mut i = 1;
        let mut orient = String::new();
        match cx.word(i) {
            w @ ("NEXT" | "PRIOR" | "FIRST" | "LAST" | "BEFORE" | "AFTER" | "CURRENT") => {
                orient = w.to_string();
                i += 1;
            }
            w @ ("RELATIVE" | "ABSOLUTE") => {
                orient = w.to_string();
                i += 1;
                if let Some(h) = host_at(&cx.toks, i) {
                    orient.push_str(" ?");
                    s.inputs.push(cx.host_ref(&h));
                    i = h.next;
                } else {
                    let mut num = String::new();
                    while i < n && (cx.toks[i].sym('-') || cx.toks[i].sym('+')) {
                        num.push_str(&cx.src.text[cx.toks[i].start..cx.toks[i].end]);
                        i += 1;
                    }
                    num.push_str(cx.word(i));
                    i += 1;
                    orient = format!("{orient} {num}");
                }
            }
            _ => {}
        }
        if cx.word(i) == "FROM" {
            i += 1;
        }
        let name = cx.word(i).to_string();
        if name.is_empty() {
            self.bad(cx, i);
            return s;
        }
        if !self.cursors.contains_key(&name) {
            self.diag(cx.tok_line(i), "SQL0504", 30, vec![name.clone()]);
        }
        s.desc.name = name;
        s.desc.orient = orient;
        i += 1;
        if cx.word(i) == "FOR" {
            i += 1;
            if let Some(h) = host_at(&cx.toks, i) {
                s.rows = Some(Rows::Host(cx.host_ref(&h)));
                i = h.next;
            } else {
                s.rows = Some(Rows::Lit(cx.word(i).parse().unwrap_or(1)));
                i += 1;
            }
            if matches!(cx.word(i), "ROW" | "ROWS") {
                i += 1;
            }
            s.desc.rows = true;
        }
        if cx.word(i) == "INTO" {
            s.outputs = host_list(&cx.toks, i + 1, n).iter().map(|h| cx.host_ref(h)).collect();
        } else if i < n {
            self.bad(cx, i);
        }
        let mut all = s.outputs.clone();
        all.extend(s.inputs.clone());
        if let Some(Rows::Host(h)) = &s.rows {
            all.push(h.clone());
        }
        self.check_hosts(cx.src.scope, &all);
        s
    }

    /// `PREPARE s FROM :hv` or `FROM 'text'`.
    fn prepare(&mut self, cx: &Cx<'_>) -> Stmt {
        let name = cx.word(1).to_string();
        let from = find_top(&cx.toks, 2, |t| t.is("FROM")).unwrap_or(cx.toks.len());
        let mut s = self.dynamic(cx, from + 1, Op::Prepare, "PREPARE");
        s.desc.name = name;
        s
    }

    /// The text of `PREPARE` or `EXECUTE IMMEDIATE`: a host variable or
    /// a literal at token `i`.
    fn dynamic(&mut self, cx: &Cx<'_>, i: usize, op: Op, verb: &str) -> Stmt {
        let mut s = self.base(cx, verb, op);
        if let Some(h) = host_at(&cx.toks, i) {
            s.inputs.push(cx.host_ref(&h));
            self.check_hosts(cx.src.scope, &s.inputs);
        } else if let Some(Kind::Str(t)) = cx.toks.get(i).map(|t| &t.kind) {
            s.desc.text.clone_from(t);
        } else {
            self.bad(cx, i);
        }
        s
    }

    /// `EXECUTE s [USING :a, ...]`.
    fn execute(&mut self, cx: &Cx<'_>) -> Stmt {
        let mut s = self.base(cx, "EXECUTE", Op::Execute);
        s.desc.name = cx.word(1).to_string();
        if let Some(u) = find_top(&cx.toks, 2, |t| t.is("USING")) {
            s.inputs = host_list(&cx.toks, u + 1, cx.toks.len()).iter().map(|h| cx.host_ref(h)).collect();
            self.check_hosts(cx.src.scope, &s.inputs);
        }
        s
    }

    /// `GET DIAGNOSTICS [CONDITION n] :a = ITEM, ...`.
    fn diagnostics(&mut self, cx: &Cx<'_>) -> Stmt {
        let mut s = self.base(cx, "GET DIAGNOSTICS", Op::Diag);
        let n = cx.toks.len();
        let mut i = 2;
        if cx.word(i) == "CONDITION" {
            i += 2;
        }
        let mut items = Vec::new();
        while i < n {
            let Some(h) = host_at(&cx.toks, i) else {
                self.bad(cx, i);
                break;
            };
            if !cx.toks.get(h.next).is_some_and(|t| t.sym('=')) {
                self.bad(cx, h.next);
                break;
            }
            items.push(cx.word(h.next + 1).to_string());
            s.outputs.push(cx.host_ref(&h));
            i = h.next + 3;
        }
        s.desc.text = items.join(",");
        let outs = s.outputs.clone();
        self.check_hosts(cx.src.scope, &outs);
        s
    }

    /// `WHENEVER SQLERROR|SQLWARNING|NOT FOUND CONTINUE|GO TO label`.
    fn whenever_stmt(&mut self, cx: &Cx<'_>) {
        let (cond, mut i) = match cx.word(1) {
            "NOT" if cx.word(2) == "FOUND" => ("NOT FOUND", 3),
            "SQLERROR" => ("SQLERROR", 2),
            "SQLWARNING" => ("SQLWARNING", 2),
            _ => {
                self.bad(cx, 1);
                return;
            }
        };
        let action = match cx.word(i) {
            "CONTINUE" => None,
            "GO" if cx.word(i + 1) == "TO" => {
                i += 2;
                Some(label_at(cx, i))
            }
            "GOTO" => {
                i += 1;
                Some(label_at(cx, i))
            }
            _ => {
                self.bad(cx, i);
                return;
            }
        };
        match cond {
            "NOT FOUND" => self.whenever.not_found = action,
            "SQLERROR" => self.whenever.error = action,
            _ => self.whenever.warning = action,
        }
    }

    /// Any other statement: run as it is, host variables as inputs.
    fn exec(&mut self, cx: &Cx<'_>, verb: &str) -> Stmt {
        let mut s = self.base(cx, verb, Op::Exec);
        let (text, ins) = rewrite(cx, &|h| self.fields(cx, h), 0, cx.toks.len(), &[], &mut Edits::default());
        s.inputs = ins.iter().map(|h| cx.host_ref(h)).collect();
        self.check_hosts(cx.src.scope, &s.inputs);
        self.check_text(cx, &text);
        s.desc.text = flat(&text);
        s
    }
}

/// A label after `GO TO`, with or without a colon.
fn label_at(cx: &Cx<'_>, i: usize) -> String {
    match cx.toks.get(i).map(|t| &t.kind) {
        Some(Kind::Host(h)) => h.clone(),
        Some(Kind::Word(w)) => w.clone(),
        _ => String::new(),
    }
}

/// The text on one line, as the program keeps it.
fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
