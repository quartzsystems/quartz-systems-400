// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The records: `3018` (monitor start and end), `1000` (a statement),
//! `3000` (table scan), `3001` (index used), `3003` (sort), `3014`
//! (the query's summary) and `3010` (input values). Each column means
//! what IBM's record of that number says; a column QS/400 has no value
//! for is null.

use crate::exec::SqlContext;
use crate::sqlca::Sqlca;
use db::record::datetime::{Date, Time};

use crate::value::{Timestamp, Value};

use super::plan::{PlanEvent, TableName};
use super::{JobIdent, Scope, text};

/// One record: the columns it sets, the rest null.
#[derive(Clone, Debug, PartialEq)]
pub struct Record(pub Vec<(&'static str, Value)>);

impl Record {
    /// The record ID (`QQRID`).
    #[must_use]
    pub fn id(&self) -> i64 {
        self.get("QQRID").and_then(|v| v.as_i64().ok()).unwrap_or(0)
    }

    /// The value of `col`.
    #[must_use]
    pub fn get(&self, col: &str) -> Option<&Value> {
        self.0.iter().find(|(c, _)| *c == col).map(|(_, v)| v)
    }

    fn set(&mut self, col: &'static str, v: Value) {
        self.0.push((col, v));
    }

    fn s(&mut self, col: &'static str, v: &str) {
        self.set(col, text(v));
    }

    fn n(&mut self, col: &'static str, v: i64) {
        self.set(col, Value::int(v));
    }

    fn yn(&mut self, col: &'static str, v: bool) {
        self.s(col, if v { "Y" } else { "N" });
    }
}

/// A statement as the session ran it.
#[derive(Debug)]
pub struct Observed<'a> {
    /// IBM's statement operation (`QQC21`): `SI`, `OP`, `IN`, `PR` ...
    pub op: &'static str,
    /// The statement text.
    pub text: &'a str,
    /// The cursor it names.
    pub cursor: &'a str,
    /// The prepared statement it names.
    pub statement: &'a str,
    /// When it started and ended.
    pub start: Timestamp,
    /// The end.
    pub end: Timestamp,
    /// Microseconds it took.
    pub micros: u64,
    /// Its outcome.
    pub sqlca: &'a Sqlca,
    /// Rows an `INSERT`, `UPDATE`, `DELETE` or `MERGE` changed.
    pub rows: Option<i64>,
    /// Its input values.
    pub inputs: &'a [Value],
    /// Prepared at run time (`D`), else static (`S`).
    pub dynamic: bool,
}

/// The statement function (`QQC11`) of an operation.
#[must_use]
pub fn function(op: &str) -> &'static str {
    match op {
        "SI" | "OP" | "FE" | "VI" => "S",
        "UP" => "U",
        "IN" => "I",
        "DL" => "D",
        "AL" | "CC" | "CD" | "CF" | "CG" | "CI" | "CP" | "CQ" | "CS" | "CT" | "CV" | "CB"
        | "DR" | "LO" | "CO" | "RT" | "GR" | "RV" => "L",
        _ => "O",
    }
}

/// The statement operation (`QQC21`) of a statement's text.
#[must_use]
pub fn operation(text: &str) -> &'static str {
    let mut w: Vec<String> = text
        .split(|c: char| c.is_whitespace() || c == '(')
        .filter(|s| !s.is_empty())
        .take(5)
        .map(str::to_ascii_uppercase)
        .collect();
    if w.len() > 2 && w[0] == "CREATE" && w[1] == "OR" {
        w.drain(1..3);
    }
    let word = |i: usize| w.get(i).cloned().unwrap_or_default();
    let (first, second, third) = (word(0), word(1), word(2));
    match (first.as_str(), second.as_str()) {
        ("SELECT" | "WITH", _) => "SI",
        ("VALUES", _) => "VI",
        ("INSERT", _) => "IN",
        ("UPDATE", _) => "UP",
        ("DELETE", _) => "DL",
        ("MERGE", _) => "MG",
        ("CALL", _) => "CA",
        ("COMMIT", _) => "CM",
        ("ROLLBACK", _) if third == "SAVEPOINT" || second == "TO" => "RB",
        ("ROLLBACK", _) => "RO",
        ("SAVEPOINT", _) => "SA",
        ("RELEASE", "SAVEPOINT" | "TO") => "RS",
        ("RELEASE", _) => "RE",
        ("CONNECT", _) => "CN",
        ("DISCONNECT", _) => "DI",
        ("LOCK", _) => "LK",
        ("LABEL", _) => "LO",
        ("COMMENT", _) => "CO",
        ("GRANT", _) => "GR",
        ("REVOKE", _) => "RV",
        ("RENAME", _) => "RT",
        ("REFRESH", _) => "RF",
        ("DROP", _) => "DR",
        ("ALTER", "TABLE") => "AL",
        ("ALTER", "SEQUENCE") => "AQ",
        ("ALTER", "PROCEDURE") => "AP",
        ("ALTER", "FUNCTION") => "AF",
        ("SET", "CONNECTION") => "SC",
        ("SET", "SCHEMA" | "CURRENT") if third == "SCHEMA" || second == "SCHEMA" => "SS",
        ("SET", "PATH") => "SP",
        ("SET", "TRANSACTION") => "ST",
        ("SET", "RESULT") => "SR",
        ("SET", _) => "SV",
        ("CREATE", _) => create_op(&second),
        _ => "X0",
    }
}

