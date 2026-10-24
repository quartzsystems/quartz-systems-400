// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CRTSQLRPGI`'s side of the compile: the SQLCA declared as IBM's
//! precompiler declares it, the `EXEC SQL` statements handed to the
//! precompiler (`rpgsqlpre`) with the symbols as its host variables
//! and the files as its tables, and its diagnostics and generated
//! lines taken into the compile.

use rpgfront::ast::{Module, Pos, Stmt};
use rpgfront::reader::Options as ReadOptions;
use rpgfront::{Diag, MemIncludes, Member};
use rpgsqlpre::{Env, HostInfo, Precompiled, Source};

use crate::decl::{self, Cx};
use crate::files::ExtFiles;
use crate::syms::{Kind, Table};
use crate::types::Ty;

/// The SQLCA, with IBM's names, layout and short names.
const SQLCA: &str = "**FREE
dcl-ds SQLCA;
  SQLCAID char(8) inz(x'0000000000000000');
  SQLAID char(8) overlay(SQLCAID);
  SQLCABC int(10);
  SQLABC bindec(9) overlay(SQLCABC);
  SQLCODE int(10);
  SQLCOD bindec(9) overlay(SQLCODE);
  SQLERRML int(5);
  SQLERL bindec(4) overlay(SQLERRML);
  SQLERRMC char(70);
  SQLERM char(70) overlay(SQLERRMC);
  SQLERRP char(8);
  SQLERP char(8) overlay(SQLERRP);
  SQLERR char(24);
  SQLER1 bindec(9) overlay(SQLERR : *next);
  SQLER2 bindec(9) overlay(SQLERR : *next);
  SQLER3 bindec(9) overlay(SQLERR : *next);
  SQLER4 bindec(9) overlay(SQLERR : *next);
  SQLER5 bindec(9) overlay(SQLERR : *next);
  SQLER6 bindec(9) overlay(SQLERR : *next);
  SQLERRD int(10) dim(6) overlay(SQLERR);
  SQLWRN char(11);
  SQLWN0 char(1) overlay(SQLWRN : *next);
  SQLWN1 char(1) overlay(SQLWRN : *next);
  SQLWN2 char(1) overlay(SQLWRN : *next);
  SQLWN3 char(1) overlay(SQLWRN : *next);
  SQLWN4 char(1) overlay(SQLWRN : *next);
  SQLWN5 char(1) overlay(SQLWRN : *next);
  SQLWN6 char(1) overlay(SQLWRN : *next);
  SQLWN7 char(1) overlay(SQLWRN : *next);
  SQLWN8 char(1) overlay(SQLWRN : *next);
  SQLWN9 char(1) overlay(SQLWRN : *next);
  SQLWNA char(1) overlay(SQLWRN : *next);
  SQLWARN char(1) dim(11) overlay(SQLWRN);
  SQLSTATE char(5);
  SQLSTT char(5) overlay(SQLSTATE);
end-ds;
";

/// The SQLDA of `INCLUDE SQLDA`, with IBM's names and layout: the
/// program defines the constant `SQL_NUM`, the number of entries.
const SQLDA: &str = "**FREE
dcl-ds SQLDA;
  SQLDAID char(8);
  SQLDABC bindec(9);
  SQLN bindec(4);
  SQLD bindec(4);
  SQL_VAR char(80) dim(SQL_NUM);
end-ds;
dcl-ds SQLVAR;
  SQLTYPE bindec(4);
  SQLLEN bindec(4);
  SQLRES char(12);
  SQLDATA pointer;
  SQLIND pointer;
  SQLNAMELEN bindec(4);
  SQLNAME char(30);
end-ds;
";

/// Declare the SQLCA in the global scope.
pub fn declare_sqlca(t: &mut Table, diags: &mut Vec<Diag>, ctl: &crate::ctl::Ctl) {
    declare_text(t, diags, ctl, "SQLCA", SQLCA);
}

/// Declare the SQLDA in the global scope (`INCLUDE SQLDA`).
pub fn declare_sqlda(t: &mut Table, diags: &mut Vec<Diag>, ctl: &crate::ctl::Ctl) {
    declare_text(t, diags, ctl, "SQLDA", SQLDA);
}

