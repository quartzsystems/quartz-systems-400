// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every reference the cross-reference lists, read from the syntax
//! tree: each use of a name on the line it is written, `M` where the
//! statement changes it; indicators as `*INxx` fields and as
//! indicators (`D` where a resulting indicator or an input
//! specification sets them).

use std::collections::{BTreeMap, HashMap};

use rpgfront::ast::{Conditioning, Def, Expr, Keyword, Stmt, Step};
use rpgfront::lex::Kind as TokKind;

use crate::compile::Compiled;
use crate::syms::{Kind, SymId};
use crate::types::Ty;

/// A reference: line and flag (`D`, `M` or blank).
pub(crate) type At = (u32, char);

/// What the walk found.
#[derive(Default)]
pub(crate) struct Refs {
    /// By symbol.
    pub syms: HashMap<SymId, Vec<At>>,
    /// `*INxx`, `*INLR`... used as fields.
    pub fields: BTreeMap<String, Vec<At>>,
    /// Indicators: `01`, `LR`, `OF`...
    pub inds: BTreeMap<String, Vec<At>>,
    /// Names used and not defined, with their references.
    pub undefined: BTreeMap<(usize, String), Vec<At>>,
    /// Subfields a qualified name asks for that its structure lacks.
    pub undefined_subs: BTreeMap<(SymId, String), Vec<At>>,
    /// Record formats named through their file (`FILE.FMT`).
    pub formats: HashMap<String, Vec<At>>,
    /// Subroutines, tags, key and parameter lists and exception
    /// names, by scope: what they are and their references.
    pub labels: BTreeMap<(usize, String), (&'static str, Vec<At>)>,
}

/// The indicator of a special word: `*IN01` is `01`, `*INLR` is `LR`.
fn indicator(word: &str) -> Option<String> {
    let w = word.strip_prefix("*IN")?;
    (w.len() == 2).then(|| w.to_string())
}

/// Fixed-form operations that leave their result field as it is.
fn keeps_result(op: &str) -> bool {
    matches!(
        op,
        "COMP" | "TESTN" | "TESTZ" | "TEST" | "LOOKUP" | "SETLL" | "SETGT" | "WRITE" | "UPDATE" | "DELETE" | "DSPLY"
            | "CALL" | "CALLB" | "KFLD" | "TAG" | "GOTO" | "EXSR" | "PLIST" | "KLIST" | "OPEN" | "CLOSE"
            | "FEOD" | "UNLOCK" | "EXCEPT" | "CAB" | "CABEQ" | "CABNE" | "CABLT" | "CABLE" | "CABGT" | "CABGE"
            | "CAS" | "CASEQ" | "CASNE" | "CASLT" | "CASLE" | "CASGT" | "CASGE"
    )
}

/// Keywords whose arguments are not names of the program.
fn not_names(kw: &str) -> bool {
    matches!(
        kw,
        "EXTNAME" | "EXTFLD" | "EXTFILE" | "EXTMBR" | "EXTPGM" | "PREFIX" | "DATFMT" | "TIMFMT" | "OPTIONS"
            | "RENAME" | "INCLUDE" | "IGNORE" | "DTAARA" | "CCSID" | "EXTDESC" | "EXPORT" | "IMPORT"
            | "OFLIND" | "RAFDATA"
    )
}

struct Walk<'a> {
    c: &'a Compiled,
    scope: usize,
    out: Refs,
    /// The statement being walked is free-form.
    free: bool,
}

/// Added to a line to sort a reference after the others; the listing
/// takes it off.
pub(crate) const LATE: u32 = 1 << 24;

/// `*IN(n)`'s index when it is a number.
fn lit_index(e: &Expr) -> Option<u32> {
    match e {
        Expr::Lit(rpgfront::ast::Lit::Num(n), _) => n.trim().parse().ok(),
        _ => None,
    }
}

/// The length of an operand of `MOVEA`, in characters.
fn operand_len(c: &Compiled, scope: usize, e: &Expr) -> Option<u32> {
    match e {
        Expr::Lit(rpgfront::ast::Lit::Char(s), _) => u32::try_from(s.chars().count()).ok(),
        Expr::Ref { name, .. } => {
            let id = c.table.find(scope, name)?;
            let s = &c.table.syms[id];
            Some(s.ty.size(&|i| c.table.ds_len(i)))
        }
        _ => None,
    }
}

