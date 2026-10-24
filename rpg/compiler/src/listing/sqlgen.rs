// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The member IBM's SQL precompiler writes and hands the compiler
//! (`QTEMP/QSQLTEMP1`), which the `CRTSQLRPGI` listing lists: the
//! source with each `EXEC SQL` statement made a comment and the code
//! that calls the runtime after it, the SQLCA, the runtime's
//! prototypes and a structure per statement before each scope's first
//! calculation. Only the listing uses it; the program is compiled from
//! the statements as the precompiler reads them (`crate::sql`).

use rpgfront::reader::{Class, Line};
use rpgfront::source::{Member, Record};
use rpgsqlpre::{HostRef, Op, Rows, Source, Stmt};

use crate::compile::Compiled;
use crate::syms::Kind;
use crate::types::Ty;

/// The SQLCA, the runtime's prototypes and constants: code and comment.
const SQLCA: &[(&str, &str)] = &[
    ("       /SET CCSID(*CHAR:*JOBRUNMIX)", ""),
    ("       // SQL COMMUNICATION AREA", "//SQL"),
    ("       DCL-DS SQLCA;", "//SQL"),
    ("         SQLCAID CHAR(8) INZ(X'0000000000000000');", "//SQL"),
    ("          SQLAID CHAR(8) OVERLAY(SQLCAID);", "//SQL"),
    ("         SQLCABC INT(10);", "//SQL"),
    ("          SQLABC BINDEC(9) OVERLAY(SQLCABC);", "//SQL"),
    ("         SQLCODE INT(10);", "//SQL"),
    ("          SQLCOD BINDEC(9) OVERLAY(SQLCODE);", "//SQL"),
    ("         SQLERRML INT(5);", "//SQL"),
    ("          SQLERL BINDEC(4) OVERLAY(SQLERRML);", "//SQL"),
    ("         SQLERRMC CHAR(70);", "//SQL"),
    ("          SQLERM CHAR(70) OVERLAY(SQLERRMC);", "//SQL"),
    ("         SQLERRP CHAR(8);", "//SQL"),
    ("          SQLERP CHAR(8) OVERLAY(SQLERRP);", "//SQL"),
    ("         SQLERR CHAR(24);", "//SQL"),
    ("          SQLER1 BINDEC(9) OVERLAY(SQLERR:*NEXT);", "//SQL"),
    ("          SQLER2 BINDEC(9) OVERLAY(SQLERR:*NEXT);", "//SQL"),
    ("          SQLER3 BINDEC(9) OVERLAY(SQLERR:*NEXT);", "//SQL"),
    ("          SQLER4 BINDEC(9) OVERLAY(SQLERR:*NEXT);", "//SQL"),
    ("          SQLER5 BINDEC(9) OVERLAY(SQLERR:*NEXT);", "//SQL"),
    ("          SQLER6 BINDEC(9) OVERLAY(SQLERR:*NEXT);", "//SQL"),
    ("          SQLERRD INT(10) DIM(6) OVERLAY(SQLERR);", "//SQL"),
    ("         SQLWRN CHAR(11);", "//SQL"),
    ("          SQLWN0 CHAR(1) OVERLAY(SQLWRN:*NEXT);", "//SQL"),
    ("          SQLWN1 CHAR(1) OVERLAY(SQLWRN:*NEXT);", "//SQL"),
    ("          SQLWN2 CHAR(1) OVERLAY(SQLWRN:*NEXT);", "//SQL"),
    ("          SQLWN3 CHAR(1) OVERLAY(SQLWRN:*NEXT);", "//SQL"),
    ("          SQLWN4 CHAR(1) OVERLAY(SQLWRN:*NEXT);", "//SQL"),
    ("          SQLWN5 CHAR(1) OVERLAY(SQLWRN:*NEXT);", "//SQL"),
    ("          SQLWN6 CHAR(1) OVERLAY(SQLWRN:*NEXT);", "//SQL"),
    ("          SQLWN7 CHAR(1) OVERLAY(SQLWRN:*NEXT);", "//SQL"),
    ("          SQLWN8 CHAR(1) OVERLAY(SQLWRN:*NEXT);", "//SQL"),
    ("          SQLWN9 CHAR(1) OVERLAY(SQLWRN:*NEXT);", "//SQL"),
    ("          SQLWNA CHAR(1) OVERLAY(SQLWRN:*NEXT);", "//SQL"),
    ("          SQLWARN CHAR(1) DIM(11) OVERLAY(SQLWRN);", "//SQL"),
    ("         SQLSTATE CHAR(5);", "//SQL"),
    ("          SQLSTT CHAR(5) OVERLAY(SQLSTATE);", "//SQL"),
    ("       END-DS SQLCA;", "//SQL"),
    ("       DCL-PR SQLROUTE_CALL EXTPGM(SQLROUTE);", "//SQL"),
    ("         CA LIKEDS(SQLCA);", "//SQL"),
    ("         *N BINDEC(4) OPTIONS(*NOPASS);", "//SQL"),
    ("         *N CHAR(1) OPTIONS(*NOPASS);", "//SQL"),
    ("       END-PR SQLROUTE_CALL;", "//SQL"),
    ("       DCL-PR SQLOPEN_CALL EXTPGM(SQLOPEN);", "//SQL"),
    ("         CA LIKEDS(SQLCA);", "//SQL"),
    ("         *N BINDEC(4);", "//SQL"),
    ("       END-PR SQLOPEN_CALL;", "//SQL"),
    ("       DCL-PR SQLCLSE_CALL EXTPGM(SQLCLSE);", "//SQL"),
    ("         CA LIKEDS(SQLCA);", "//SQL"),
    ("         *N BINDEC(4);", "//SQL"),
    ("       END-PR SQLCLSE_CALL;", "//SQL"),
    ("       DCL-PR SQLCMIT_CALL EXTPGM(SQLCMIT);", "//SQL"),
    ("         CA LIKEDS(SQLCA);", "//SQL"),
    ("         *N BINDEC(4);", "//SQL"),
    ("       END-PR SQLCMIT_CALL;", "//SQL"),
    ("       /RESTORE CCSID(*CHAR)", ""),
    ("       DCL-C SQLROUTE CONST('QSYS/QSQROUTE');", "//SQL"),
    ("       DCL-C SQLOPEN CONST('QSYS/QSQROUTE');", "//SQL"),
    ("       DCL-C SQLCLSE CONST('QSYS/QSQLCLSE');", "//SQL"),
    ("       DCL-C SQLCMIT CONST('QSYS/QSQLCMIT');", "//SQL"),
    ("       DCL-C SQFRD CONST(2);", "//SQL"),
    ("       DCL-C SQFCRT CONST(8);", "//SQL"),
    ("       DCL-C SQFOVR CONST(16);", "//SQL"),
    ("       DCL-C SQFAPP CONST(32);", "//SQL"),
];