/// Declare generated definitions in the global scope.
fn declare_text(t: &mut Table, diags: &mut Vec<Diag>, ctl: &crate::ctl::Ctl, name: &str, text: &str) {
    let m = Member::from_text("QTEMP", "QSQLSRC", name, text);
    let (_, module, _) = rpgfront::front(m, &mut MemIncludes::default(), &ReadOptions::default());
    let mut quiet = Vec::new();
    let mut cx = Cx {
        t,
        diags: &mut quiet,
        scope: 0,
        prefix: String::new(),
        datfmt: ctl.datfmt,
        timfmt: ctl.timfmt,
        datsep: ctl.datsep.unwrap_or(ctl.datfmt.sep()),
        timsep: ctl.timsep.unwrap_or(ctl.timfmt.sep()),
    };
    for d in &module.defs {
        let mut d = d.clone();
        unplace(&mut d);
        decl::declare(&mut cx, &d);
    }
    // A clash with the program's own names is the program's error.
    diags.extend(quiet.into_iter().filter(|d| d.severity > 20));
}

/// Generated definitions have no source line.
fn unplace(d: &mut rpgfront::ast::Def) {
    d.at = Pos::default();
    for c in &mut d.children {
        unplace(c);
    }
}

/// The `EXEC SQL` statements of some calculations, with their scope.
fn collect(calcs: &[Stmt], scope: usize, out: &mut Vec<Source>) {
    for s in calcs {
        if s.op.code == "EXEC SQL" {
            let text = match s.op.f2.first() {
                Some(rpgfront::ast::Expr::Lit(rpgfront::ast::Lit::Char(t), _)) => t.clone(),
                _ => String::new(),
            };
            out.push(Source {
                line: s.at.line,
                text,
                scope,
            });
        }
    }
}

/// The compiler's symbols and files as the precompiler sees them.
struct HostEnv<'a> {
    t: &'a Table,
    ext: &'a mut dyn ExtFiles,
}

impl HostEnv<'_> {
    fn named_fields(&self, di: usize) -> u32 {
        let n = self.t.ds[di]
            .subfields
            .iter()
            .filter(|&&s| {
                let n = &self.t.syms[s].name;
                !n.is_empty() && n != "*N"
            })
            .count();
        u32::try_from(n).unwrap_or(0)
    }
}

impl Env for HostEnv<'_> {
    fn host(&self, scope: usize, name: &str) -> Option<HostInfo> {
        let (base, sub) = match name.split_once('.') {
            Some((a, b)) => (a, Some(b)),
            None => (name, None),
        };
        let id = self.t.find(scope, base)?;
        let s = &self.t.syms[id];
        if !matches!(s.kind, Some(Kind::Field | Kind::Subfield | Kind::Param | Kind::Ds(_))) {
            return None;
        }
        let info = |ty: &Ty, dim: u32| HostInfo {
            fields: match ty {
                Ty::Ds(di) => self.named_fields(*di),
                _ => 0,
            },
            dim,
        };
        match sub {
            None => Some(info(&s.ty, s.dim)),
            Some(f) => {
                let Ty::Ds(di) = s.ty else { return None };
                let sid = self.t.subfield(di, f)?;
                let x = &self.t.syms[sid];
                Some(info(&x.ty, x.dim))
            }
        }
    }

    fn table(&mut self, schema: Option<&str>, name: &str) -> bool {
        self.ext.describe(name, schema.unwrap_or("*LIBL")).is_some()
    }
}

/// Each scope's first calculation: IBM's precompiler puts its
/// declarations before it (the SQLCA before the main source's, or the
/// first procedure's when there is none).
#[must_use]
pub fn first_calcs(m: &Module, scopes: &[usize]) -> Vec<(usize, u32)> {
    let main = m
        .calcs
        .first()
        .map(|s| s.at.line)
        .or_else(|| m.procs.iter().find_map(|p| p.calcs.first().map(|s| s.at.line)))
        .unwrap_or(0);
    let mut out = vec![(0, main)];
    for (p, &scope) in m.procs.iter().zip(scopes) {
        if let Some(s) = p.calcs.first() {
            out.push((scope, s.at.line));
        }
    }
    out
}