impl Walk<'_> {
    fn find(&self, name: &str) -> Option<SymId> {
        self.c.table.find(self.scope, name)
    }

    fn sym(&mut self, id: SymId, line: u32, flag: char) {
        self.out.syms.entry(id).or_default().push((line, flag));
    }

    fn name(&mut self, name: &str, line: u32, flag: char) -> Option<SymId> {
        if let Some(l) = self.out.labels.get_mut(&(self.scope, name.to_string())) {
            l.1.push((line, flag));
            return None;
        }
        if let Some(id) = self.find(name) {
            self.sym(id, line, flag);
            Some(id)
        } else {
            self.out.undefined.entry((self.scope, name.to_string())).or_default().push((line, flag));
            None
        }
    }

    fn special(&mut self, word: &str, line: u32, modified: bool) {
        let flag = if modified { 'M' } else { ' ' };
        if let Some(ind) = indicator(word) {
            self.out.fields.entry(word.to_string()).or_default().push((line, flag));
            self.out.inds.entry(ind).or_default().push((line, flag));
        } else if word == "*IN" {
            self.out.fields.entry("*IN".to_string()).or_default().push((line, flag));
            self.out.inds.entry("*IN".to_string()).or_default().push((line, flag));
        } else if let Some(l) = self.out.labels.get_mut(&(self.scope, word.to_string())) {
            l.1.push((line, flag));
        }
    }

    fn ind(&mut self, ind: &str, line: u32, flag: char) {
        let ind = ind.trim().to_ascii_uppercase();
        let ind = ind.strip_prefix("*IN").unwrap_or(&ind).to_string();
        if ind.len() != 2 {
            return;
        }
        self.out.inds.entry(ind).or_default().push((line, flag));
    }

    /// An output specification's conditioning: each `OR` group on the
    /// line after the one before.
    fn cond_on(&mut self, c: &Conditioning, line: u32) {
        for (k, alt) in c.iter().enumerate() {
            for ci in alt {
                self.ind(&ci.ind, line + u32::try_from(k).unwrap_or(0), ' ');
            }
        }
    }

    /// Conditioning indicators, one to a line, the last on `line`.
    fn cond(&mut self, c: &Conditioning, line: u32) {
        let n = u32::try_from(c.iter().map(Vec::len).sum::<usize>()).unwrap_or(0);
        let mut k = 0;
        for alt in c {
            for ci in alt {
                self.ind(&ci.ind, (line + k + 1).saturating_sub(n), ' ');
                k += 1;
            }
        }
    }

    /// An expression; `target` when the statement changes it.
    fn expr(&mut self, e: &Expr, target: bool) {
        let flag = if target { 'M' } else { ' ' };
        match e {
            Expr::Ref { name, steps, at } => {
                let id = self.name(name, at.line, flag);
                let mut ty = id.map(|i| self.c.table.syms[i].ty.clone());
                for s in steps {
                    match s {
                        Step::Args(a) => {
                            for x in a {
                                self.expr(x, false);
                            }
                        }
                        Step::Field(f) if id.is_some_and(|i| matches!(self.c.table.syms[i].kind, Some(Kind::File(_)))) => {
                            self.out.formats.entry(f.clone()).or_default().push((at.line, ' '));
                        }
                        Step::Field(f) => {
                            let sub = match &ty {
                                Some(Ty::Ds(di)) => self.c.table.subfield(*di, f),
                                _ => None,
                            };
                            ty = sub.map(|s| self.c.table.syms[s].ty.clone());
                            if let Some(sid) = sub {
                                self.sym(sid, at.line, ' ');
                            } else if let Some(ds) = id {
                                self.out.undefined_subs.entry((ds, f.clone())).or_default().push((at.line, ' '));
                            }
                        }
                    }
                }
            }
            Expr::Special { word, index, at } => {
                self.special(word, at.line, target);
                if let Some(i) = index {
                    if let Some(n) = lit_index(i) {
                        // An element set by an assignment is noted last.
                        let line = if target { at.line + LATE } else { at.line };
                        self.out.inds.entry(format!("{n:02}")).or_default().push((line, flag));
                    }
                    self.expr(i, false);
                }
            }
            Expr::Bif { name, args, .. } => {
                // %SUBST and the like as a target change their first
                // operand; %NULLIND changes only the null indicator.
                for (k, a) in args.iter().enumerate() {
                    self.expr(a, target && k == 0 && name != "%LEN" && name != "%NULLIND");
                }
            }
            Expr::Unary(_, a, _) => self.expr(a, false),
            Expr::Binary(_, a, b, _) => {
                self.expr(a, false);
                self.expr(b, false);
            }
            Expr::Assign(_, a, b, _) => {
                self.expr(a, true);
                self.expr(b, false);
            }
            Expr::List(v, _) => {
                for x in v {
                    self.expr(x, false);
                }
            }
            Expr::Lit(..) | Expr::Fig(..) | Expr::Empty(_) => {}
        }
    }

    /// The array index of `LOOKUP`'s factor 2, which it sets.
    fn lookup(&mut self, e: &Expr) {
        if let Expr::Ref { name, steps, at } = e {
            self.name(name, at.line, ' ');
            for s in steps {
                if let Step::Args(a) = s {
                    for x in a {
                        self.expr(x, true);
                    }
                }
            }
        } else {
            self.expr(e, false);
        }
    }

    /// `MOVEA` to or from `*IN(n)`: the indicators it covers.
    fn movea(&mut self, s: &Stmt) -> bool {
        let line = s.at.line;
        let ind_of = |e: &Expr| match e {
            Expr::Special { word, index: Some(i), .. } if word == "*IN" => lit_index(i),
            _ => None,
        };
        let (Some(f2), Some(r)) = (s.op.f2.first(), s.op.result.first()) else { return false };
        let (start, len, flag) = if let Some(n) = ind_of(r) {
            let all = matches!(f2, Expr::Fig(..));
            let len = if all { 100 - n } else { operand_len(self.c, self.scope, f2).unwrap_or(1) };
            self.expr(f2, false);
            (n, len, 'M')
        } else if let Some(n) = ind_of(f2) {
            let len = operand_len(self.c, self.scope, r).unwrap_or(1);
            self.expr(r, true);
            (n, len, ' ')
        } else {
            return false;
        };
        self.out.fields.entry("*IN".to_string()).or_default().push((line, flag));
        self.out.inds.entry("*IN".to_string()).or_default().push((line, flag));
        for k in start..(start + len).min(100) {
            self.out.inds.entry(format!("{k:02}")).or_default().push((line, flag));
        }
        true
    }

    fn stmt(&mut self, s: &Stmt) {
        let line = s.at.line;
        if s.op.code == "MOVEA" && self.movea(s) {
            return;
        }
        if !s.level.is_empty() && s.level != "SR" && s.level != "L0" {
            self.ind(&s.level, line, ' ');
        }
        self.cond(&s.cond, line);
        let op = s.op.code.as_str();
        let defines = matches!(op, "BEGSR" | "TAG" | "KLIST" | "PLIST");
        for e in &s.op.f1 {
            if defines {
                if let Some(n) = label_name(e)
                    && let Some(l) = self.out.labels.get_mut(&(self.scope, n))
                {
                    l.1.push((e.pos().line, 'D'));
                }
                continue;
            }
            self.expr(e, op == "PARM");
        }
        let f2_changes = matches!(op, "CLEAR" | "RESET");
        for e in &s.op.f2 {
            if op == "LOOKUP" {
                self.lookup(e);
            } else {
                self.expr(e, f2_changes);
            }
        }
        // A call written as `CALLP` changes nothing; a bare call does.
        let bare_call = op == "CALLP"
            && self
                .c
                .read
                .lines
                .iter()
                .find(|l| l.no == line)
                .is_none_or(|l| !l.free_area().1.trim_start().to_ascii_uppercase().starts_with("CALLP"));
        if let Some(x) = &s.op.xf2 {
            if let ("FOR", Expr::Assign(_, a, b, _)) = (op, x) {
                self.expr(a, false);
                self.expr(b, false);
            } else if op == "CALLP" {
                if let Expr::Ref { name, steps, at } = x {
                    self.name(name, at.line, if bare_call { 'M' } else { ' ' });
                    for st in steps {
                        if let Step::Args(a) = st {
                            for y in a {
                                self.expr(y, false);
                            }
                        }
                    }
                } else {
                    self.expr(x, false);
                }
            } else if matches!(op, "XML-INTO" | "XML-SAX" | "DATA-INTO") {
                self.operands_of_into(x, op == "DATA-INTO");
            } else {
                self.expr(x, false);
            }
        }
        // A result field the statement defines is noted as defined only;
        // DEFINE's is defined by the statement.
        if s.op.len.trim().is_empty() && op != "DEFINE" {
            // A free-form DSPLY's response is changed.
            let changes = !keeps_result(op) || op == "DSPLY" && self.free;
            for e in &s.op.result {
                self.expr(e, changes);
            }
        }
        for (_, e) in &s.op.more {
            self.expr(e, false);
        }
        for r in &s.op.resind {
            self.ind(r, line, 'D');
        }
    }

    /// The operands of `XML-INTO`, `XML-SAX` and `DATA-INTO`: the
    /// target, or `%HANDLER`'s communication area, is changed.
    fn operands_of_into(&mut self, x: &Expr, handler_changed: bool) {
        let items: Vec<&Expr> = match x {
            Expr::List(v, _) => v.iter().collect(),
            e => vec![e],
        };
        for (k, e) in items.into_iter().enumerate() {
            match e {
                // DATA-INTO's handler procedure counts as changed too.
                Expr::Bif { name, args, .. } if name == "%HANDLER" => {
                    for (i, a) in args.iter().enumerate() {
                        self.expr(a, i == 1 || handler_changed);
                    }
                }
                e => self.expr(e, k == 0),
            }
        }
    }

    /// The names in a declaration's keywords.
    fn keywords(&mut self, kws: &[Keyword]) {
        for k in kws {
            if not_names(&k.name.to_ascii_uppercase()) {
                continue;
            }
            // LIKE of a subfield qualified by a structure that is not
            // qualified: the structure is named, and nothing used.
            if k.name.eq_ignore_ascii_case("LIKE")
                && let Some(a) = k.args.first()
                && a.toks.len() >= 3
                && a.toks[1].text() == "."
                && let TokKind::Name(h) = &a.toks[0].kind
                && let Some(id) = self.find(h)
                && matches!(self.c.table.syms[id].kind, Some(Kind::Ds(i)) if !self.c.table.ds[i].qualified)
            {
                self.sym(id, a.toks[0].at.line, 'n');
                continue;
            }
            // SFILE names the format, then the record number field.
            let skip = usize::from(k.name.eq_ignore_ascii_case("SFILE"));
            for a in k.args.iter().skip(skip) {
                for t in &a.toks {
                    match &t.kind {
                        TokKind::Name(n) => {
                            if let Some(id) = self.find(n) {
                                self.sym(id, t.at.line, ' ');
                            } else if let Some(l) = self.out.labels.get_mut(&(self.scope, n.to_ascii_uppercase())) {
                                // A subroutine (INFSR).
                                l.1.push((t.at.line, ' '));
                            } else if self.c.table.recfmts.iter().any(|r| r.name == *n) {
                                self.out.formats.entry(n.clone()).or_default().push((t.at.line, ' '));
                            }
                        }
                        TokKind::Special(w) => self.special(w, t.at.line, false),
                        _ => {}
                    }
                }
            }
        }
    }

    fn def(&mut self, d: &Def) {
        self.keywords(&d.kws);
        if let Some(v) = &d.value {
            self.expr(v, false);
        }
        for ch in &d.children {
            // An overlay of its own structure beside POS is dropped.
            let dropped = self.c.diags.iter().any(|x| x.msgid == "RNF3575" && x.at.line == ch.at.line);
            let own = |k: &&Keyword| {
                dropped
                    && k.name.eq_ignore_ascii_case("OVERLAY")
                    && k.args.first().and_then(|a| a.toks.first()).is_some_and(|t| matches!(&t.kind, TokKind::Name(n) if n.eq_ignore_ascii_case(&d.name)))
            };
            if ch.kws.iter().any(|k| own(&k)) {
                let mut ch = ch.clone();
                ch.kws.retain(|k| !own(&k));
                self.def(&ch);
            } else {
                self.def(ch);
            }
        }
    }
}

