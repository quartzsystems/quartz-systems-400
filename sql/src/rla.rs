// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The scripted record-level driver: one line per operation with the
//! status and values expected, mismatches reported. The conformance
//! vehicle for RLA semantics against the real machine; the console's
//! `rla` command and the conformance kit both run it.
//!
//! ```text
//! OPEN QSTEST/CUST [MBR] [INPUT|UPDATE|OUTPUT|ALL] [KEYED|ARRIVAL]  => 00000
//! SETLL 3 'Bob'      => 00000 FOUND EQUAL
//! READ               => 00000 CUSNO=3 NAME='Bob'
//! CHAIN #4           => 00000                 (#n is a relative record number)
//! WRITE CUSNO=9 NAME='Zed' BAL=1.5  => 00000
//! UPDATE NAME='Zoe'  => 00000
//! DELETE | UNLOCK | FEOD | CLOSE     => 00000
//! ```
//!
//! `READN` and `CHAINN` read without a lock. A line starting with `#` or
//! `--` is a comment.

use std::fmt::Write as _;

use db::odp::{KeyArg, Odp, OpenOpts, Usage};
use db::{DbError, Value as DbValue};
use qdecimal::Decimal;
use qsname::{MemberSpec, Name};
use store::txn::Txn;

use crate::ast::ObjectName;
use crate::exec::dml::build_record;
use crate::exec::{SqlContext, TableInfo};
use crate::value::Value;

/// What a run reported.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// One line per script line, then the summary.
    pub text: String,
    /// Lines that matched.
    pub ok: u32,
    /// Lines that did not.
    pub bad: u32,
    /// What each executed line observed, in script order.
    pub lines: Vec<LineResult>,
}

/// One executed line: the status, the feedback flags and the values of
/// the fields the line asked about.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineResult {
    /// Line number in the script, from 1.
    pub line: u32,
    /// Five-digit status code.
    pub status: String,
    /// `%FOUND`.
    pub found: bool,
    /// `%EQUAL`.
    pub equal: bool,
    /// `%EOF`.
    pub eof: bool,
    /// The fields named after `=>` with the values read.
    pub fields: Vec<(String, String)>,
}

/// What a line expects after `=>`.
#[derive(Default)]
struct Expect {
    status: Option<String>,
    found: Option<bool>,
    equal: Option<bool>,
    eof: Option<bool>,
    fields: Vec<(String, String)>,
}

/// Split a line into words, keeping quoted strings together.
#[must_use]
pub fn words(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                cur.push(c);
            }
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_expect(text: &str) -> Expect {
    let mut e = Expect::default();
    for w in words(text) {
        let upper = w.to_ascii_uppercase();
        if w.len() == 5 && w.chars().all(|c| c.is_ascii_digit()) {
            e.status = Some(w);
        } else if let Some((f, v)) = w.split_once('=') {
            e.fields
                .push((f.to_ascii_uppercase(), v.trim_matches('\'').to_string()));
        } else {
            match upper.as_str() {
                "FOUND" => e.found = Some(true),
                "NOFOUND" | "NOTFOUND" => e.found = Some(false),
                "EQUAL" => e.equal = Some(true),
                "NOTEQUAL" | "NOEQUAL" => e.equal = Some(false),
                "EOF" => e.eof = Some(true),
                "NOEOF" => e.eof = Some(false),
                _ => {}
            }
        }
    }
    e
}

/// A key argument from words: `*START`, `*END`, `#n`, else values.
fn key_arg(ws: &[String]) -> Result<KeyArg, String> {
    match ws.first().map(|w| w.to_ascii_uppercase()).as_deref() {
        None => Err("a key is needed".into()),
        Some("*START") => Ok(KeyArg::Start),
        Some("*END") => Ok(KeyArg::End),
        Some("*HIVAL") => Ok(KeyArg::HiVal),
        Some("*LOVAL") => Ok(KeyArg::LoVal),
        Some(w) if w.starts_with('#') => Ok(KeyArg::Rrn(
            w[1..]
                .parse()
                .map_err(|_| format!("{w}: a record number is needed"))?,
        )),
        _ => Ok(KeyArg::Values(
            ws.iter().map(|w| key_value(w)).collect::<Result<_, _>>()?,
        )),
    }
}

fn key_value(w: &str) -> Result<DbValue, String> {
    if w.starts_with('\'') {
        return ccsid::to_ebcdic(w.trim_matches('\''))
            .map(DbValue::Char)
            .map_err(|_| format!("{w}: not CP037"));
    }
    if w.eq_ignore_ascii_case("*NULL") {
        return Ok(DbValue::Null);
    }
    Decimal::parse(w).map(DbValue::Num).or_else(|_| {
        ccsid::to_ebcdic(w)
            .map(DbValue::Char)
            .map_err(|_| format!("{w}: not CP037"))
    })
}