/// Precompile a module's statements: `scopes` are the procedures'
/// symbol scopes, in order.
pub fn precompile(m: &Module, t: &Table, scopes: &[usize], ext: &mut dyn ExtFiles, opts: &rpgsqlpre::Options) -> Precompiled {
    let mut srcs = Vec::new();
    collect(&m.calcs, 0, &mut srcs);
    for (p, &scope) in m.procs.iter().zip(scopes) {
        collect(&p.calcs, scope, &mut srcs);
    }
    srcs.sort_by_key(|s| s.line);
    let mut env = HostEnv { t, ext };
    rpgsqlpre::precompile(&srcs, &mut env, opts)
}

/// The precompiler's diagnostics as the compile's.
#[must_use]
pub fn diags(p: &Precompiled) -> Vec<Diag> {
    p.diags
        .iter()
        .map(|d| {
            let mut x = Diag::new(d.msgid, d.sev, Pos { line: d.line, col: 0 });
            for a in &d.args {
                x = x.arg(a.clone());
            }
            x
        })
        .collect()
}

/// The precompile listing of a compile: the main member's records and
/// those of the members an SQL `INCLUDE` brings (a member that is not
/// fully free set off with `**END-FREE` and `**FREE` in one that is).
#[must_use]
pub fn precompile_listing(c: &crate::Compiled, p: &Precompiled, opts: &crate::Options, ext: &mut dyn ExtFiles) -> Vec<String> {
    use rpgsqlpre::listing::Record;
    let mut records: Vec<Record> = Vec::new();
    let mut at: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut wrapped: Option<usize> = None;
    let marker = |text: &str, free: bool| Record { text: text.into(), seq: "000000".into(), free };
    for l in &c.read.lines {
        let inc = c.read.sql_includes.iter().find(|i| i.member == Some(l.member) && l.member > 0);
        if l.depth > 0 && inc.is_none() {
            continue;
        }
        if wrapped.is_some_and(|m| m != l.member) {
            records.push(marker("**FREE", false));
            wrapped = None;
        }
        if let Some(i) = inc
            && wrapped.is_none()
            && c.module.fully_free
            && !i.fully_free
        {
            records.push(marker("**END-FREE", true));
            wrapped = Some(l.member);
        }
        records.push(Record {
            text: l.text.clone(),
            seq: l.seq.clone(),
            free: l.fully_free,
        });
        at.insert(l.no, u32::try_from(records.len()).unwrap_or(0));
    }
    if wrapped.is_some() {
        records.push(marker("**FREE", true));
    }
    // The **FREE that ends an included member lists as free form.
    for r in &mut records {
        if r.text == "**FREE" && r.seq == "000000" {
            r.free = true;
        }
    }
    let record_of = |line: u32| at.get(&line).copied().unwrap_or(line);
    let mut srcs = Vec::new();
    collect(&c.module.calcs, 0, &mut srcs);
    for p in &c.module.procs {
        collect(&p.calcs, 1, &mut srcs);
    }
    let xref = crate::listing::sql_xref(c, p, ext, &record_of);
    let command = opts.sql.clone().unwrap_or_default();
    let object = format!("{}/{}", opts.lib, opts.name);
    let h = rpgsqlpre::listing::Heading {
        source_file: format!("{}/{}", opts.src.0, opts.src.1),
        member: if opts.src.2 == opts.name { "*OBJ".into() } else { opts.src.2.clone() },
        objtype: if opts.command == crate::Command::CrtRpgMod { "*MODULE" } else { "*PGM" }.into(),
        release: "V7R5M0".into(),
        options: command,
        object,
        statements: srcs.len(),
        set_options: srcs
            .iter()
            .filter(|s| {
                let up = s.text.to_ascii_uppercase();
                let mut w = up.split_whitespace();
                w.next() == Some("SET") && w.next() == Some("OPTION")
            })
            .map(|s| s.line)
            .collect(),
    };
    rpgsqlpre::listing::listing(p, &h, &records, &xref, &record_of)
}
