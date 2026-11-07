// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The prototype's database: schema `HLINV` with table `ASSETS` and its
//! twelve sample rows as `web/table-view.html` shows them, `LOCATIONS`
//! the sites they are in, the catalog's other entries, five saved
//! requests and a history. The SQL the table pages make (`data::sqltext`)
//! runs over the rows; the golden page's Run SQL script has its
//! results as the prototype shows them; anything else is the database's
//! `SQL0104`.

#![allow(clippy::many_single_char_names)] // rows, tables, columns

use std::collections::BTreeMap;
use std::sync::Mutex;

use qsweb::msg::{Kind as MsgKind, Message};

use crate::env::db::{
    Column, Commit, Db, Entry, Kind, Options, Outcome, ReqType, Request, ResultColumn, Val,
};
use crate::util::secs;

/// The schemas the user may list (Preferences' default schema offers
/// them).
pub const SCHEMAS: &[&str] = &["HLINV", "QGPL", "WEBSTORE"];

/// Every schema in the catalog.
pub const CATALOG: &[&str] = &["HLINV", "HLINVTEST", "QGPL", "QSYS2", "WEBSTORE"];

/// `HLINV.ASSETS`' columns.
pub const ASSETS_COLUMNS: &[&str] = &[
    "ASSET_TAG",
    "LOCATION",
    "MANUFACTURER",
    "MODEL",
    "SERIAL",
    "ASSET_TYPE",
    "STATUS",
    "ACQUIRED",
    "COST",
    "RACK_U",
    "NOTES",
    "CHANGED_TS",
];

/// `HLINV.ASSETS`' rows (the first eight columns; the rest are NULL
/// and the change stamp).
pub const ASSETS: &[[&str; 8]] = &[
    [
        "QS000101",
        "QS-HQ-R1",
        "QUANTA",
        "T4048-IX8",
        "QTFCEV2340021",
        "SWITCH",
        "ACTIVE",
        "2025-03-11",
    ],
    [
        "QS000102",
        "QS-HQ-R1",
        "QUANTA",
        "T4048-IX8",
        "QTFCEV2340022",
        "SWITCH",
        "ACTIVE",
        "2025-03-11",
    ],
    [
        "QS000110",
        "QS-HQ-R1",
        "EDGECORE",
        "AS5712-54X",
        "571254X2041B0019",
        "SWITCH",
        "ACTIVE",
        "2024-11-02",
    ],
    [
        "QS000111",
        "QS-HQ-R1",
        "EDGECORE",
        "AS5712-54X",
        "571254X2041B0020",
        "SWITCH",
        "ACTIVE",
        "2024-11-02",
    ],
    [
        "QS000201",
        "QS-HQ-R2",
        "DELL",
        "R740XD",
        "7XK2Q33",
        "SERVER",
        "ACTIVE",
        "2024-06-20",
    ],
    [
        "QS000202",
        "QS-HQ-R2",
        "DELL",
        "R740XD",
        "7XK2Q34",
        "SERVER",
        "ACTIVE",
        "2024-06-20",
    ],
    [
        "QS000203",
        "QS-HQ-R2",
        "DELL",
        "R740XD",
        "7XK2Q35",
        "SERVER",
        "ACTIVE",
        "2024-06-20",
    ],
    [
        "QS000210",
        "QS-HQ-R2",
        "DELL",
        "QLE2562",
        "BFE1234567",
        "ADAPTER",
        "ACTIVE",
        "2026-09-20",
    ],
    [
        "QS000302",
        "QS-HQ-R3",
        "WEIDMULLER",
        "UR20-FBC-EIP",
        "2024-11-00341",
        "IO-COUPLER",
        "ACTIVE",
        "2026-10-01",
    ],
    [
        "QS000401",
        "QS-BR1-R1",
        "DELL",
        "S4048-ON",
        "8H3K2P2",
        "SWITCH",
        "ACTIVE",
        "2024-09-08",
    ],
    [
        "QS000402",
        "QS-BR1-R1",
        "WATCHGUARD",
        "M370",
        "80BA0F2B1C33",
        "FIREWALL",
        "ACTIVE",
        "2024-09-08",
    ],
    [
        "QS000403",
        "QS-BR1-R1",
        "ARISTA",
        "7050SX-64",
        "JPE14210033",
        "SWITCH",
        "SPARE",
        "2023-05-02",
    ],
];

/// When every sample row last changed.
pub const CHANGED: &str = "2026-10-29-13.58.02.000000";

/// The sites.
pub const LOCATIONS: &[&str] = &["QS-BR1-R1", "QS-HQ-R1", "QS-HQ-R2", "QS-HQ-R3"];

/// A catalog entry: name, kind, text, rows, last changed (month, day,
/// hour, minute; none for 0).
type Sample = (&'static str, Kind, &'static str, u64, (u32, u32, u32, u32));