/// The name a label operation defines: a name or `*PSSR`-like word.
fn label_name(e: &Expr) -> Option<String> {
    match e {
        Expr::Ref { name, steps, .. } if steps.is_empty() => Some(name.clone()),
        Expr::Special { word, index: None, .. } => Some(word.clone()),
        _ => None,
    }
}

/// The labels each scope defines.
fn labels(c: &Compiled) -> BTreeMap<(usize, String), (&'static str, Vec<At>)> {
    let mut out = BTreeMap::new();
    let scopes = std::iter::once(&c.module.calcs).chain(c.module.procs.iter().map(|p| &p.calcs));
    for (scope, calcs) in scopes.enumerate() {
        for s in calcs {
            let what = match s.op.code.as_str() {
                "BEGSR" => "BEGSR",
                "TAG" => "TAG",
                "KLIST" => "KLIST",
                "PLIST" => "PLIST",
                _ => continue,
            };
            if let Some(n) = s.op.f1.first().and_then(label_name)

            {
                out.insert((scope, n), (what, Vec::new()));
            }
        }
    }
    for o in &c.module.outputs {
        if !o.except.is_empty() {
            out.entry((0, o.except.to_ascii_uppercase())).or_insert(("EXCEPT", Vec::new())).1.push((o.at.line, 'D'));
        }
    }
    out
}