/// `INCLUDE SQLDA`: the descriptor areas.
const SQLDA: &[(&str, &str)] = &[
    ("       /SET CCSID(*CHAR:*JOBRUNMIX)", ""),
    ("       // SQL DESCRIPTOR AREA", "//SQL"),
    ("       DCL-DS SQLDA;", "//SQL"),
    ("         SQLDAID CHAR(8);", "//SQL"),
    ("         SQLDABC INT(10);", "//SQL"),
    ("         SQLN INT(5);", "//SQL"),
    ("         SQLD INT(5);", "//SQL"),
    ("         SQL_VAR CHAR(80) DIM(SQL_NUM);", "//SQL"),
    ("          *N POINTER OVERLAY(SQL_VAR:17);", "//SQL"),
    ("          *N POINTER OVERLAY(SQL_VAR:33);", "//SQL"),
    ("       END-DS SQLDA;", "//SQL"),
    ("       DCL-DS SQLVAR;", "//SQL"),
    ("         SQLTYPE INT(5);", "//SQL"),
    ("         SQLLEN INT(5);", "//SQL"),
    ("         SQLRES CHAR(12);", "//SQL"),
    ("         SQLINFO1 CHAR(16);", "//SQL"),
    ("          SQLDATA POINTER OVERLAY(SQLINFO1);", "//SQL"),
    ("          SQL_RESULT_SET_LOCATOR INT(20) OVERLAY(SQLINFO1);", "//SQL"),
    ("         SQLINFO2 CHAR(16);", "//SQL"),
    ("          SQLIND POINTER OVERLAY(SQLINFO2);", "//SQL"),
    ("          SQL_ROW_CHANGE INT(10) OVERLAY(SQLINFO2);", "//SQL"),
    ("          SQL_RESULT_SET_ROWS INT(10) OVERLAY(SQLINFO2);", "//SQL"),
    ("         SQLNAMELEN INT(5);", "//SQL"),
    ("         SQLNAME CHAR(30);", "//SQL"),
    ("       END-DS SQLVAR;", "//SQL"),
    ("       // EXTENDED SQLDA", "//SQL"),
    ("       DCL-DS SQLVAR2;", "//SQL"),
    ("         SQLLONGL INT(10);", "//SQL"),
    ("         SQLRSVDL CHAR(28);", "//SQL"),
    ("         SQLDATAL POINTER;", "//SQL"),
    ("         SQLTNAMELN INT(5);", "//SQL"),
    ("         SQLTNAME CHAR(30);", "//SQL"),
    ("       END-DS SQLVAR2;", "//SQL"),
    ("       /RESTORE CCSID(*CHAR)", ""),
];

const SQL: &str = "//SQL";

/// A line of code with its comment at column 81.
fn code(text: &str, cmt: &str) -> String {
    if cmt.is_empty() { text.to_string() } else { format!("{text:<80}{cmt}") }
}