/// `HLINV`'s catalog entries.
const HLINV: &[Sample] = &[
    (
        "ASSETS",
        Kind::Table,
        "Hardware assets",
        12,
        (10, 29, 13, 58),
    ),
    (
        "LOCATIONS",
        Kind::Table,
        "Racks and sites",
        14,
        (9, 20, 10, 0),
    ),
    ("MANUFACTURERS", Kind::Table, "Vendors", 38, (9, 14, 12, 11)),
    (
        "MOVEMENTS",
        Kind::Table,
        "Asset movement log",
        2207,
        (10, 29, 6, 14),
    ),
    (
        "CONTRACTS",
        Kind::Table,
        "Support contracts",
        27,
        (10, 2, 15, 30),
    ),
    (
        "NOTES",
        Kind::Table,
        "Free text notes",
        1118,
        (10, 28, 17, 2),
    ),
    (
        "ASSETS_BY_LOC",
        Kind::View,
        "Assets joined to location",
        12,
        (0, 0, 0, 0),
    ),
    (
        "EXPIRING",
        Kind::View,
        "Contracts expiring in 90 days",
        4,
        (0, 0, 0, 0),
    ),
    (
        "ASSETLF1",
        Kind::Logical,
        "Assets by serial",
        12,
        (0, 0, 0, 0),
    ),
    (
        "ASSETP",
        Kind::Physical,
        "Assets (system name)",
        412,
        (0, 0, 0, 0),
    ),
];

fn col(
    name: &str,
    typ: &str,
    length: u32,
    scale: Option<u32>,
    nullable: bool,
    default: &str,
) -> Column {
    Column {
        name: name.into(),
        typ: typ.into(),
        length,
        scale,
        nullable,
        default: default.into(),
        ..Column::default()
    }
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(ToString::to_string).collect()
}

/// `HLINV.ASSETS`' columns as the catalog describes them.
pub fn assets_columns() -> Vec<Column> {
    let mut c = vec![
        col("ASSET_TAG", "CHAR", 10, None, false, ""),
        col("LOCATION", "CHAR", 10, None, false, ""),
        col("MANUFACTURER", "VARCHAR", 40, None, false, ""),
        col("MODEL", "VARCHAR", 40, None, false, ""),
        col("SERIAL", "VARCHAR", 40, None, true, "NULL"),
        col("ASSET_TYPE", "CHAR", 12, None, false, ""),
        col("STATUS", "CHAR", 8, None, false, ""),
        col("ACQUIRED", "DATE", 4, None, true, "NULL"),
        col("COST", "DECIMAL", 11, Some(2), true, "NULL"),
        col("RACK_U", "SMALLINT", 2, None, true, "NULL"),
        col("NOTES", "VARCHAR", 256, None, true, "NULL"),
        col(
            "CHANGED_TS",
            "TIMESTAMP",
            10,
            None,
            false,
            "CURRENT TIMESTAMP",
        ),
    ];
    c[0].key = true;
    c[1].choices = strs(LOCATIONS);
    c[5].choices = strs(&[
        "SWITCH",
        "SERVER",
        "FIREWALL",
        "ADAPTER",
        "IO-COUPLER",
        "OPTICAL",
        "OTHER",
    ]);
    c[6].choices = strs(&["ACTIVE", "SPARE", "RETIRED"]);
    c
}

fn assets_rows() -> Vec<Vec<Val>> {
    ASSETS
        .iter()
        .map(|r| {
            let mut v: Vec<Val> = r.iter().map(|s| Val::Text((*s).into())).collect();
            v.extend([Val::Null, Val::Null, Val::Null, Val::Text(CHANGED.into())]);
            v
        })
        .collect()
}

/// A table of the sample system.
#[derive(Clone, Debug)]
struct Table {
    columns: Vec<Column>,
    rows: Vec<Vec<Val>>,
}

/// One connection.
#[derive(Clone, Debug, Default)]
struct Conn {
    user: String,
    opts: Options,
    /// The tables before the first change of the unit of work.
    before: Option<BTreeMap<(String, String), Table>>,
    /// Open cursors: their columns and the rows left.
    cursors: BTreeMap<u32, (Vec<ResultColumn>, Vec<Vec<Val>>)>,
}

#[derive(Debug)]
struct State {
    tables: BTreeMap<(String, String), Table>,
    conns: BTreeMap<u32, Conn>,
    next: u32,
    requests: Vec<Request>,
    history: BTreeMap<String, Vec<String>>,
}

/// The user the sample history is kept for.
const USER_SAMPLE: &str = "CWELLMAN";

/// The connections a session may have.
pub const MAX_CONNECTIONS: usize = 16;

/// The sample database.
#[derive(Debug)]
pub struct FixtureDb {
    state: Mutex<State>,
    /// Statements run, as run (for tests).
    pub ran: Mutex<Vec<(String, String)>>,
}

fn stamp(m: u32, d: u32, h: u32, mi: u32) -> u64 {
    // Local (EDT) to UTC.
    secs(2026, m, d, h + 4, mi, 0)
}