fn create_op(kind: &str) -> &'static str {
    match kind {
        "TABLE" => "CT",
        "INDEX" | "UNIQUE" | "ENCODED" => "CI",
        "VIEW" => "CV",
        "PROCEDURE" => "CP",
        "FUNCTION" => "CF",
        "TRIGGER" => "CG",
        "ALIAS" | "SYNONYM" => "CS",
        "SEQUENCE" => "CQ",
        "VARIABLE" => "CB",
        "SCHEMA" | "COLLECTION" => "CC",
        "TYPE" | "DISTINCT" => "CD",
        _ => "X0",
    }
}

/// The current time in the job's zone.
#[must_use]
pub fn now(ctx: &SqlContext) -> Timestamp {
    let (secs, picos) = ctx.clock_picos();
    let local = secs.saturating_add_signed(ctx.registers.timezone_secs);
    Timestamp::new(
        Date::from_unix(local),
        Time::from_unix(local),
        picos / 1_000_000 * 1_000_000,
    )
}

/// The columns every record has.
fn common(ctx: &SqlContext, job: &JobIdent, id: i64, ucnt: i64) -> Record {
    let mut r = Record(Vec::with_capacity(40));
    r.n("QQRID", id);
    r.set("QQTIME", Value::Timestamp(now(ctx)));
    r.s(
        "QQJFLD",
        &format!("{:<10}{:<10}{:06}", job.name, job.user, job.number),
    );
    r.s("QQRDBN", &ctx.dbmon.system);
    r.s("QQSYS", &ctx.dbmon.system);
    r.s("QQJOB", &job.name);
    r.s("QQUSER", &job.user);
    r.s("QQJNUM", &format!("{:06}", job.number));
    r.n("QQUCNT", ucnt);
    r.s("QQUDEF", "");
    r
}

/// A `3018` record: a monitor started (`S`) or ended (`E`).
#[must_use]
pub fn monitor(
    ctx: &SqlContext,
    job: &JobIdent,
    scope: &Scope,
    id: &str,
    start: bool,
    comment: &str,
    command: &str,
) -> Record {
    let mut r = common(ctx, job, 3018, 0);
    r.0.retain(|(c, _)| *c != "QQUDEF");
    r.s("QQUDEF", comment);
    r.s("QQC11", if *scope == Scope::All { "A" } else { "C" });
    r.s("QQC12", if start { "S" } else { "E" });
    r.s(
        "QQC301",
        if *scope == Scope::All {
            "*ALL/*ALL/*ALL"
        } else {
            "*"
        },
    );
    r.s("QQC101", id);
    r.s("QQC102", &release());
    r.s("QQ1000L", command);
    r
}

/// The release as `VxRyMz`.
fn release() -> String {
    "V1R1M0".into()
}

/// The records of one statement, in IBM's order: the access methods,
/// the summary, the input values, the statement.
#[must_use]
pub fn statement(
    ctx: &SqlContext,
    job: &JobIdent,
    o: &Observed<'_>,
    plan: &[PlanEvent],
    ucnt: u64,
    stn: u64,
) -> Vec<Record> {
    let u = i64::try_from(ucnt).unwrap_or(i64::MAX);
    let mut out = Vec::new();
    for e in plan {
        out.push(access(ctx, job, e, u, plan));
    }
    if !plan.is_empty() {
        out.push(summary(ctx, job, plan, u, !o.inputs.is_empty()));
    }
    if !o.inputs.is_empty() && (ucnt > 0) {
        let mut r = common(ctx, job, 3010, u);
        let vals: Vec<String> = o
            .inputs
            .iter()
            .map(|v| {
                if matches!(v, Value::Null) {
                    "NULL".into()
                } else {
                    v.to_string()
                }
            })
            .collect();
        let joined = vals.join(",");
        r.s("QQC11", "R");
        r.s("QQC12", "I");
        r.s("QQ1000", &cut(&joined, 1000));
        r.n("QQI7", i64::try_from(joined.chars().count()).unwrap_or(0));
        r.s(
            "QVC11",
            if joined.chars().count() > 1000 {
                "Y"
            } else {
                "N"
            },
        );
        out.push(r);
    }
    out.push(sql_info(ctx, job, o, u, stn));
    out
}

fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn table_cols(r: &mut Record, t: &TableName) {
    r.s("QQTLN", &t.lib);
    r.s("QQTFN", &t.file);
    r.s("QQTMN", &t.member);
    r.s("QQPTLN", &t.lib);
    r.s("QQPTFN", &t.file);
    r.s("QQPTMN", &t.member);
    r.s("QVQTBL", &t.file);
    r.s("QVQLIB", &t.lib);
    r.s("QVPTBL", &t.file);
    r.s("QVPLIB", &t.lib);
}

fn access(
    ctx: &SqlContext,
    job: &JobIdent,
    e: &PlanEvent,
    ucnt: i64,
    plan: &[PlanEvent],
) -> Record {
    let tables = i64::try_from(
        plan.iter()
            .filter(|p| !matches!(p, PlanEvent::Sort { .. }))
            .count(),
    )
    .unwrap_or(1);
    match e {
        PlanEvent::Scan {
            table,
            rows,
            indexes,
            selection,
            position,
        } => {
            let mut r = common(ctx, job, 3000, ucnt);
            subselect(&mut r);
            table_cols(&mut r, table);
            r.n("QQTOTR", i64::try_from(*rows).unwrap_or(i64::MAX));
            r.n("QQJNP", i64::from(*position));
            r.yn("QQDSS", *selection);
            r.yn("QQIDXA", false);
            r.s("QQRCOD", if *indexes { "T2" } else { "T1" });
            r.s("QQC11", "N");
            r.s("QQC13", "N");
            r.s("QQC14", "N");
            r.s("QQC15", "N");
            r.s("QQC18", "N");
            r.n("QVFILES", tables);
            r
        }
        PlanEvent::Index {
            table,
            index,
            constraint,
            rows,
            positioned,
            ordering,
            position,
        } => {
            let mut r = common(ctx, job, 3001, ucnt);
            subselect(&mut r);
            table_cols(&mut r, table);
            r.s("QQILNM", &index.lib);
            r.s("QQIFNM", &index.file);
            r.s("QQIMNM", &index.member);
            r.s("QVINAM", constraint.as_deref().unwrap_or(&index.file));
            r.s("QVILIB", &index.lib);
            r.n("QQTOTR", i64::try_from(*rows).unwrap_or(i64::MAX));
            r.n("QQJNP", i64::from(*position));
            r.yn("QQKP", !positioned.is_empty());
            r.yn("QQKS", false);
            r.n("QQI3", i64::try_from(positioned.len()).unwrap_or(0));
            r.yn("QQDSS", false);
            r.yn("QQIDXA", false);
            r.s(
                "QQRCOD",
                match (!positioned.is_empty(), *ordering) {
                    (true, true) => "I3",
                    (false, true) => "I2",
                    _ => "I1",
                },
            );
            r.yn("QQC11", constraint.is_some());
            if let Some(c) = constraint {
                r.s("QQ1000", c);
            }
            r.s("QQC15", if constraint.is_some() { "C" } else { "B" });
            r.s("QVC12", "P");
            r.s("QVC3003", &positioned.join(", "));
            r.n("QVFILES", tables);
            r
        }
        PlanEvent::Sort { rows, reason } => {
            let mut r = common(ctx, job, 3003, ucnt);
            subselect(&mut r);
            r.n("QQRSS", i64::try_from(*rows).unwrap_or(i64::MAX));
            r.s("QQRCOD", reason);
            r.n("QVFILES", tables);
            r
        }
    }
}

/// Subselect numbers: QS/400 numbers none, every access is the first.
fn subselect(r: &mut Record) {
    r.n("QQQDTN", 1);
    r.n("QQQDTL", 1);
    r.n("QQMATN", 0);
    r.n("QQMATL", 0);
}