/// A host variable's field in a statement's structure.
#[derive(Clone)]
struct Field {
    /// As the generated code names it (`REC.NM`, `INDS(1)`).
    name: String,
    ty: Ty,
}

/// A fixed-form subfield of a structure: from, to, type and
/// decimals, keywords, comment.
fn subfield(n: u32, from: u32, to: u32, ty: &str, kws: &str, cmt: &str) -> String {
    let head = format!("     D  SQL_{n:05}");
    format!("{head:<25}{from:>7}{to:>7}{ty:<3} {kws:<37}{cmt}")
}

/// `PACKEVEN` for an even number of digits.
fn even(d: u8) -> String {
    if d.is_multiple_of(2) { "PACKEVEN".into() } else { String::new() }
}

fn int_len(d: u8) -> u32 {
    match d {
        3 => 1,
        5 => 2,
        20 => 8,
        _ => 4,
    }
}

/// How a field is laid out: its length, its type column (type and
/// decimals) and its keywords.
fn layout(ty: &Ty, size: u32) -> (u32, String, String) {
    let jobrun = "CCSID(*JOBRUNMIX)";
    match ty {
        Ty::Int(d) => (int_len(*d), "I 0".into(), String::new()),
        // An unsigned integer is packed.
        Ty::Uns(d) => (u32::from(*d) / 2 + 1, "P 0".into(), even(*d)),
        Ty::Packed(d, s) => (u32::from(*d) / 2 + 1, format!("P{s:>2}"), even(*d)),
        Ty::Zoned(d, s) => (u32::from(*d), format!("S{s:>2}"), String::new()),
        Ty::Bin(d, s) => (if *d <= 4 { 2 } else { 4 }, format!("B{s:>2}"), String::new()),
        Ty::Float(b) => (u32::from(*b), "F".into(), String::new()),
        Ty::Char(n) => (*n, "A".into(), jobrun.into()),
        Ty::VarChar(n, p) => (n + u32::from(*p), "A".into(), format!("VARYING{} {jobrun}", if *p == 4 { "(4)" } else { "" })),
        // The job's date and time formats, the oracle's.
        Ty::Date(..) => (8, "D".into(), "DATFMT(*YMD-)".into()),
        Ty::Time(..) => (8, "T".into(), "TIMFMT(*HMS:)".into()),
        Ty::Timestamp(_) => (26, "Z".into(), String::new()),
        _ => (size, "A".into(), jobrun.into()),
    }
}

/// One `EXEC SQL` statement of the source.
struct Sql<'a> {
    src: &'a Source,
    /// Its last line.
    last: u32,
    /// Its first word.
    verb: String,
    stmt: Option<&'a Stmt>,
    /// The statement number.
    no: u32,
}

/// The names of a structure's header fields and fields.
#[derive(Clone, Default)]
struct Header {
    len: u32,
    mark: u32,
    okay: u32,
    fields: Vec<u32>,
}

/// What a statement declares and calls.
#[derive(Clone)]
enum Shape {
    /// No code: `DECLARE`, `WHENEVER`, `SET OPTION`, `INCLUDE`.
    None,
    /// No structure: `SQLER6 = n` and the call with the SQLCA.
    Static,
    /// A structure: inputs, outputs.
    Struct(Vec<Field>, Vec<Field>),
    /// `EXECUTE ... USING`: the values go in and come back.
    Execute(Vec<Field>),
    /// A free-form structure and `SQCALLn`: the extra parameters and
    /// their arguments.
    FreeStruct(Vec<String>, Vec<String>),
    /// `SQCALLn` with host variables as its arguments.
    Call(Vec<String>),
}

/// What the generator writes.
struct Gen<'a> {
    c: &'a Compiled,
    out: Vec<Record>,
    /// The next `SQL_nnnnn`.
    next: u32,
    /// A structure without host variables has had its header split:
    /// the ones after keep the shorter header.
    split: bool,
    free: bool,
}