/// Marks a reference line that is a listing number already (a
/// generated line).
pub(crate) const GEN: u32 = 1 << 25;

/// The generated `=I` line of each field, as a marked listing number.
fn generated_defs(c: &Compiled) -> HashMap<String, u32> {
    let plan = super::genlines::plan(c);
    let nums = super::source::numbers(c, &plan);
    let mut out = HashMap::new();
    for (b, first) in plan.iter().zip(&nums.blocks) {
        if b.kind == 'I' {
            for (name, k, _) in &b.defs {
                out.entry(name.clone()).or_insert((first + k) | GEN);
            }
        }
    }
    out
}

/// The names of free-form calculations the compiler dropped for an
/// error: IBM still lists their references.
fn dropped(w: &mut Walk<'_>) {
    let c = w.c;
    let kept: std::collections::HashSet<u32> =
        c.module.calcs.iter().chain(c.module.procs.iter().flat_map(|p| p.calcs.iter())).map(|s| s.at.line).collect();
    let info = super::stmts::analyze(&c.read.lines);
    for (l, inf) in c.read.lines.iter().zip(&info) {
        if l.member != 0 || !inf.starts || inf.ty != super::stmts::Ty::Spec('C') || kept.contains(&l.no) || !l.fully_free {
            continue;
        }
        // Calculations between procedures are ignored.
        let in_proc = c.module.procs.iter().any(|p| p.at.line <= l.no && l.no <= p.end.line);
        if !in_proc && c.module.procs.first().is_some_and(|p| l.no > p.at.line) {
            continue;
        }
        // What follows an ended statement is ignored altogether.
        if c.diags.iter().any(|d| d.at.line == l.no && d.msgid == "RNF5508") {
            continue;
        }
        let text = l.text.split("//").next().unwrap_or("").to_ascii_uppercase();
        let words: Vec<&str> = text.split(|ch: char| !(ch.is_ascii_alphanumeric() || "_#@$=*".contains(ch))).filter(|t| !t.is_empty()).collect();
        let op = words.first().copied().unwrap_or("");
        let assign = words.get(1) == Some(&"=");
        let names: Vec<&str> = words.iter().copied().filter(|t| *t != "=" && !t.starts_with('*') && w.find(t).is_some()).collect();
        let last = names.len().saturating_sub(1);
        for (k, n) in names.iter().enumerate() {
            let changed = ((assign || matches!(op, "CLEAR" | "RESET")) && k == 0) || (op == "DSPLY" && k == last.min(2) && k > 0);
            w.name(n, l.no, if changed { 'M' } else { ' ' });
        }
    }
}