/// The saved requests the prototype lists.
pub fn sample_requests() -> Vec<Request> {
    let r = |id, name: &str, typ, body: &str, format: &str, at: u64, shared: &str| Request {
        id,
        owner: "CWELLMAN".into(),
        name: name.into(),
        typ,
        body: body.into(),
        format: format.into(),
        last_run: at,
        shared: shared.into(),
    };
    let up = crate::data::qdx::to_text(&crate::data::qdx::Request::Upload {
        system: "QSHQ400".into(),
        upload: crate::data::qdx::Upload {
            file: "vendors.csv".into(),
            schema: "HLINV".into(),
            table: "MANUFACTURERS".into(),
            ..crate::data::qdx::Upload::default()
        },
    });
    let down = crate::data::qdx::to_text(&crate::data::qdx::Request::Download {
        system: "QSHQ400".into(),
        download: crate::data::qdx::Download {
            source: crate::data::qdx::Source::Table {
                schema: "HLINV".into(),
                table: "ASSETS".into(),
                where_clause: String::new(),
            },
            columns: Vec::new(),
            limit: None,
            file: "assets.xlsx".into(),
            options: crate::data::qdx::FileOptions {
                format: crate::data::qdx::Format::Xlsx,
                ..crate::data::qdx::FileOptions::default()
            },
        },
    });
    vec![
        r(
            1,
            "Active assets by location",
            ReqType::Sql,
            "SELECT location, COUNT(*) FROM hlinv.assets WHERE status='ACTIVE' GROUP BY location ORDER BY location",
            "HTML",
            stamp(10, 29, 9, 12),
            "*PUBLIC",
        ),
        r(
            2,
            "Expiring contracts",
            ReqType::Sql,
            "SELECT * FROM hlinv.expiring ORDER BY end_date",
            "PDF",
            stamp(10, 27, 8, 0),
            "ADMINS",
        ),
        r(
            3,
            "Jobs in MSGW",
            ReqType::Sql,
            "SELECT * FROM qsys2.active_job_info WHERE job_status='MSGW'",
            "HTML",
            stamp(10, 29, 13, 59),
            "",
        ),
        r(
            4,
            "Load vendor list",
            ReqType::CopyTo,
            &up,
            "CSV",
            stamp(9, 14, 12, 11),
            "",
        ),
        r(
            5,
            "Asset export",
            ReqType::CopyFrom,
            &down,
            "XLSX",
            stamp(10, 28, 18, 0),
            "WAREHOUSE",
        ),
    ]
}

impl Default for FixtureDb {
    fn default() -> Self {
        let mut tables = BTreeMap::new();
        tables.insert(
            ("HLINV".to_string(), "ASSETS".to_string()),
            Table {
                columns: assets_columns(),
                rows: assets_rows(),
            },
        );
        let mut loc = vec![
            col("LOCATION", "CHAR", 10, None, false, ""),
            col("SITE", "VARCHAR", 40, None, true, "NULL"),
        ];
        loc[0].key = true;
        tables.insert(
            ("HLINV".to_string(), "LOCATIONS".to_string()),
            Table {
                columns: loc,
                rows: LOCATIONS
                    .iter()
                    .map(|l| vec![Val::Text((*l).into()), Val::Null])
                    .collect(),
            },
        );
        for (name, kind, ..) in HLINV {
            if !matches!(*name, "ASSETS" | "LOCATIONS") {
                let mut c = vec![col("NAME", "VARCHAR", 40, None, false, "")];
                c[0].key = *kind == Kind::Table;
                tables.insert(
                    ("HLINV".into(), (*name).into()),
                    Table {
                        columns: c,
                        rows: Vec::new(),
                    },
                );
            }
        }
        let mut c = vec![col("ID", "INTEGER", 4, None, false, "")];
        c[0].key = true;
        tables.insert(
            ("QGPL".into(), "SCRATCH".into()),
            Table {
                columns: c,
                rows: Vec::new(),
            },
        );
        let history = BTreeMap::from([(
            "CWELLMAN".to_string(),
            strs(&[
                "SELECT COUNT(*) FROM hlinv.assets GROUP BY location",
                "SELECT * FROM qsys2.active_job_info WHERE job_status = 'MSGW'",
                "UPDATE hlinv.assets SET status = 'RETIRED' WHERE asset_tag = 'QS000301'",
                "SELECT * FROM qsys2.system_status_info",
            ]),
        )]);
        Self {
            state: Mutex::new(State {
                tables,
                conns: BTreeMap::new(),
                next: 0,
                requests: sample_requests(),
                history,
            }),
            ran: Mutex::new(Vec::new()),
        }
    }
}

pub use crate::data::sqltext::like;

/// A token of the SQL the fixture reads.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Word(String),
    Mark,
    Num(String),
    Str(String),
    P(char),
}

fn lex(sql: &str) -> Vec<Tok> {
    let cs: Vec<char> = sql.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '"' || c == '\'' {
            let mut s = String::new();
            i += 1;
            while i < cs.len() {
                if cs[i] == c {
                    if cs.get(i + 1) == Some(&c) {
                        s.push(c);
                        i += 2;
                        continue;
                    }
                    break;
                }
                s.push(cs[i]);
                i += 1;
            }
            i += 1;
            out.push(if c == '"' { Tok::Word(s) } else { Tok::Str(s) });
        } else if c.is_ascii_alphabetic() || c == '_' {
            let st = i;
            while i < cs.len() && (cs[i].is_ascii_alphanumeric() || cs[i] == '_') {
                i += 1;
            }
            out.push(Tok::Word(
                cs[st..i].iter().collect::<String>().to_ascii_uppercase(),
            ));
        } else if c.is_ascii_digit() {
            let st = i;
            while i < cs.len() && (cs[i].is_ascii_digit() || cs[i] == '.') {
                i += 1;
            }
            out.push(Tok::Num(cs[st..i].iter().collect()));
        } else if c == '?' {
            out.push(Tok::Mark);
            i += 1;
        } else {
            out.push(Tok::P(c));
            i += 1;
        }
    }
    out
}

/// A failed statement's outcome.
fn fail(id: &str, state: &str, code: i32, text: &str) -> Outcome {
    Outcome {
        message: Some(Message::new(MsgKind::Error, id, text)),
        sqlstate: state.into(),
        sqlcode: code,
        failed: true,
        ..Outcome::default()
    }
}

fn syntax(at: &str) -> Outcome {
    fail(
        "SQL0104",
        "42601",
        -104,
        &format!("Token {at} was not valid."),
    )
}

struct P<'a> {
    t: &'a [Tok],
    i: usize,
    params: &'a [Val],
    next: usize,
}