impl Gen<'_> {
    fn push(&mut self, seq: &str, text: String) {
        self.out.push(Record {
            seq: seq.to_string(),
            date: String::new(),
            text,
        });
    }

    fn name(&mut self) -> u32 {
        let n = self.next;
        self.next += 1;
        n
    }

    /// The fields a host variable stands for: a structure's subfields,
    /// an array's elements (indicators), or itself.
    fn fields(&self, scope: usize, h: &HostRef) -> Vec<Field> {
        let mut v = self.expand(scope, &h.name);
        if let Some(i) = &h.ind {
            v.extend(self.expand(scope, i));
        }
        v
    }

    fn expand(&self, scope: usize, name: &str) -> Vec<Field> {
        let t = &self.c.table;
        let up = name.to_ascii_uppercase();
        let unknown = || vec![Field { name: up.clone(), ty: Ty::Char(1) }];
        let (base, sub) = match up.split_once('.') {
            Some((a, b)) => (a.to_string(), Some(b.to_string())),
            None => (up.clone(), None),
        };
        let Some(id) = t.find(scope, &base) else { return unknown() };
        let sym = match (&sub, &t.syms[id].ty) {
            (None, _) => id,
            (Some(f), Ty::Ds(di)) => match t.subfield(*di, f) {
                Some(x) => x,
                None => return unknown(),
            },
            (Some(_), _) => return unknown(),
        };
        let x = &t.syms[sym];
        let di = match (&x.kind, &x.ty) {
            (Some(Kind::Ds(di)), _) | (_, Ty::Ds(di)) => Some(*di),
            _ => None,
        };
        match di {
            Some(di) if sub.is_none() && x.dim == 0 => {
                let qualified = t.ds.get(di).is_some_and(|d| d.qualified);
                t.ds[di]
                    .subfields
                    .iter()
                    .map(|&f| &t.syms[f])
                    .filter(|f| !f.name.is_empty() && f.name != "*N")
                    .map(|f| Field {
                        name: if qualified { format!("{up}.{}", f.name) } else { f.name.clone() },
                        ty: f.ty.clone(),
                    })
                    .collect()
            }
            _ if x.dim > 0 => (1..=x.dim).map(|k| Field { name: format!("{up}({k})"), ty: x.ty.clone() }).collect(),
            _ => vec![Field { name: up.clone(), ty: x.ty.clone() }],
        }
    }

    fn size(&self, ty: &Ty) -> u32 {
        ty.size(&|i| self.c.table.ds_len(i))
    }

    /// A statement's fixed-form structure: its header (the names of
    /// the length, the mark, the data flag) and each field's name.
    fn structure(&mut self, seq: &str, verb: &str, no: u32, fields: &[Field], scope: usize) -> Header {
        // COMMIT and ROLLBACK mark the data flag.
        let flag = match verb {
            "COMMIT" => '3',
            "ROLLBACK" => '5',
            _ => '0',
        };
        let mut lines = Vec::new();
        let kw = if scope > 0 { "STATIC" } else { "" };
        lines.push(format!("{:<80}{verb}", format!("     D                 DS                  {kw}")));
        let len = self.name();
        lines.push(subfield(len, 1, 2, "B 0", "INZ(128)", "length of header"));
        let stmt = self.name();
        lines.push(subfield(stmt, 3, 4, "B 0", &format!("INZ({no})"), "statement number"));
        let mark = self.name();
        lines.push(subfield(mark, 5, 8, "U 0", "INZ(0)", "invocation mark"));
        let okay = self.name();
        lines.push(subfield(okay, 9, 9, "A", &format!("INZ('{flag}') CCSID(*JOBRUNMIX)"), "data is okay"));
        let end = self.name();
        let to = if fields.is_empty() || self.split { 127 } else { 128 };
        lines.push(subfield(end, 10, to, "A", "CCSID(*JOBRUNMIX)", "end of header"));
        if fields.is_empty() {
            let n = self.name();
            lines.push(subfield(n, 128, 128, "A", "CCSID(*JOBRUNMIX)", "end of header"));
            self.split = true;
        }
        let mut at = 129;
        let mut names = Vec::new();
        for f in fields {
            let (l, ty, kws) = layout(&f.ty, self.size(&f.ty));
            let n = self.name();
            lines.push(subfield(n, at, at + l - 1, &ty, &kws, &f.name));
            names.push(n);
            at += l;
        }
        if self.free {
            self.push(seq, "**END-FREE".into());
        }
        for l in lines {
            self.push(seq, l);
        }
        if self.free {
            self.push(seq, "**FREE".into());
        }
        Header { len, mark, okay, fields: names }
    }

    /// A free-form structure of the header alone and the prototype of
    /// its call: the parameters after the SQLCA.
    fn free_structure(&mut self, seq: &str, verb: &str, no: u32, extra: &[String]) -> u32 {
        self.push(seq, code("       DCL-DS *N;", &format!("//{verb}")));
        let len = self.name();
        self.push(seq, code(&format!("         SQL_{len:05} BINDEC(4) INZ(128);"), "//length of header"));
        let n = self.name();
        self.push(seq, code(&format!("         SQL_{n:05} BINDEC(4) INZ({no});"), "//statement number"));
        let n = self.name();
        self.push(seq, code(&format!("         SQL_{n:05} UNS(10) INZ(0);"), "//invocation mark"));
        let n = self.name();
        self.push(seq, code(&format!("         SQL_{n:05} CHAR(1) INZ('0') CCSID(*JOBRUNMIX);"), "//data is okay"));
        let n = self.name();
        self.push(seq, code(&format!("         SQL_{n:05} CHAR(118) CCSID(*JOBRUNMIX);"), "//end of header"));
        let n = self.name();
        self.push(seq, code(&format!("         SQL_{n:05} CHAR(1) CCSID(*JOBRUNMIX);"), "//end of header"));
        self.push(seq, code("       END-DS;", &format!("//{verb}")));
        let mut params = vec![format!("LIKE(SQL_{len:05})")];
        params.extend(extra.iter().cloned());
        self.prototype(seq, no, &params);
        len
    }

    /// `SQCALLnnnnnn`'s prototype: the SQLCA, then the parameters.
    fn prototype(&mut self, seq: &str, no: u32, params: &[String]) {
        self.push(seq, code(&format!("       DCL-PR SQCALL{no:06} EXTPGM(SQLROUTE);"), SQL));
        self.push(seq, code("         CA LIKEDS(SQLCA);", SQL));
        for (k, p) in params.iter().enumerate() {
            self.push(seq, code(&format!("         P{:04} {p};", k + 1), SQL));
        }
        self.push(seq, code(&format!("       END-PR SQCALL{no:06};"), SQL));
    }

    /// A call: the program, the SQLCA, the arguments.
    fn call(&mut self, seq: &str, name: &str, args: &[String]) {
        self.push(seq, code(&format!("          {name}("), SQL));
        self.push(seq, code("               SQLCA", SQL));
        for a in args {
            self.push(seq, code(&format!("             : {a}"), SQL));
        }
        self.push(seq, code("          );", SQL));
    }

    fn line(&mut self, seq: &str, text: &str) {
        self.push(seq, code(&format!("          {text}"), SQL));
    }
}