fn summary(ctx: &SqlContext, job: &JobIdent, plan: &[PlanEvent], ucnt: i64, host: bool) -> Record {
    let mut r = common(ctx, job, 3014, ucnt);
    subselect(&mut r);
    let sorted = plan.iter().find_map(|p| match p {
        PlanEvent::Sort { reason, .. } => Some(*reason),
        _ => None,
    });
    let tables = plan
        .iter()
        .filter(|p| !matches!(p, PlanEvent::Sort { .. }))
        .count();
    let ordered_by_index = plan
        .iter()
        .any(|p| matches!(p, PlanEvent::Index { ordering: true, .. }));
    r.yn(
        "QQORDG",
        matches!(sorted, Some("F2" | "F7")) || ordered_by_index,
    );
    r.yn("QQGRPG", sorted == Some("F1"));
    r.yn("QQJNG", tables > 1);
    r.yn("QQUNIN", sorted == Some("F5"));
    r.yn("QQHSTV", host);
    r.yn(
        "QQRCDS",
        plan.iter().any(|p| {
            matches!(
                p,
                PlanEvent::Scan {
                    selection: true,
                    ..
                }
            )
        }),
    );
    r.yn("QQC11", false);
    r.yn("QQC12", false);
    r.s("QQC102", "*N");
    r.s("QQC103", "QAQQINI");
    r.s(
        "QVC11",
        if ordered_by_index {
            "I"
        } else if sorted.is_some() {
            "S"
        } else {
            " "
        },
    );
    // The join method: `N` nested loop, `H` hash, `M` sort-merge.
    let method = ctx.plan.join_code().unwrap_or('N').to_string();
    r.s("QVC13", if tables > 1 { &method } else { " " });
    r.yn("QVC14", sorted == Some("F4"));
    r.yn("QVC15", false);
    r
}

/// A `1000` record of a plan cache entry, as `DUMP_PLAN_CACHE` writes
/// it: the statement, its number, when it was last used.
#[must_use]
pub fn cached(
    ctx: &SqlContext,
    text: &str,
    number: u64,
    (first, last): (Timestamp, Timestamp),
    sql_naming: bool,
) -> Record {
    let job = JobIdent {
        name: "QDBSRV01".into(),
        user: "QSYS".into(),
        number: 0,
    };
    let n = i64::try_from(number).unwrap_or(i64::MAX);
    let mut r = common(ctx, &job, 1000, n);
    r.0.retain(|(c, _)| *c != "QQTIME");
    r.set("QQTIME", Value::Timestamp(last));
    let op = operation(text);
    r.n("QQSTN", n);
    r.set("QQSTIM", Value::Timestamp(first));
    r.s("QQC11", function(op));
    r.s("QQC12", "D");
    r.s("QQC21", op);
    r.s("QQC22", "NA");
    r.n("QQI7", i64::try_from(text.chars().count()).unwrap_or(0));
    r.s("QQ1000", &cut(text, 1000));
    r.s("QQ1000L", text);
    r.s("QVC17", if sql_naming { "S" } else { "N" });
    r
}

fn sql_info(ctx: &SqlContext, job: &JobIdent, o: &Observed<'_>, ucnt: i64, stn: u64) -> Record {
    let mut r = common(ctx, job, 1000, ucnt);
    r.n("QQSTN", i64::try_from(stn).unwrap_or(i64::MAX));
    r.set("QQSTIM", Value::Timestamp(o.start));
    r.set("QQETIM", Value::Timestamp(o.end));
    r.s("QQC11", function(o.op));
    r.s("QQC12", if o.dynamic { "D" } else { "S" });
    r.yn("QQC13", matches!(o.op, "PR" | "PD" | "EI"));
    r.s("QQC14", if o.sqlca.sqlcode < 0 { "U" } else { "S" });
    r.s("QQC21", o.op);
    r.s("QQC22", "NA");
    r.n("QQI2", o.rows.unwrap_or(0));
    r.n("QQI4", i64::try_from(o.micros / 1000).unwrap_or(i64::MAX));
    r.n("QQI6", i64::try_from(o.micros).unwrap_or(i64::MAX));
    r.n("QQI7", i64::try_from(o.text.chars().count()).unwrap_or(0));
    r.n("QQI8", i64::from(o.sqlca.sqlcode));
    r.s("QQC81", &o.sqlca.sqlstate);
    r.s("QQC181", o.cursor);
    r.s("QQC182", o.statement);
    r.s("QQ1000", &cut(o.text, 1000));
    r.s("QQ1000L", o.text);
    r.s(
        "QVC17",
        if ctx.naming == crate::ast::Naming::Sql {
            "S"
        } else {
            "N"
        },
    );
    r.s("QVC102", &ctx.user);
    if let Some(s) = ctx.current_schema {
        r.s("QVC1284", &s.to_string());
    }
    r.s(
        "QVC41",
        match ctx.session.commit_def().map(|c| c.borrow().lcklvl()) {
            None => "NC",
            Some(db::cmt::LckLvl::Chg) => "UR",
            Some(db::cmt::LckLvl::Cs) => "CS",
            Some(db::cmt::LckLvl::All) => "RS",
        },
    );
    r
}