impl P<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn word(&mut self, w: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Word(x)) if x == w) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn p(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::P(c)) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn name(&mut self) -> Option<String> {
        if let Some(Tok::Word(w)) = self.peek() {
            let w = w.clone();
            self.i += 1;
            Some(w)
        } else {
            None
        }
    }
    fn table(&mut self, schema: &str) -> Option<(String, String)> {
        let a = self.name()?;
        if self.p('.') || self.p('/') {
            Some((a, self.name()?))
        } else {
            Some((schema.to_string(), a))
        }
    }
    fn value(&mut self) -> Option<Val> {
        let v = match self.peek()? {
            Tok::Mark => {
                let v = self.params.get(self.next).cloned().unwrap_or(Val::Null);
                self.next += 1;
                v
            }
            Tok::Num(n) => Val::Num(n.clone()),
            Tok::Str(s) => Val::Text(s.clone()),
            Tok::Word(w) if w == "NULL" => Val::Null,
            _ => return None,
        };
        self.i += 1;
        Some(v)
    }
    fn names(&mut self) -> Vec<String> {
        let mut v = Vec::new();
        while let Some(n) = self.name() {
            v.push(n);
            if !self.p(',') {
                break;
            }
        }
        v
    }
    fn tok(&self) -> String {
        match self.peek() {
            Some(Tok::Word(w)) => w.clone(),
            Some(Tok::P(c)) => c.to_string(),
            Some(Tok::Num(n)) => n.clone(),
            Some(Tok::Str(s)) => format!("'{s}'"),
            Some(Tok::Mark) => "?".into(),
            None => "*N".into(),
        }
    }
}

/// A condition: Find's `UPPER(CAST(c AS VARCHAR(n))) LIKE ?`, or
/// `c = v AND ...`.
enum Cond {
    Like(String, String),
    Eq(Vec<(String, Val)>),
}

fn cond(p: &mut P<'_>) -> Option<Cond> {
    if p.word("UPPER") {
        p.p('(');
        p.word("CAST");
        p.p('(');
        let c = p.name()?;
        p.word("AS");
        p.name();
        if p.p('(') {
            p.i += 1;
            p.p(')');
        }
        p.p(')');
        p.p(')');
        p.word("LIKE");
        let v = p.value()?;
        return Some(Cond::Like(c, v.text()));
    }
    let mut eqs = Vec::new();
    loop {
        let c = p.name()?;
        if !p.p('=') {
            return None;
        }
        eqs.push((c, p.value()?));
        if !p.word("AND") {
            break;
        }
    }
    Some(Cond::Eq(eqs))
}

fn matches(t: &Table, row: &[Val], c: &Cond) -> bool {
    let at = |n: &str| t.columns.iter().position(|c| c.name == n);
    match c {
        Cond::Like(col, pat) => at(col).is_some_and(|i| like(pat, &row[i].text().to_uppercase())),
        Cond::Eq(eqs) => eqs.iter().all(|(col, v)| {
            at(col).is_some_and(|i| row[i].text().trim_end() == v.text().trim_end())
        }),
    }
}

/// A value checked against its column: the database's error for one
/// that does not fit.
fn check(c: &Column, v: &Val) -> Result<Val, Box<Outcome>> {
    if v.is_null() {
        if !c.nullable {
            return Err(Box::new(fail(
                "SQL0407",
                "23502",
                -407,
                &format!("Null values not allowed in column {}.", c.name),
            )));
        }
        return Ok(Val::Null);
    }
    let t = v.text();
    if c.numeric() {
        let ok = !t.is_empty()
            && t.trim_start_matches('-')
                .chars()
                .all(|ch| ch.is_ascii_digit() || ch == '.')
            && t.matches('.').count() <= 1;
        if !ok {
            return Err(Box::new(fail(
                "SQL0302",
                "22018",
                -302,
                &format!("Conversion error on value {t} for column {}.", c.name),
            )));
        }
        return Ok(Val::Num(t));
    }
    if matches!(c.typ.as_str(), "CHAR" | "VARCHAR") && t.chars().count() > c.length as usize {
        return Err(Box::new(fail(
            "SQL0404",
            "22001",
            -404,
            &format!("Value for column {} too long.", c.name),
        )));
    }
    if c.typ == "DATE" && (t.len() != 10 || t.as_bytes()[4] != b'-') {
        return Err(Box::new(fail(
            "SQL0181",
            "22007",
            -181,
            &format!("Value {t} in column {} not a valid date.", c.name),
        )));
    }
    Ok(Val::Text(t))
}

fn not_found(schema: &str, table: &str) -> Outcome {
    fail(
        "SQL0204",
        "42704",
        -204,
        &format!("{table} in {schema} not found."),
    )
}

impl FixtureDb {
    /// The prototype's history for `user` (the one the golden page
    /// shows), whatever ran since.
    pub fn sample_history(&self, user: &str) {
        let h = Self::default()
            .state
            .into_inner()
            .expect("db")
            .history
            .remove(USER_SAMPLE)
            .unwrap_or_default();
        self.state
            .lock()
            .expect("db")
            .history
            .insert(user.to_string(), h);
    }

    /// Rows of a table (for tests).
    pub fn rows(&self, schema: &str, table: &str) -> Vec<Vec<Val>> {
        self.state
            .lock()
            .expect("db")
            .tables
            .get(&(schema.to_string(), table.to_string()))
            .map(|t| t.rows.clone())
            .unwrap_or_default()
    }