/// The member IBM's precompiler generates for the compile, or `None`
/// when the compile has no SQL.
#[must_use]
pub(crate) fn member(c: &Compiled) -> Option<Member> {
    let p = c.precompiled.as_ref()?;
    let srcs = sources(c);
    let free = c.module.fully_free;
    let mut g = Gen {
        c,
        out: Vec::new(),
        next: 0,
        split: false,
        free,
    };
    // The statements, numbered: cursor declarations first.
    let mut sqls: Vec<Sql> = srcs
        .iter()
        .map(|s| {
            let verb = s.text.split_whitespace().next().unwrap_or("").to_ascii_uppercase();
            let stmt = p.stmts.iter().find(|x| x.line == s.line);
            let breaks = u32::try_from(s.text.trim_end().matches('\n').count()).unwrap_or(0);
            let last = stmt.map_or(s.line + breaks, |x| x.last_line);
            // A fixed-form statement ends at its END-EXEC.
            let last = if fixed_exec(c, s.line) { end_exec(c, s.line).unwrap_or(last) } else { last };
            Sql { src: s, last, verb, stmt, no: 0 }
        })
        .collect();
    let mut no = 0;
    for s in sqls.iter_mut().filter(|s| s.verb == "DECLARE" && is_cursor(&s.src.text)) {
        no += 1;
        s.no = no;
    }
    for s in sqls.iter_mut().filter(|s| !(s.verb == "DECLARE" && is_cursor(&s.src.text))) {
        if s.verb == "INCLUDE" {
            continue;
        }
        no += 1;
        s.no = no;
        if s.verb == "SET" && second_word(&s.src.text) == "OPTION" {
            no += u32::try_from(options(&s.src.text)).unwrap_or(1) - 1;
        }
    }
    // Where each scope's declarations go: before its first
    // calculation (the main source's SQLCA before an earlier INCLUDE).
    let scopes = scope_starts(c);
    let main_at = scopes.first().map_or(0, |(_, l)| *l);
    let shapes: Vec<Shape> = sqls.iter().map(|s| shape(&g, s)).collect();
    let mut headers: Vec<Header> = vec![Header::default(); sqls.len()];
    let mut seqs: Vec<String> = vec![String::new(); sqls.len()];
    let mut prev_seq = String::new();
    let kept: Vec<&Line> = c.read.lines.iter().filter(|l| keep(c, l)).collect();
    // An included member that is not fully free, in a member that is,
    // is set off with `**END-FREE` and `**FREE`.
    let mut wrapped: Option<usize> = None;
    let mut i = 0;
    while i < kept.len() {
        let l = kept[i];
        if wrapped.is_some_and(|m| m != l.member) {
            g.push("000000", "**FREE".into());
            wrapped = None;
        }
        g.free = l.fully_free;
        // The declarations due before this line.
        for (scope, at) in &scopes {
            if *at != l.no {
                continue;
            }
            if *scope == 0 {
                for (t, cm) in SQLCA {
                    g.push(&prev_seq, code(t, cm));
                }
                if c.read.sqlda {
                    for (t, cm) in SQLDA {
                        g.push(&prev_seq, code(t, cm));
                    }
                }
            }
            for (k, s) in sqls.iter().enumerate() {
                if s.src.scope != *scope {
                    continue;
                }
                seqs[k] = stmt_seq(c, s, main_at, &prev_seq);
                headers[k] = declare(&mut g, s, &shapes[k], &seqs[k]);
            }
        }
        // An SQL INCLUDE: its lines as comments, then the member's.
        if let Some(inc) = c.read.sql_includes.iter().find(|x| x.line == l.no) {
            let last = if l.fully_free { l.no } else { end_exec(c, l.no).unwrap_or(l.no) };
            let mut j = i;
            while j < kept.len() && kept[j].no <= last {
                g.push(&kept[j].seq, comment(kept[j]));
                prev_seq.clone_from(&kept[j].seq);
                j += 1;
            }
            if let Some(m) = inc.member
                && l.fully_free
                && !inc.fully_free
            {
                g.push("000000", "**END-FREE".into());
                wrapped = Some(m);
            }
            i = j;
            continue;
        }
        // A statement: its lines as comments, then its code.
        if let Some(k) = sqls.iter().position(|s| s.src.line == l.no) {
            let s = &sqls[k];
            let mut j = i;
            while j < kept.len() && kept[j].no <= s.last {
                g.push(&kept[j].seq, comment(kept[j]));
                prev_seq.clone_from(&kept[j].seq);
                j += 1;
            }
            calls(&mut g, s, &shapes[k], &headers[k], &seqs[k]);
            i = j;
            continue;
        }
        g.push(&l.seq, l.text.clone());
        prev_seq.clone_from(&l.seq);
        i += 1;
    }
    if wrapped.is_some() {
        g.push("000000", "**FREE".into());
    }
    Some(Member {
        lib: "QTEMP".into(),
        file: "QSQLTEMP1".into(),
        name: c.read.members[0].name.clone(),
        ccsid: 37,
        records: g.out,
    })
}

