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
dcl-c SQFRD 2;
dcl-c SQFCRT 8;
dcl-c SQFOVR 16;
dcl-c SQFAPP 32;
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
            sqltype: 0,
        };
        match sub {
            // A LOB or file reference structure stands for one value.
            None if self.t.sqltypes.contains_key(&id) => Some(HostInfo {
                fields: 0,
                dim: s.dim,
                sqltype: self.t.sqltypes[&id],
            }),
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

/// The definitions IBM's precompiler writes for `SQLTYPE(type[: n])`
/// (settled on pub400, `tools/conformance/rpg/b11sql/SQL060`), as
/// free-form source; `None` for a type it does not know.
#[must_use]
pub fn sqltype_source(name: &str, ty: &str, len: Option<&str>) -> Option<String> {
    let n = len.unwrap_or("");
    let ds = |fields: &[String]| {
        let mut s = format!("**FREE\ndcl-ds {name};\n");
        for f in fields {
            s.push_str("  ");
            s.push_str(f);
            s.push('\n');
        }
        s.push_str("end-ds;\n");
        s
    };
    let lob = |data: &str| ds(&[format!("{name}_LEN uns(10);"), format!("{name}_DATA {data};")]);
    let file = || ds(&[format!("{name}_NL uns(10);"), format!("{name}_DL uns(10);"), format!("{name}_FO uns(10);"), format!("{name}_NAME char(255);")]);
    let one = |t: &str| format!("**FREE\ndcl-s {name} {t};\n");
    Some(match ty {
        "CLOB" | "XML_CLOB" if !n.is_empty() => lob(&format!("char({n})")),
        "BLOB" | "XML_BLOB" if !n.is_empty() => lob(&format!("char({n}) ccsid(*hex)")),
        "DBCLOB" | "XML_DBCLOB" if !n.is_empty() => lob(&format!("graph({n})")),
        "CLOB_LOCATOR" | "BLOB_LOCATOR" | "DBCLOB_LOCATOR" | "XML_LOCATOR" => one("uns(10)"),
        "CLOB_FILE" | "BLOB_FILE" | "DBCLOB_FILE" | "XML_CLOB_FILE" | "XML_BLOB_FILE" | "XML_DBCLOB_FILE" => file(),
        "BINARY" if !n.is_empty() => one(&format!("char({n}) ccsid(*hex)")),
        "VARBINARY" if !n.is_empty() => one(&format!("varchar({n}) ccsid(*hex)")),
        "ROWID" => one("varchar(40) ccsid(*hex)"),
        "RESULT_SET_LOCATOR" => one("int(20)"),
        _ => return None,
    })
}

/// What kind of SQL value an `SQLTYPE` host variable holds, for the
/// runtime: 1 a large object, 2 a locator, 3 a file reference; with the
/// large object's kind (1 character, 2 binary, 3 double-byte) above.
#[must_use]
pub fn sqltype_kind(ty: &str) -> i64 {
    let of = |t: &str| {
        if t.contains("DBCLOB") {
            3
        } else if t.contains("BLOB") {
            2
        } else {
            1
        }
    };
    if ty.ends_with("_LOCATOR") && ty != "RESULT_SET_LOCATOR" {
        2 | (of(ty) << 4)
    } else if ty.ends_with("_FILE") {
        3 | (of(ty) << 4)
    } else if ty.contains("LOB") {
        1 | (of(ty) << 4)
    } else {
        0
    }
}

/// Declare an `SQLTYPE` definition as the definitions it stands for;
/// its other keywords (`DIM`, `INZ`, ...) go with the first.
pub fn declare_sqltype(cx: &mut Cx<'_>, d: &rpgfront::ast::Def, k: &rpgfront::ast::Keyword) -> Option<crate::syms::SymId> {
    let text = |i: usize| k.arg(i).map(|a| a.toks.iter().map(rpgfront::lex::Tok::text).collect::<String>().to_ascii_uppercase());
    let (ty, len) = (text(0).unwrap_or_default(), text(1));
    let name = d.name.to_ascii_uppercase();
    let Some(src) = sqltype_source(&name, ty.trim(), len.as_deref().map(str::trim)) else {
        let mut g = Diag::new(crate::diag::ids::BAD_KEYWORD_VALUE, 30, k.at);
        g.args.push(format!("SQLTYPE({ty})"));
        cx.diags.push(g);
        return None;
    };
    let m = Member::from_text("QTEMP", "QSQLSRC", &name, &src);
    let (_, module, _) = rpgfront::front(m, &mut MemIncludes::default(), &ReadOptions::default());
    let mut last = None;
    for (i, g) in module.defs.iter().enumerate() {
        let mut g = g.clone();
        place(&mut g, d.at);
        if i == 0 {
            g.kws.extend(d.kws.iter().filter(|x| x.name != "SQLTYPE").cloned());
        }
        last = decl::declare(cx, &g);
    }
    if let Some(id) = last {
        let kind = sqltype_kind(ty.trim());
        if kind != 0 {
            cx.t.sqltypes.insert(id, kind);
        }
    }
    last
}

/// Generated definitions take the line of the definition they replace.
fn place(d: &mut rpgfront::ast::Def, at: Pos) {
    d.at = at;
    for c in &mut d.children {
        place(c, at);
    }
}