/// Walk the module.
pub(crate) fn collect(c: &Compiled) -> Refs {
    let mut w = Walk {
        c,
        scope: 0,
        out: Refs {
            labels: labels(c),
            ..Refs::default()
        },
        free: false,
    };
    let m = &c.module;
    w.keywords(&m.ctl);
    for f in &m.files {
        w.keywords(&f.kws);
    }
    for d in &m.defs {
        w.def(d);
    }
    let gen_lines = generated_defs(c);
    for i in &m.inputs {
        if let Some(id) = w.find(&i.name.to_ascii_uppercase()) {
            w.sym(id, i.at.line, ' ');
        }
        w.ind(&i.rid, i.at.line, 'D');
        for f in &i.fields {
            let line = if i.external { gen_lines.get(&f.name.to_ascii_uppercase()).copied().unwrap_or(f.at.line) } else { f.at.line };
            for x in &f.inds {
                w.ind(x, line, 'D');
            }
            w.ind(&f.level, line, 'D');
        }
    }
    let free_at = |line: u32| {
        c.read.lines.iter().find(|l| l.no == line).is_some_and(|l| !matches!(l.class, rpgfront::reader::Class::Spec(_)))
    };
    // Calculations between procedures are ignored.
    let first_proc = m.procs.first().map_or(u32::MAX, |p| p.at.line);
    for s in m.calcs.iter().filter(|s| s.at.line < first_proc) {
        w.free = free_at(s.at.line);
        w.stmt(s);
    }
    for o in &m.outputs {
        let named = c.read.lines.iter().find(|l| l.no == o.at.line).is_none_or(|l| !l.text.get(6..16).unwrap_or("").trim().is_empty());
        if named && let Some(id) = w.find(&o.name.to_ascii_uppercase()) {
            w.sym(id, o.at.line, ' ');
        }
        w.cond_on(&o.cond, o.at.line);
        for f in &o.fields {
            w.cond_on(&f.cond, f.at.line);
            let name = f.field.to_ascii_uppercase();
            if (name.starts_with("PAGE") && name.len() <= 5) || name == "UDATE" || name == "*DATE" {
                w.out.fields.entry(name).or_default().push((f.at.line, ' '));
            } else if !name.is_empty() && !name.starts_with('*') {
                // Blank after clears the field.
                w.name(&name, f.at.line, if f.blank { 'M' } else { ' ' });
            }
        }
    }
    for (k, p) in m.procs.iter().enumerate() {
        if let Some(id) = c.table.find(0, &p.name.to_ascii_uppercase()) {
            w.sym(id, p.at.line, ' ');
            // A fixed-form procedure's end specification names it too.
            let named_end = c.read.lines.iter().any(|l| {
                l.no == p.end.line
                    && matches!(l.class, rpgfront::reader::Class::Spec('P' | 'p'))
                    && !l.text.get(6..21).unwrap_or("").trim().is_empty()
            });
            if p.end.line != p.at.line && named_end {
                w.sym(id, p.end.line, ' ');
            }
        }
        w.scope = k + 1;
        if let Some(pi) = &p.pi {
            w.def(pi);
        }
        for f in &p.files {
            w.keywords(&f.kws);
        }
        for d in &p.defs {
            w.def(d);
        }
        for s in &p.calcs {
            w.free = free_at(s.at.line);
            w.stmt(s);
        }
    }
    for ct in &m.ctdata {
        if !ct.name.is_empty() {
            // Loading the data is no use of the array.
            w.name(&ct.name.to_ascii_uppercase(), ct.at.line, 'm');
        }
    }
    dropped(&mut w);
    for f in c.files.iter().filter(|f| !f.auto_overflow) {
        if let Some(of) = &f.oflind {
            w.ind(of, f.def_line, 'd');
            let word = format!("*IN{}", of.trim().to_ascii_uppercase().trim_start_matches("*IN"));
            w.out.fields.entry(word).or_default().push((f.def_line, 'd'));
        }
    }
    w.out
}

/// Whether a symbol is defined where the table says (a prototype only
/// when a `DCL-PR` declares it).
pub(crate) fn has_definition(c: &Compiled, id: SymId) -> bool {
    let s = &c.table.syms[id];
    match s.kind {
        Some(Kind::Proto(_)) => {
            let declared = |defs: &[Def]| defs.iter().any(|d| d.kind == rpgfront::ast::DefKind::Pr && d.name.eq_ignore_ascii_case(&s.name));
            declared(&c.module.defs) || c.module.procs.iter().any(|p| declared(&p.defs))
        }
        Some(Kind::Subr | Kind::Label | Kind::Klist | Kind::Plist) => false,
        // Subfields a LIKEDS copies are defined where the original is.
        Some(Kind::Subfield) if s.name.is_empty() => false,
        Some(Kind::Subfield) => s.parent.is_none_or(|p| c.table.syms[p].def_line <= s.def_line),
        // A basing pointer BASED declares.
        Some(Kind::Field) if s.implicit && s.def_line == 0 => false,
        _ => true,
    }
}