/// The `EXEC SQL` statements, with their scopes (procedures from 1).
pub(crate) fn sources(c: &Compiled) -> Vec<Source> {
    let mut out = Vec::new();
    let calcs = std::iter::once(&c.module.calcs).chain(c.module.procs.iter().map(|p| &p.calcs));
    for (scope, calcs) in calcs.enumerate() {
        for s in calcs.iter().filter(|s| s.op.code == "EXEC SQL") {
            let text = match s.op.f2.first() {
                Some(rpgfront::ast::Expr::Lit(rpgfront::ast::Lit::Char(t), _)) => t.clone(),
                _ => String::new(),
            };
            out.push(Source { line: s.at.line, text, scope });
        }
    }
    out.sort_by_key(|s| s.line);
    out
}

/// Lines of the generated member: the main member's and those of the
/// members an SQL `INCLUDE` brings (a `/COPY` stays a directive).
fn keep(c: &Compiled, l: &Line) -> bool {
    l.depth == 0 || c.read.sql_includes.iter().any(|i| i.member == Some(l.member))
}

/// A statement's line made a comment: `//*` before free-form text (at
/// column 8 in fixed-format source), `C*` for `C/` and `C+`.
fn comment(l: &Line) -> String {
    let t = &l.text;
    if l.fully_free {
        format!("//*{t}")
    } else if matches!(l.class, Class::Free) || !t.get(5..7).is_some_and(|c| c.eq_ignore_ascii_case("C/") || c.eq_ignore_ascii_case("C+")) {
        let at = t.char_indices().nth(7).map_or(t.len(), |(i, _)| i);
        format!("{:<7}//*{}", &t[..at], &t[at..])
    } else {
        let mut ch: Vec<char> = t.chars().collect();
        ch[6] = '*';
        ch.into_iter().collect()
    }
}

/// A statement in fixed form: `C/EXEC SQL`.
fn fixed_exec(c: &Compiled, line: u32) -> bool {
    c.read
        .lines
        .iter()
        .find(|l| l.no == line)
        .is_some_and(|l| l.text.get(5..).is_some_and(|t| t.to_ascii_uppercase().starts_with("C/EXEC")))
}

/// The line of a fixed-form statement's `C/END-EXEC`.
fn end_exec(c: &Compiled, from: u32) -> Option<u32> {
    c.read
        .lines
        .iter()
        .filter(|l| l.no >= from)
        .find(|l| l.text.get(5..).is_some_and(|t| t.to_ascii_uppercase().starts_with("C/END-EXEC")))
        .map(|l| l.no)
}

fn second_word(text: &str) -> String {
    text.split_whitespace().nth(1).unwrap_or("").to_ascii_uppercase()
}

fn is_cursor(text: &str) -> bool {
    let up = text.to_ascii_uppercase();
    up.split_whitespace().take(5).any(|w| w == "CURSOR")
}

/// The options of a `SET OPTION`: one statement number each.
fn options(text: &str) -> usize {
    text.split(',').count().max(1)
}

/// Each scope's first calculation; the main source's SQLCA goes before
/// its first (or the first SQL `INCLUDE` before it, or without main
/// calculations the first procedure).
fn scope_starts(c: &Compiled) -> Vec<(usize, u32)> {
    let m = &c.module;
    let scopes: Vec<usize> = (1..=m.procs.len()).collect();
    let mut firsts = crate::sql::first_calcs(m, &scopes);
    if m.calcs.is_empty()
        && let (Some(p), Some(main)) = (m.procs.first(), firsts.first_mut())
    {
        main.1 = p.at.line;
    }
    if let (Some(inc), Some(main)) = (c.read.sql_includes.first(), firsts.first_mut())
        && (main.1 == 0 || inc.line < main.1)
    {
        main.1 = inc.line;
    }
    firsts
}