    /// The prototype's Run SQL script's statements and what they did.
    fn canned(sql: &str) -> Option<Outcome> {
        let norm = sql.split_whitespace().collect::<Vec<_>>().join(" ");
        let text = |c: &str| ResultColumn {
            name: c.into(),
            typ: "CHAR".into(),
        };
        match norm.as_str() {
            "SELECT asset_tag, location, manufacturer, model, serial, asset_type, status, acquired FROM hlinv.assets WHERE status IN ('ACTIVE', 'SPARE') ORDER BY location, asset_tag" => {
                Some(Outcome {
                    columns: ASSETS_COLUMNS[..8].iter().map(|c| text(c)).collect(),
                    rows: ASSETS
                        .iter()
                        .map(|r| r.iter().map(|s| Val::Text((*s).into())).collect())
                        .collect(),
                    sqlstate: "00000".into(),
                    ms: 41,
                    ..Outcome::default()
                })
            }
            "SELECT location, COUNT(*) AS n FROM hlinv.assets GROUP BY location" => Some(Outcome {
                columns: vec![
                    text("LOCATION"),
                    ResultColumn {
                        name: "N".into(),
                        typ: "INTEGER".into(),
                    },
                ],
                rows: [
                    ("QS-HQ-R1", "4"),
                    ("QS-HQ-R2", "4"),
                    ("QS-HQ-R3", "1"),
                    ("QS-BR1-R1", "3"),
                ]
                .iter()
                .map(|(l, n)| vec![Val::Text((*l).into()), Val::Num((*n).into())])
                .collect(),
                sqlstate: "00000".into(),
                ms: 12,
                ..Outcome::default()
            }),
            "UPDATE hlinv.assets SET status = 'RETIRED' WHERE asset_tag = 'QS000301'" => {
                Some(Outcome {
                    changed: 1,
                    sqlstate: "00000".into(),
                    ms: 9,
                    ..Outcome::default()
                })
            }
            _ => None,
        }
    }

