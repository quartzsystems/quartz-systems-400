// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Declarations and references from the tree: files, definitions,
//! procedures, subroutines, tags and lists are declared; every name an
//! operand, keyword or specification uses is a reference.

use compdiag::front::{Declaration, Reference, SymbolKind};

use super::{Analysis, LineMap};
use crate::ast::{Def, DefKind, Expr, FileDecl, Keyword, Module, Pos, Step, Stmt};

/// Fill `a`'s declarations and references from its tree.
pub(super) fn collect(a: &mut Analysis, map: &LineMap) {
    let mut w = Walker {
        map,
        decls: Vec::new(),
        refs: Vec::new(),
        scope: None,
    };
    w.module(&a.ast);
    a.declarations = w.decls;
    a.references = w.refs;
    a.declarations.sort_by_key(|d| (d.source, d.span.start));
    a.references.sort_by_key(|r| (r.source, r.span.start));
    // A reference's scope: the procedure whose lines hold it.
    let procs: Vec<(String, usize, u32, u32)> = a
        .ast
        .procs
        .iter()
        .filter_map(|p| {
            let (s, from) = map.place(p.at.line)?;
            let (_, to) = map.place(p.end.line)?;
            Some((p.name.to_ascii_uppercase(), s, from, to.max(from)))
        })
        .collect();
    a.link(|r| {
        procs
            .iter()
            .find(|(_, s, from, to)| *s == r.source && (*from..=*to).contains(&r.span.line))
            .map(|(n, ..)| n.clone())
    });
}

struct Walker<'m> {
    map: &'m LineMap,
    decls: Vec<Declaration>,
    refs: Vec<Reference>,
    scope: Option<String>,
}