/// The sequence number of a statement's declarations and code: its
/// last line's in free form; in fixed form its first line's, or at the
/// SQLCA's place the SQLCA's.
fn stmt_seq(c: &Compiled, s: &Sql<'_>, main_at: u32, prev: &str) -> String {
    let fixed = fixed_exec(c, s.src.line);
    if fixed && s.src.line == main_at {
        return prev.to_string();
    }
    let at = if fixed { s.src.line } else { s.last };
    c.read.lines.iter().find(|l| l.no == at).map(|l| l.seq.clone()).unwrap_or_default()
}

/// What a statement declares and calls.
fn shape(g: &Gen<'_>, s: &Sql<'_>) -> Shape {
    let Some(st) = s.stmt else { return Shape::None };
    let scope = s.src.scope;
    let ins: Vec<Field> = st.inputs.iter().flat_map(|h| g.fields(scope, h)).collect();
    let outs: Vec<Field> = st.outputs.iter().flat_map(|h| g.fields(scope, h)).collect();
    // A statement of a descriptor, and PREPARE and EXECUTE IMMEDIATE,
    // pass their host variables.
    let text = matches!(st.desc.op, Op::Prepare | Op::ExecImm).then(|| st.inputs.first()).flatten();
    let args: Vec<String> = text.into_iter().chain(st.da.as_ref()).map(|h| h.name.to_ascii_uppercase()).collect();
    if !args.is_empty() {
        return Shape::Call(args);
    }
    match st.desc.op {
        Op::Nothing | Op::End => Shape::None,
        Op::Prepare | Op::ExecImm | Op::Describe => Shape::Static,
        Op::Fetch if st.rows.is_some() => {
            let arr = st.outputs.first().map(|h| h.name.to_ascii_uppercase()).unwrap_or_default();
            let dim = g.c.table.find(scope, &arr).map_or(0, |id| g.c.table.syms[id].dim);
            Shape::FreeStruct(vec![format!("LIKE({arr}) DIM({dim})")], vec![arr])
        }
        Op::Fetch if outs.is_empty() => Shape::FreeStruct(Vec::new(), Vec::new()),
        // A RELATIVE count goes in SQLER4, not the structure.
        Op::Fetch if st.desc.orient.starts_with("RELATIVE") => Shape::Struct(Vec::new(), outs),
        Op::Open => Shape::Struct(ins, Vec::new()),
        Op::Close => Shape::Struct(Vec::new(), Vec::new()),
        Op::Exec if matches!(st.verb.as_str(), "COMMIT" | "ROLLBACK") => Shape::Struct(Vec::new(), Vec::new()),
        Op::Execute if ins.is_empty() => Shape::Static,
        Op::Execute => Shape::Execute(ins),
        Op::Exec if ins.is_empty() && outs.is_empty() => Shape::Static,
        _ => Shape::Struct(ins, outs),
    }
}

/// A statement's declarations.
fn declare(g: &mut Gen<'_>, s: &Sql<'_>, shape: &Shape, seq: &str) -> Header {
    match shape {
        Shape::Struct(ins, outs) => {
            let all: Vec<Field> = ins.iter().chain(outs).cloned().collect();
            g.structure(seq, &s.verb, s.no, &all, s.src.scope)
        }
        Shape::Execute(ins) => g.structure(seq, &s.verb, s.no, ins, s.src.scope),
        Shape::FreeStruct(params, _) => {
            let len = g.free_structure(seq, &s.verb, s.no, params);
            Header { len, ..Header::default() }
        }
        Shape::Call(args) => {
            let likes: Vec<String> = args.iter().map(|a| format!("LIKE({a})")).collect();
            g.prototype(seq, s.no, &likes);
            Header::default()
        }
        Shape::None | Shape::Static => Header::default(),
    }
}