    #[allow(clippy::too_many_lines)] // one arm per statement
    fn exec(st: &mut State, conn: u32, sql: &str, params: &[Val]) -> Outcome {
        let schema = st
            .conns
            .get(&conn)
            .map(|c| c.opts.schema.clone())
            .unwrap_or_default();
        let toks = lex(sql);
        let mut p = P {
            t: &toks,
            i: 0,
            params,
            next: 0,
        };
        let changes = |st: &mut State| {
            if let Some(c) = st.conns.get(&conn)
                && c.opts.commit != Commit::None
                && c.before.is_none()
            {
                let snap = st.tables.clone();
                if let Some(c) = st.conns.get_mut(&conn) {
                    c.before = Some(snap);
                }
            }
        };
        let ok = |o: Outcome| Outcome {
            sqlstate: "00000".into(),
            ..o
        };
        if p.word("SELECT") {
            enum What {
                Count,
                Max(String),
                Cols(Vec<String>),
            }
            let what = if p.word("COUNT") {
                p.p('(');
                p.p('*');
                p.p(')');
                What::Count
            } else if p.word("MAX") {
                p.p('(');
                let c = p.name().unwrap_or_default();
                p.p(')');
                What::Max(c)
            } else if p.p('*') {
                What::Cols(Vec::new())
            } else {
                What::Cols(p.names())
            };
            if !p.word("FROM") {
                return syntax(&p.tok());
            }
            let Some(key) = p.table(&schema) else {
                return syntax(&p.tok());
            };
            let Some(t) = st.tables.get(&key) else {
                return not_found(&key.0, &key.1);
            };
            let c = if p.word("WHERE") {
                match cond(&mut p) {
                    Some(c) => Some(c),
                    None => return syntax(&p.tok()),
                }
            } else {
                None
            };
            let mut rows: Vec<&Vec<Val>> = t
                .rows
                .iter()
                .filter(|r| c.as_ref().is_none_or(|c| matches(t, r, c)))
                .collect();
            if p.word("ORDER") {
                p.word("BY");
                let order = p.names();
                let idx: Vec<(usize, bool)> = order
                    .iter()
                    .filter_map(|o| {
                        t.columns
                            .iter()
                            .position(|c| &c.name == o)
                            .map(|i| (i, t.columns[i].numeric()))
                    })
                    .collect();
                rows.sort_by(|a, b| {
                    for (i, num) in &idx {
                        let o = if *num {
                            let f = |v: &Val| v.text().parse::<f64>().unwrap_or(f64::MIN);
                            f(&a[*i]).total_cmp(&f(&b[*i]))
                        } else {
                            a[*i].text().cmp(&b[*i].text())
                        };
                        if o.is_ne() {
                            return o;
                        }
                    }
                    std::cmp::Ordering::Equal
                });
            }
            let mut offset = 0usize;
            if p.word("OFFSET") {
                if let Some(Tok::Num(n)) = p.peek() {
                    offset = n.parse().unwrap_or(0);
                    p.i += 1;
                }
                p.word("ROWS");
            }
            let mut limit = usize::MAX;
            if p.word("FETCH") {
                p.word("FIRST");
                if let Some(Tok::Num(n)) = p.peek() {
                    limit = n.parse().unwrap_or(usize::MAX);
                    p.i += 1;
                }
                p.word("ROWS");
                p.word("ONLY");
            }
            if p.peek().is_some() {
                return syntax(&p.tok());
            }
            let rows: Vec<&Vec<Val>> = rows.into_iter().skip(offset).take(limit).collect();
            return match what {
                What::Count => ok(Outcome {
                    columns: vec![ResultColumn {
                        name: "00001".into(),
                        typ: "INTEGER".into(),
                    }],
                    rows: vec![vec![Val::Num(rows.len().to_string())]],
                    ..Outcome::default()
                }),
                What::Max(c) => {
                    let Some(i) = t.columns.iter().position(|x| x.name == c) else {
                        return syntax(&c);
                    };
                    let m = rows
                        .iter()
                        .map(|r| r[i].clone())
                        .filter(|v| !v.is_null())
                        .max_by_key(Val::text)
                        .unwrap_or(Val::Null);
                    ok(Outcome {
                        columns: vec![ResultColumn {
                            name: "00001".into(),
                            typ: t.columns[i].typ.clone(),
                        }],
                        rows: vec![vec![m]],
                        ..Outcome::default()
                    })
                }
                What::Cols(names) => {
                    let names = if names.is_empty() {
                        t.columns.iter().map(|c| c.name.clone()).collect()
                    } else {
                        names
                    };
                    let mut idx = Vec::new();
                    for n in &names {
                        match t.columns.iter().position(|c| &c.name == n) {
                            Some(i) => idx.push(i),
                            None => {
                                return fail(
                                    "SQL0206",
                                    "42703",
                                    -206,
                                    &format!("Column {n} not in specified tables."),
                                );
                            }
                        }
                    }
                    ok(Outcome {
                        columns: idx
                            .iter()
                            .map(|i| ResultColumn {
                                name: t.columns[*i].name.clone(),
                                typ: t.columns[*i].typ.clone(),
                            })
                            .collect(),
                        rows: rows
                            .iter()
                            .map(|r| idx.iter().map(|i| r[*i].clone()).collect())
                            .collect(),
                        ..Outcome::default()
                    })
                }
            };
        }
        if p.word("INSERT") {
            p.word("INTO");
            let Some(key) = p.table(&schema) else {
                return syntax(&p.tok());
            };
            let Some(t) = st.tables.get(&key).cloned() else {
                return not_found(&key.0, &key.1);
            };
            let names = if p.p('(') {
                let n = p.names();
                p.p(')');
                n
            } else {
                t.columns.iter().map(|c| c.name.clone()).collect()
            };
            if !p.word("VALUES") || !p.p('(') {
                return syntax(&p.tok());
            }
            let mut vals = Vec::new();
            loop {
                let Some(v) = p.value() else {
                    return syntax(&p.tok());
                };
                vals.push(v);
                if !p.p(',') {
                    break;
                }
            }
            p.p(')');
            let mut row = Vec::new();
            for c in &t.columns {
                let v = match names.iter().position(|n| *n == c.name) {
                    Some(i) => vals.get(i).cloned().unwrap_or(Val::Null),
                    None if c.default.starts_with("CURRENT") => Val::Text(CHANGED.into()),
                    None if c.nullable => Val::Null,
                    None => {
                        return fail(
                            "SQL0407",
                            "23502",
                            -407,
                            &format!("Null values not allowed in column {}.", c.name),
                        );
                    }
                };
                match check(c, &v) {
                    Ok(v) => row.push(v),
                    Err(o) => return *o,
                }
            }
            if let Some(k) = t.columns.iter().position(|c| c.key)
                && t.rows.iter().any(|r| r[k].text() == row[k].text())
            {
                return fail(
                    "SQL0803",
                    "23505",
                    -803,
                    &format!("Duplicate key value specified for {}.", key.1),
                );
            }
            changes(st);
            if let Some(t) = st.tables.get_mut(&key) {
                t.rows.push(row);
            }
            return ok(Outcome {
                changed: 1,
                ..Outcome::default()
            });
        }
        if p.word("UPDATE") {
            let Some(key) = p.table(&schema) else {
                return syntax(&p.tok());
            };
            let Some(t) = st.tables.get(&key).cloned() else {
                return not_found(&key.0, &key.1);
            };
            if !p.word("SET") {
                return syntax(&p.tok());
            }
            let mut sets = Vec::new();
            loop {
                let Some(n) = p.name() else {
                    return syntax(&p.tok());
                };
                p.p('=');
                let Some(v) = p.value() else {
                    return syntax(&p.tok());
                };
                let Some(i) = t.columns.iter().position(|c| c.name == n) else {
                    return fail(
                        "SQL0206",
                        "42703",
                        -206,
                        &format!("Column {n} not in specified tables."),
                    );
                };
                match check(&t.columns[i], &v) {
                    Ok(v) => sets.push((i, v)),
                    Err(o) => return *o,
                }
                if !p.p(',') {
                    break;
                }
            }
            let c = if p.word("WHERE") { cond(&mut p) } else { None };
            changes(st);
            let mut n = 0;
            if let Some(tm) = st.tables.get_mut(&key) {
                for r in &mut tm.rows {
                    if c.as_ref().is_none_or(|c| matches(&t, r, c)) {
                        for (i, v) in &sets {
                            r[*i] = v.clone();
                        }
                        n += 1;
                    }
                }
            }
            return ok(Outcome {
                changed: n,
                ..Outcome::default()
            });
        }
        if p.word("DELETE") {
            p.word("FROM");
            let Some(key) = p.table(&schema) else {
                return syntax(&p.tok());
            };
            let Some(t) = st.tables.get(&key).cloned() else {
                return not_found(&key.0, &key.1);
            };
            let c = if p.word("WHERE") { cond(&mut p) } else { None };
            changes(st);
            let before = t.rows.len();
            if let Some(tm) = st.tables.get_mut(&key) {
                tm.rows
                    .retain(|r| !c.as_ref().is_none_or(|c| matches(&t, r, c)));
                return ok(Outcome {
                    changed: i64::try_from(before - tm.rows.len()).unwrap_or(0),
                    ..Outcome::default()
                });
            }
        }
        syntax(&p.tok())
    }
}