impl Walker<'_> {
    fn decl(&mut self, name: &str, kind: SymbolKind, at: Pos, detail: String) {
        if name.is_empty() || name.starts_with('*') {
            return;
        }
        let Some((source, span)) = self.map.find(at.line, at.col, name) else {
            return;
        };
        self.decls.push(Declaration {
            name: name.to_ascii_uppercase(),
            kind,
            source,
            span,
            detail,
            scope: self.scope.clone(),
        });
    }

    fn reference(&mut self, name: &str, kind: SymbolKind, at: Pos) {
        if name.is_empty() || name.starts_with('*') || name.starts_with('%') {
            return;
        }
        let Some((source, span)) = self.map.find(at.line, at.col, name) else {
            return;
        };
        self.refs.push(Reference {
            name: name.to_ascii_uppercase(),
            kind,
            source,
            span,
            declaration: None,
        });
    }

    fn module(&mut self, m: &Module) {
        self.keywords(&m.ctl);
        for f in &m.files {
            self.file(f);
        }
        for d in &m.defs {
            self.def(d, None);
        }
        for i in &m.inputs {
            self.reference(&i.name, SymbolKind::Format, i.at);
            for f in &i.fields {
                if i.external {
                    self.reference(&f.name, SymbolKind::Field, f.at);
                } else {
                    self.decl(&f.name, SymbolKind::Field, f.at, "input field".into());
                }
            }
        }
        for s in &m.calcs {
            self.stmt(s);
        }
        for o in &m.outputs {
            self.reference(&o.name, SymbolKind::Format, o.at);
            if !o.except.is_empty() {
                self.reference(&o.except, SymbolKind::Label, o.at);
            }
            for f in &o.fields {
                if !matches!(
                    f.field.as_str(),
                    "PAGE" | "UDATE" | "UDAY" | "UMONTH" | "UYEAR"
                ) && !f.field.starts_with("PAGE")
                {
                    self.reference(&f.field, SymbolKind::Field, f.at);
                }
                if let Some(e) = &f.constant {
                    self.expr(e, SymbolKind::Variable);
                }
            }
        }
        for p in &m.procs {
            self.decl(&p.name, SymbolKind::Procedure, p.at, "procedure".into());
            self.scope = Some(p.name.to_ascii_uppercase());
            self.keywords(&p.kws);
            if let Some(pi) = &p.pi {
                self.def(pi, None);
            }
            for f in &p.files {
                self.file(f);
            }
            for d in &p.defs {
                self.def(d, None);
            }
            for s in &p.calcs {
                self.stmt(s);
            }
            self.scope = None;
        }
        for c in &m.ctdata {
            if !c.name.is_empty() {
                self.reference(&c.name, SymbolKind::Variable, c.at);
            }
        }
    }

    fn file(&mut self, f: &FileDecl) {
        let detail = f
            .fixed
            .as_ref()
            .map_or_else(|| "file".to_string(), |c| format!("file {}", c.device));
        self.decl(&f.name, SymbolKind::File, f.at, detail);
        self.keywords(&f.kws);
    }

    fn def(&mut self, d: &Def, parent: Option<&str>) {
        let (kind, word) = match d.kind {
            DefKind::Standalone => (SymbolKind::Variable, "S"),
            DefKind::Const => (SymbolKind::Constant, "C"),
            DefKind::Ds => (SymbolKind::Structure, "DS"),
            DefKind::Subfield => (SymbolKind::Subfield, "subfield"),
            DefKind::Pr => (SymbolKind::Prototype, "PR"),
            DefKind::Pi => (SymbolKind::Procedure, "PI"),
            DefKind::Parm => (SymbolKind::Parameter, "parameter"),
            DefKind::Enum => (SymbolKind::Constant, "ENUM"),
        };
        let mut detail = word.to_string();
        if let Some(c) = &d.fixed {
            let t = format!("{}{}{}", c.to, c.ty, c.decimals);
            if !t.is_empty() {
                detail = format!("{detail} {t}");
            }
        } else if let Some(k) = d.kws.first() {
            detail = format!("{detail} {}", k.name);
        }
        if let Some(p) = parent {
            detail = format!("{detail} of {p}");
        }
        // A PI repeats its procedure's name; the parameters are its own.
        if d.kind != DefKind::Pi {
            self.decl(&d.name, kind, d.at, detail);
        }
        self.keywords(&d.kws);
        if let Some(v) = &d.value {
            self.expr(v, SymbolKind::Constant);
        }
        let me = d.name.to_ascii_uppercase();
        for c in &d.children {
            let c_kind = match (d.kind, c.kind) {
                (DefKind::Enum, _) => DefKind::Const,
                (_, k) => k,
            };
            let mut c2 = c.clone();
            c2.kind = c_kind;
            self.def(&c2, Some(&me));
        }
    }

    fn keywords(&mut self, ks: &[Keyword]) {
        for k in ks {
            for a in &k.args {
                if let Some(e) = &a.expr {
                    self.expr(e, SymbolKind::Variable);
                }
            }
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        let op = &s.op;
        let code = op.code.as_str();
        let name_of = |v: &[Expr]| {
            v.first()
                .and_then(|e| e.name().map(|n| (n.to_string(), e.pos())))
        };
        let mut f1_done = false;
        let mut f2_done = false;
        let mut result_done = false;
        match code {
            "BEGSR" => {
                if let Some((n, at)) = name_of(&op.f1) {
                    self.decl(&n, SymbolKind::Subroutine, at, "subroutine".into());
                }
                f1_done = true;
            }
            "TAG" => {
                if let Some((n, at)) = name_of(&op.f1) {
                    self.decl(&n, SymbolKind::Label, at, "tag".into());
                }
                f1_done = true;
            }
            "KLIST" | "PLIST" => {
                if let Some((n, at)) = name_of(&op.f1) {
                    self.decl(&n, SymbolKind::List, at, code.to_ascii_lowercase());
                }
                f1_done = true;
            }
            "EXSR" | "GOTO" => {
                let kind = if code == "EXSR" {
                    SymbolKind::Subroutine
                } else {
                    SymbolKind::Label
                };
                for e in &op.f2 {
                    self.expr(e, kind);
                }
                f2_done = true;
            }
            _ if code.starts_with("CAS") || code.starts_with("CAB") => {
                let kind = if code.starts_with("CAS") {
                    SymbolKind::Subroutine
                } else {
                    SymbolKind::Label
                };
                for e in &op.result {
                    self.expr(e, kind);
                }
                result_done = true;
            }
            "DEFINE" => {
                if let Some((n, at)) = name_of(&op.result) {
                    self.decl(&n, SymbolKind::Variable, at, "defined like".into());
                }
                result_done = true;
            }
            _ => {}
        }
        if !result_done && !op.len.is_empty() {
            if let Some((n, at)) = name_of(&op.result) {
                let detail = format!("{}{}", op.len, op.decimals);
                self.decl(&n, SymbolKind::Variable, at, detail);
            }
            result_done = true;
        }
        if !f1_done {
            for e in &op.f1 {
                self.expr(e, SymbolKind::Variable);
            }
        }
        if !f2_done {
            for e in &op.f2 {
                self.expr(e, SymbolKind::Variable);
            }
        }
        if !result_done {
            for e in &op.result {
                self.expr(e, SymbolKind::Variable);
            }
        }
        if let Some(e) = &op.xf2 {
            self.expr(e, SymbolKind::Variable);
        }
        for (_, e) in &op.more {
            self.expr(e, SymbolKind::Variable);
        }
    }

    fn expr(&mut self, e: &Expr, kind: SymbolKind) {
        match e {
            Expr::Ref { name, steps, at } => {
                self.reference(name, kind, *at);
                for s in steps {
                    match s {
                        Step::Args(args) => {
                            for a in args {
                                self.expr(a, SymbolKind::Variable);
                            }
                        }
                        Step::Field(f) => self.reference(f, SymbolKind::Subfield, *at),
                    }
                }
            }
            Expr::Special { index, .. } => {
                if let Some(i) = index {
                    self.expr(i, SymbolKind::Variable);
                }
            }
            Expr::Bif { args, .. } | Expr::List(args, _) => {
                for a in args {
                    self.expr(a, SymbolKind::Variable);
                }
            }
            Expr::Unary(_, a, _) => self.expr(a, SymbolKind::Variable),
            Expr::Binary(_, a, b, _) | Expr::Assign(_, a, b, _) => {
                self.expr(a, SymbolKind::Variable);
                self.expr(b, SymbolKind::Variable);
            }
            Expr::Lit(..) | Expr::Fig(..) | Expr::Empty(_) => {}
        }
    }
}