/// The code after a statement.
fn calls(g: &mut Gen<'_>, s: &Sql<'_>, shape: &Shape, h: &Header, seq: &str) {
    let Some(st) = s.stmt else { return };
    let hdr = format!("SQL_{:05}", h.len);
    let numbered = |g: &mut Gen<'_>| g.push(seq, code("          SQLER6 = -4;", &format!("{SQL} {}", s.no)));
    match shape {
        Shape::None => return,
        Shape::Static => {
            g.line(seq, &format!("SQLER6 = {};", s.no));
            g.call(seq, "SQLROUTE_CALL", &[]);
        }
        Shape::Call(args) => {
            g.line(seq, &format!("SQLER6 = {};", s.no));
            g.call(seq, &format!("SQCALL{:06}", s.no), args);
        }
        Shape::FreeStruct(_, args) => {
            if let Some(r) = &st.rows {
                let n = match r {
                    Rows::Lit(n) => n.to_string(),
                    Rows::Host(h) => h.name.to_ascii_uppercase(),
                };
                g.line(seq, &format!("SQLER5 = {n};"));
            }
            g.line(seq, "SQLER6 = -4;");
            let mut all = vec![hdr];
            all.extend(args.iter().cloned());
            g.call(seq, &format!("SQCALL{:06}", s.no), &all);
        }
        Shape::Execute(ins) => {
            for (k, f) in ins.iter().enumerate() {
                g.line(seq, &format!("SQL_{:05} = {};", h.fields[k], f.name));
            }
            numbered(g);
            g.call(seq, "SQLROUTE_CALL", std::slice::from_ref(&hdr));
            g.line(seq, &format!("IF SQL_{:05} = '1';", h.okay));
            for (k, f) in ins.iter().enumerate() {
                g.line(seq, &format!("EVAL {} = SQL_{:05};", f.name, h.fields[k]));
            }
            g.line(seq, "ENDIF;");
        }
        Shape::Struct(ins, outs) => match st.desc.op {
            Op::Close => close(g, seq, s.no, h, "SQLCLSE_CALL"),
            Op::Exec if matches!(st.verb.as_str(), "COMMIT" | "ROLLBACK") => close(g, seq, s.no, h, "SQLCMIT_CALL"),
            Op::Open => {
                for (k, f) in ins.iter().enumerate() {
                    g.line(seq, &format!("SQL_{:05} = {};", h.fields[k], f.name));
                }
                g.line(seq, "SQLER6 = -4;");
                g.push(seq, code(&format!("          IF SQL_{:05} = 0", h.mark), SQL));
                g.line(seq, &format!("OR SQL_{:05} <> *LOVAL;", h.okay));
                g.call(seq, "SQLROUTE_CALL", std::slice::from_ref(&hdr));
                g.line(seq, "ELSE;");
                g.call(seq, "SQLOPEN_CALL", std::slice::from_ref(&hdr));
                g.line(seq, "ENDIF;");
            }
            _ => {
                for (k, f) in ins.iter().enumerate() {
                    g.line(seq, &format!("SQL_{:05} = {};", h.fields[k], f.name));
                }
                if st.desc.op == Op::Fetch && st.desc.orient.starts_with("RELATIVE") {
                    g.line(seq, "SQLER3 = 0;");
                    g.line(seq, &format!("SQLER4 = {};", relative(s)));
                }
                numbered(g);
                g.call(seq, "SQLROUTE_CALL", std::slice::from_ref(&hdr));
                if !outs.is_empty() {
                    g.line(seq, &format!("IF SQL_{:05} = '1';", h.okay));
                    for (k, f) in outs.iter().enumerate() {
                        g.line(seq, &format!("EVAL {} = SQL_{:05};", f.name, h.fields[ins.len() + k]));
                    }
                    g.line(seq, "ENDIF;");
                }
            }
        },
    }
    whenever(g, st, seq);
}

/// `CLOSE`, `COMMIT`, `ROLLBACK`: the call, or the runtime's own.
fn close(g: &mut Gen<'_>, seq: &str, no: u32, h: &Header, own: &str) {
    let hdr = format!("SQL_{:05}", h.len);
    g.line(seq, &format!("SQLER6 = {no};"));
    g.line(seq, &format!("IF SQL_{:05} = 0;", h.mark));
    g.call(seq, "SQLROUTE_CALL", std::slice::from_ref(&hdr));
    g.line(seq, "ELSE;");
    g.call(seq, own, std::slice::from_ref(&hdr));
    g.line(seq, "ENDIF;");
}

/// `FETCH RELATIVE n`'s count as `SQLER4` takes it: a signed literal
/// or the host variable.
fn relative(s: &Sql<'_>) -> String {
    let up = s.src.text.to_ascii_uppercase();
    let after = up.split("RELATIVE").nth(1).unwrap_or("").trim_start();
    let word: String = after.chars().take_while(|ch| !ch.is_whitespace()).collect();
    if let Some(h) = word.strip_prefix(':') {
        h.to_string()
    } else if word.starts_with('-') || word.starts_with('+') {
        word
    } else {
        format!("+{word}")
    }
}

/// The `WHENEVER` tests after a statement.
fn whenever(g: &mut Gen<'_>, st: &Stmt, seq: &str) {
    let w = &st.whenever;
    for (label, test) in [
        (&w.error, "IF SQLCOD < 0;"),
        (&w.warning, "IF SQLWN0 = 'W' AND SQLCOD >= 0;"),
        (&w.not_found, "IF SQLCOD = 100 OR SQLSTT = '02000';"),
    ] {
        let Some(label) = label else { continue };
        g.line(seq, test);
        g.push(seq, format!("{:<80}SQL", format!("     C                   GOTO      {}", label.to_ascii_uppercase())));
        g.line(seq, "ENDIF;");
    }
}