impl Db for FixtureDb {
    fn schemas(&self, _user: &str, pattern: &str) -> Vec<String> {
        CATALOG
            .iter()
            .filter(|s| like(pattern, s))
            .map(ToString::to_string)
            .collect()
    }

    fn tables(&self, _user: &str, schema: &str) -> Result<Vec<Entry>, Message> {
        let st = self.state.lock().expect("db");
        if schema == "HLINV" {
            return Ok(HLINV
                .iter()
                .map(|(name, kind, text, rows, (m, d, h, mi))| Entry {
                    name: (*name).into(),
                    kind: *kind,
                    text: (*text).into(),
                    rows: if *name == "ASSETS" {
                        st.tables
                            .get(&("HLINV".into(), "ASSETS".into()))
                            .map_or(0, |t| t.rows.len() as u64)
                    } else {
                        *rows
                    },
                    changed: if *m == 0 { 0 } else { stamp(*m, *d, *h, *mi) },
                })
                .collect());
        }
        if !CATALOG.contains(&schema) {
            return Err(Message::new(
                MsgKind::Error,
                "SQL0204",
                &format!("{schema} in QSYS not found."),
            ));
        }
        Ok(st
            .tables
            .iter()
            .filter(|((s, _), _)| s == schema)
            .map(|((_, n), t)| Entry {
                name: n.clone(),
                kind: Kind::Table,
                rows: t.rows.len() as u64,
                ..Entry::default()
            })
            .collect())
    }

    fn columns(&self, _user: &str, schema: &str, table: &str) -> Result<Vec<Column>, Message> {
        let st = self.state.lock().expect("db");
        st.tables
            .get(&(schema.to_string(), table.to_string()))
            .map(|t| t.columns.clone())
            .ok_or_else(|| {
                Message::new(
                    MsgKind::Error,
                    "SQL0204",
                    &format!("{table} in {schema} not found."),
                )
            })
    }

    fn open(&self, user: &str) -> Result<u32, Message> {
        let mut st = self.state.lock().expect("db");
        if st.conns.values().filter(|c| c.user == user).count() >= MAX_CONNECTIONS {
            let text = crate::msgs::second_level(crate::msgs::NO_CONNECTIONS, &["QSHQ400"])
                .map(|s| s.text)
                .unwrap_or_default();
            return Err(
                Message::new(MsgKind::Error, crate::msgs::NO_CONNECTIONS, &text)
                    .with_data(&["QSHQ400"]),
            );
        }
        st.next += 1;
        let n = st.next;
        st.conns.insert(
            n,
            Conn {
                user: user.into(),
                ..Conn::default()
            },
        );
        Ok(n)
    }

    fn close(&self, _user: &str, conn: u32) {
        let mut st = self.state.lock().expect("db");
        if let Some(c) = st.conns.remove(&conn)
            && let Some(before) = c.before
        {
            st.tables = before;
        }
    }

    fn options(&self, _user: &str, conn: u32, opts: &Options) -> Result<(), Message> {
        let mut st = self.state.lock().expect("db");
        let Some(c) = st.conns.get_mut(&conn) else {
            return Err(Message::new(
                MsgKind::Error,
                "SQL0901",
                "The connection is not open.",
            ));
        };
        if c.before.is_some()
            && (c.opts.commit != opts.commit
                || c.opts.system_naming != opts.system_naming
                || c.opts.schema != opts.schema)
        {
            return Err(Message::new(
                MsgKind::Error,
                "SQL0752",
                "The connection's SQL options cannot change while a transaction is open.",
            ));
        }
        c.opts = opts.clone();
        Ok(())
    }

    fn run(
        &self,
        user: &str,
        conn: u32,
        sql: &str,
        params: &[Vec<Val>],
        max_rows: usize,
    ) -> Outcome {
        self.ran
            .lock()
            .expect("ran")
            .push((user.to_string(), sql.to_string()));
        let mut st = self.state.lock().expect("db");
        if !st.conns.contains_key(&conn) {
            return fail("SQL0901", "58004", -901, "The connection is not open.");
        }
        if let Some(mut o) = Self::canned(sql) {
            if o.changed > 0
                && let Some(c) = st.conns.get(&conn)
                && c.opts.commit != Commit::None
                && c.before.is_none()
            {
                let snap = st.tables.clone();
                if let Some(c) = st.conns.get_mut(&conn) {
                    c.before = Some(snap);
                }
            }
            if o.rows.len() > max_rows {
                let rest = o.rows.split_off(max_rows);
                st.next += 1;
                let cur = st.next;
                o.cursor = Some(cur);
                if let Some(c) = st.conns.get_mut(&conn) {
                    c.cursors.insert(cur, (o.columns.clone(), rest));
                }
            }
            return o;
        }
        let rows: Vec<Vec<Val>> = if params.is_empty() {
            vec![Vec::new()]
        } else {
            params.to_vec()
        };
        let mut total = Outcome::default();
        for (i, row) in rows.iter().enumerate() {
            let mut o = Self::exec(&mut st, conn, sql, row);
            if o.failed {
                o.row = (!params.is_empty()).then_some(i);
                o.changed = total.changed;
                return o;
            }
            total.changed += o.changed;
            if o.query() {
                if o.rows.len() > max_rows {
                    let rest = o.rows.split_off(max_rows);
                    st.next += 1;
                    let cur = st.next;
                    o.cursor = Some(cur);
                    if let Some(c) = st.conns.get_mut(&conn) {
                        c.cursors.insert(cur, (o.columns.clone(), rest));
                    }
                }
                return o;
            }
            total.sqlstate = o.sqlstate;
        }
        total
    }