fn sql_value(w: &str) -> Value {
    if w.starts_with('\'') {
        return Value::Str(w.trim_matches('\'').to_string());
    }
    if w.eq_ignore_ascii_case("*NULL") {
        return Value::Null;
    }
    Decimal::parse(w).map_or_else(|_| Value::Str(w.to_string()), Value::Num)
}

/// Field values of a read record as text, for the expectations.
fn field_texts(table: &TableInfo, rec: &db::odp::Record) -> Vec<(String, String)> {
    let fmt = &table.format;
    fmt.format
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let v = db::record::read_field(f, &rec.data, &rec.null_map, fmt.null_index(i))
                .ok()
                .and_then(|v| Value::from_field(f, &v).ok())
                .map_or_else(
                    || "?".to_string(),
                    |v| {
                        if v.is_null() {
                            "*NULL".into()
                        } else {
                            v.to_string().trim_end().to_string()
                        }
                    },
                );
            (f.name.to_string(), v)
        })
        .collect()
}

struct Driver {
    odp: Option<(Odp, TableInfo)>,
    last: Option<db::odp::Record>,
    /// The status while nothing is open: `00000` after a `CLOSE`, else
    /// `01211` (no file open).
    closed_status: String,
}

impl Driver {
    fn status(&self) -> String {
        self.odp
            .as_ref()
            .map_or_else(|| self.closed_status.clone(), |(o, _)| o.fb.status_code())
    }

    fn odp(&mut self) -> Result<&mut (Odp, TableInfo), String> {
        self.odp
            .as_mut()
            .ok_or_else(|| "no file is open".to_string())
    }

    fn open(&mut self, txn: &mut Txn<'_>, ctx: &SqlContext, args: &[String]) -> Result<(), String> {
        let qual = args.first().ok_or("OPEN needs LIB/FILE")?;
        let name = object_name(qual)?;
        let table = ctx.resolve_table(txn, &name).map_err(|e| e.to_string())?;
        let mut opts = OpenOpts::input(ctx.job);
        opts.member = MemberSpec::Name(table.member.name);
        for a in &args[1..] {
            match a.to_ascii_uppercase().as_str() {
                "INPUT" => opts.usage = Usage::INPUT,
                "UPDATE" => opts.usage = Usage::UPDATE,
                "OUTPUT" => opts.usage = Usage::OUTPUT,
                "ALL" => opts.usage = Usage::ALL,
                "KEYED" => opts.keyed = true,
                "ARRIVAL" => opts.keyed = false,
                m => {
                    opts.member =
                        MemberSpec::Name(Name::parse(m).map_err(|e| format!("{m}: {e}"))?);
                }
            }
        }
        if let Some((o, _)) = self.odp.take() {
            o.close(txn);
        }
        let o = Odp::open(txn, ctx.locks.clone(), table.id, opts).map_err(|e| e.to_string())?;
        self.odp = Some((o, table));
        Ok(())
    }

    /// Run one operation; the record read, if any. A database error is
    /// not an error of the script: the status code reports it.
    #[allow(clippy::too_many_lines)]
    fn step(
        &mut self,
        txn: &mut Txn<'_>,
        ctx: &SqlContext,
        op: &str,
        args: &[String],
    ) -> Result<Option<db::odp::Record>, String> {
        let up = op.to_ascii_uppercase();
        let result: Result<Option<db::odp::Record>, DbError> = match up.as_str() {
            "OPEN" => {
                self.open(txn, ctx, args)?;
                return Ok(None);
            }
            "CLOSE" => {
                let (o, _) = self.odp.take().ok_or("no file is open")?;
                o.close(txn);
                self.closed_status = "00000".into();
                return Ok(None);
            }
            "SETLL" => self.odp()?.0.setll(txn, &key_arg(args)?).map(|()| None),
            "SETGT" => self.odp()?.0.setgt(txn, &key_arg(args)?).map(|()| None),
            "READ" => self.odp()?.0.read(txn, true),
            "READN" => self.odp()?.0.read(txn, false),
            "READP" => self.odp()?.0.readp(txn, true),
            "READE" => {
                let k = if args.is_empty() {
                    None
                } else {
                    Some(key_arg(args)?)
                };
                self.odp()?.0.reade(txn, k.as_ref(), true)
            }
            "READPE" => {
                let k = if args.is_empty() {
                    None
                } else {
                    Some(key_arg(args)?)
                };
                self.odp()?.0.readpe(txn, k.as_ref(), true)
            }
            "CHAIN" => self.odp()?.0.chain(txn, &key_arg(args)?, true),
            "CHAINN" => self.odp()?.0.chain(txn, &key_arg(args)?, false),
            "WRITE" | "UPDATE" => {
                let last = self.last.clone();
                let (o, table) = self.odp()?;
                let mut values: Vec<Option<Value>> = vec![None; table.format.format.fields.len()];
                for a in args {
                    let (f, v) = a
                        .split_once('=')
                        .ok_or_else(|| format!("{a}: FIELD=value is needed"))?;
                    let (i, _) = table
                        .field(f)
                        .ok_or_else(|| format!("{f}: no such field"))?;
                    values[i] = Some(sql_value(v));
                }
                let base = if up == "UPDATE" {
                    last.as_ref()
                        .map(|r| (r.data.as_slice(), r.null_map.as_slice()))
                } else {
                    None
                };
                let (rec, nulls) = build_record(table, &values, base).map_err(|e| e.to_string())?;
                if up == "WRITE" {
                    o.write(txn, &rec, &nulls).map(|_| None)
                } else {
                    o.update(txn, &rec, &nulls).map(|()| None)
                }
            }
            "DELETE" => self.odp()?.0.delete(txn).map(|()| None),
            "UNLOCK" => self.odp()?.0.unlock_op(txn).map(|()| None),
            "FEOD" => self.odp()?.0.feod(txn).map(|()| None),
            other => return Err(format!("{other}: not an operation")),
        };
        match result {
            Ok(r) => {
                if r.is_some() {
                    self.last.clone_from(&r);
                }
                Ok(r)
            }
            Err(_) => Ok(None),
        }
    }
}