    fn fetch(&self, _user: &str, conn: u32, cursor: u32, max_rows: usize) -> Outcome {
        let mut st = self.state.lock().expect("db");
        let Some(c) = st.conns.get_mut(&conn) else {
            return fail("SQL0901", "58004", -901, "The connection is not open.");
        };
        let Some((cols, mut rest)) = c.cursors.remove(&cursor) else {
            return fail("SQL0501", "24501", -501, "Cursor is not open.");
        };
        let left = if rest.len() > max_rows {
            rest.split_off(max_rows)
        } else {
            Vec::new()
        };
        let more = !left.is_empty();
        if more {
            c.cursors.insert(cursor, (cols.clone(), left));
        }
        Outcome {
            columns: cols,
            rows: rest,
            cursor: more.then_some(cursor),
            sqlstate: "00000".into(),
            ..Outcome::default()
        }
    }

    fn end_unit(&self, _user: &str, conn: u32, commit: bool) -> Outcome {
        let mut st = self.state.lock().expect("db");
        let before = st.conns.get_mut(&conn).and_then(|c| c.before.take());
        if !commit && let Some(b) = before {
            st.tables = b;
        }
        Outcome {
            sqlstate: "00000".into(),
            ..Outcome::default()
        }
    }

    fn cancel(&self, _user: &str, _conn: u32) {}

    fn requests(&self, user: &str, groups: &[String]) -> Vec<Request> {
        let st = self.state.lock().expect("db");
        st.requests
            .iter()
            .filter(|r| {
                r.owner == user || r.shared == crate::policy::PUBLIC || groups.contains(&r.shared)
            })
            .cloned()
            .collect()
    }

    fn save_request(&self, user: &str, r: &Request) -> Result<u32, Message> {
        let mut st = self.state.lock().expect("db");
        let mut r = r.clone();
        r.owner = user.to_string();
        if r.id == 0 {
            r.id = st
                .requests
                .iter()
                .filter(|x| x.owner == user)
                .map(|x| x.id)
                .max()
                .unwrap_or(0)
                + 1;
            st.requests.push(r.clone());
        } else if let Some(x) = st
            .requests
            .iter_mut()
            .find(|x| x.owner == user && x.id == r.id)
        {
            *x = r.clone();
        } else {
            return Err(Message::new(
                MsgKind::Error,
                "CPF9801",
                &format!("Request {} not found.", r.id),
            ));
        }
        Ok(r.id)
    }

    fn delete_request(&self, user: &str, id: u32) -> Result<(), Message> {
        let mut st = self.state.lock().expect("db");
        let before = st.requests.len();
        st.requests.retain(|r| !(r.owner == user && r.id == id));
        if st.requests.len() == before {
            return Err(Message::new(
                MsgKind::Error,
                "CPF9801",
                &format!("Request {id} not found."),
            ));
        }
        Ok(())
    }

    fn history(&self, user: &str) -> Vec<String> {
        self.state
            .lock()
            .expect("db")
            .history
            .get(user)
            .cloned()
            .unwrap_or_default()
    }

    fn remember(&self, user: &str, statements: &[String]) {
        let mut st = self.state.lock().expect("db");
        let h = st.history.entry(user.to_string()).or_default();
        for s in statements {
            h.retain(|x| x != s);
            h.insert(0, s.clone());
        }
        h.truncate(100);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_pages_sql_runs() {
        let db = FixtureDb::default();
        let c = db.open("CWELLMAN").unwrap();
        let cols: Vec<String> = ASSETS_COLUMNS.iter().map(ToString::to_string).collect();
        let key = vec!["ASSET_TAG".to_string()];
        let o = db.run(
            "CWELLMAN",
            c,
            &crate::data::sqltext::page("HLINV", "ASSETS", &cols, Some("MANUFACTURER"), &key, 1, 2),
            &[vec![Val::Text("%DELL%".into())]],
            500,
        );
        assert_eq!(o.rows.len(), 2, "{o:?}");
        assert_eq!(o.rows[0][0], Val::Text("QS000202".into()));
        let ins = crate::data::sqltext::insert("HLINV", "ASSETS", &cols[..7]);
        let row = |t: &str| {
            [
                "QS000404",
                "QS-BR1-R1",
                "ARISTA",
                "7050SX-64",
                t,
                "SWITCH",
                "SPARE",
            ]
            .iter()
            .map(|s| Val::Text((*s).into()))
            .collect::<Vec<_>>()
        };
        assert_eq!(db.run("CWELLMAN", c, &ins, &[row("JPE1")], 500).changed, 1);
        let dup = db.run("CWELLMAN", c, &ins, &[row("JPE2")], 500);
        assert_eq!((dup.sqlcode, dup.row), (-803, Some(0)));
        let n = db.run(
            "CWELLMAN",
            c,
            &crate::data::sqltext::count("HLINV", "ASSETS", None),
            &[],
            500,
        );
        assert_eq!(n.rows[0][0], Val::Num("13".into()));
        assert_eq!(db.run("CWELLMAN", c, "SELEKT 1", &[], 1).sqlcode, -104);
        assert!(like("HL%", "HLINVTEST") && !like("HL_", "HLINV"));
    }
}