fn object_name(s: &str) -> Result<ObjectName, String> {
    let parse = |n: &str| Name::parse(n).map_err(|e| format!("{n}: {e}"));
    if let Some((lib, name)) = s.split_once('/') {
        parse(lib)?;
        parse(name)?;
        Ok(ObjectName::qualified(lib, name))
    } else {
        parse(s)?;
        Ok(ObjectName::new(s))
    }
}

/// Whether a read value matches an expected one: numbers by value,
/// text after trimming.
fn same(got: &str, want: &str) -> bool {
    if got == want {
        return true;
    }
    match (Decimal::parse(got), Decimal::parse(want)) {
        (Ok(a), Ok(b)) => a == b,
        _ => got.trim_end() == want.trim_end(),
    }
}

/// Run the script lines in `txn`; the paths opened are closed at the end.
pub fn run_script(txn: &mut Txn<'_>, ctx: &mut SqlContext, lines: &[String]) -> Report {
    let mut report = Report::default();
    let mut d = Driver {
        odp: None,
        last: None,
        closed_status: "01211".into(),
    };
    for (n, raw) in lines.iter().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("--") {
            continue;
        }
        let (op_text, expect_text) = line
            .split_once("=>")
            .map_or((line, ""), |(a, b)| (a.trim(), b.trim()));
        let expect = parse_expect(expect_text);
        let ws = words(op_text);
        let Some(op) = ws.first() else { continue };
        let args = &ws[1..];
        let rec = match d.step(txn, ctx, op, args) {
            Ok(r) => r,
            Err(e) => {
                report.bad += 1;
                let _ = writeln!(report.text, "{:>4} FAIL {line}\n       {e}", n + 1);
                continue;
            }
        };
        let status = d.status();
        let (found, equal, eof) = d.odp.as_ref().map_or((false, false, false), |(o, _)| {
            (o.fb.found, o.fb.equal, o.fb.eof)
        });
        let mut problems = Vec::new();
        let texts = match (&rec, &d.odp) {
            (Some(r), Some((_, table))) => field_texts(table, r),
            _ => Vec::new(),
        };
        report.lines.push(LineResult {
            line: u32::try_from(n + 1).unwrap_or(u32::MAX),
            status: status.clone(),
            found,
            equal,
            eof,
            fields: expect
                .fields
                .iter()
                .map(|(f, _)| {
                    let got = texts
                        .iter()
                        .find(|(name, _)| name == f)
                        .map_or_else(|| "?".to_string(), |(_, v)| v.clone());
                    (f.clone(), got)
                })
                .collect(),
        });
        if let Some(s) = &expect.status
            && *s != status
        {
            problems.push(format!("status {status}"));
        }
        if expect.found.is_some_and(|f| f != found) {
            problems.push(format!("found={found}"));
        }
        if expect.equal.is_some_and(|f| f != equal) {
            problems.push(format!("equal={equal}"));
        }
        if expect.eof.is_some_and(|f| f != eof) {
            problems.push(format!("eof={eof}"));
        }
        if !expect.fields.is_empty() {
            for (f, want) in &expect.fields {
                match texts.iter().find(|(n, _)| n == f) {
                    Some((_, got)) if same(got, want) => {}
                    Some((_, got)) => problems.push(format!("{f}={got}")),
                    None => problems.push(format!("{f} not read")),
                }
            }
        }
        if problems.is_empty() {
            report.ok += 1;
            let _ = writeln!(report.text, "{:>4} ok   {line}", n + 1);
        } else {
            report.bad += 1;
            let _ = writeln!(
                report.text,
                "{:>4} FAIL {line}\n       got {status} {}",
                n + 1,
                problems.join(" ")
            );
        }
    }
    if let Some((o, _)) = d.odp.take() {
        o.close(txn);
    }
    let _ = writeln!(report.text, "{} ok, {} mismatch(es)", report.ok, report.bad);
    report
}
