// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database server (8471): the SQL protocol JDBC and ODBC clients
//! speak, over the job's SQL session (`sql::embed::Embedded`). A
//! statement lives in a request parameter block (RPB): it is prepared
//! and described, its markers' format comes as a descriptor, then it is
//! executed or opened and fetched in blocks. Replies carry what the
//! request's operation result set (ORS) bitmap asks for. The catalog
//! procedures the driver calls for metadata are answered here.

pub mod catalog;
pub mod data;
pub mod format;
pub mod sqlca;
pub mod types;

use std::collections::HashMap;

use hostsvr::cp::{db_reply, db_request};
use hostsvr::database::param::{DbString, expanded, statement_text};
use hostsvr::database::{id, ors};
use hostsvr::frame::{Frame, Header, Param};
use regfac::ExitRequest;
use regfac::formats::host::{Sql, func, zdai0100, zdaq0100, zdaq0200};
use sql::embed::{Desc, Embedded, Op, Reply, Request};
use sql::sqlca::Sqlca;
use sql::value::Value;

use catalog::{Call, Rows};
use types::ColType;

use crate::exits::{Exits, who};
use crate::session::{Auth, Session};
use crate::{Handler, Outcome, reply, text};

/// The exit points.
pub const EXIT_INIT: &str = "QIBM_QZDA_INIT";
/// SQL requests, short format.
pub const EXIT_SQL1: &str = "QIBM_QZDA_SQL1";
/// SQL requests, whole statements.
pub const EXIT_SQL2: &str = "QIBM_QZDA_SQL2";

/// Replies longer than this are compressed when the client allows it.
const RLE_THRESHOLD: usize = 1024;

/// The scope the server's statements run in.
const SCOPE: &str = "QZDASOINIT:";

/// Set attributes code points.
mod attr {
    pub const DATE_FORMAT: u16 = 0x3807;
    pub const NAMING: u16 = 0x380C;
    pub const COMMIT: u16 = 0x380E;
    pub const DEFAULT_LIBRARY: u16 = 0x380F;
    pub const AUTOCOMMIT: u16 = 0x3824;
    pub const INTERFACE_TYPE: u16 = 0x383C;
    pub const INTERFACE_NAME: u16 = 0x383D;
    pub const INTERFACE_LEVEL: u16 = 0x383E;
}

/// Who the server is.
#[derive(Clone, Debug, Default)]
pub struct DbConfig {
    /// The relational database name (the system name).
    pub rdb: String,
    /// The server job: name, user, number.
    pub job: (String, String, String),
}

/// A statement in a request parameter block.
#[derive(Clone, Debug, Default)]
struct Rpb {
    stmt: String,
    cursor: String,
    text: String,
    select: bool,
    cols: Vec<ColType>,
    markers: Vec<ColType>,
    call: Option<Call>,
    results: Option<Rows>,
    pos: usize,
    open: bool,
}

/// What the last request left for send results.
#[derive(Clone, Debug, Default)]
struct Last {
    function: u16,
    sqlca: Sqlca,
}

/// The server.
pub struct Database<A, E, X> {
    /// Sign-on state.
    pub session: Session,
    /// Signing on.
    pub auth: A,
    /// The job's SQL.
    pub sql: E,
    /// Exit programs.
    pub exits: X,
    /// Who the server is.
    pub config: DbConfig,
    rpbs: HashMap<u16, Rpb>,
    descriptors: HashMap<u16, Vec<ColType>>,
    last: Last,
    commit: u16,
    autocommit: bool,
    sql_naming: bool,
    date_format: u16,
    default_lib: String,
}

/// A request's template.
#[derive(Clone, Copy, Debug, Default)]
struct Req {
    function: u16,
    ors: u32,
    rpb: u16,
    descriptor: u16,
}

impl Req {
    fn of(f: &Frame) -> Self {
        let t = &f.template;
        Self {
            function: f.header.req_rep_id,
            ors: reply::be32(t, 0),
            rpb: reply::be16(t, 14),
            descriptor: reply::be16(t, 16),
        }
    }

    fn wants(&self, bit: u32) -> bool {
        self.ors & bit != 0
    }
}

/// What a reply carries beyond its parameters.
struct Answer {
    /// The function whose results these are.
    of: u16,
    class: u16,
    code: i32,
    params: Vec<Param>,
}

fn db_text(p: &Param) -> String {
    DbString::decode(&p.data, false)
        .ok()
        .and_then(|s| s.to_text())
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn short(ccsid: u16, b: &[u8]) -> Vec<u8> {
    DbString {
        ccsid,
        long: false,
        text: b.to_vec(),
    }
    .encode()
}

impl<A: Auth, E: Embedded, X: Exits> Database<A, E, X> {
    /// A server for one connection.
    pub fn new(auth: A, sql: E, exits: X, config: DbConfig) -> Self {
        Self {
            session: Session::default(),
            auth,
            sql,
            exits,
            config,
            rpbs: HashMap::new(),
            descriptors: HashMap::new(),
            last: Last::default(),
            commit: 0,
            autocommit: true,
            sql_naming: true,
            date_format: 1,
            default_lib: String::new(),
        }
    }

    fn user(&self) -> String {
        self.session.user.clone().unwrap_or_default()
    }

    fn desc(&self, op: Op) -> Desc {
        let commit = if self.autocommit || self.commit == 0 {
            None
        } else {
            sql::embed::commit_level(match self.commit {
                1 => "*CS",
                2 => "*CHG",
                _ => "*ALL",
            })
        };
        Desc {
            op,
            sql_naming: self.sql_naming,
            commit,
            ..Desc::default()
        }
    }

    fn run(&mut self, desc: Desc, inputs: Vec<Value>) -> Reply {
        self.sql.run(&Request {
            scope: SCOPE.into(),
            desc,
            inputs,
        })
    }

    fn frame(h: &Header, r: &Req, a: Answer) -> Vec<u8> {
        let mut params = a.params;
        let mut compressed = 0u32;
        // Large replies travel run-length encoded when the client asks.
        if r.wants(ors::REPLY_RLE) {
            let mut raw = Vec::new();
            hostsvr::frame::param::encode_all(&params, &mut raw);
            if raw.len() > RLE_THRESHOLD
                && let Some(c) = hostsvr::rle::compress(&raw)
            {
                let mut d = reply::len32(raw.len()).to_be_bytes().to_vec();
                d.extend(c);
                params = vec![Param::new(db_reply::COMPRESSED, d)];
                compressed = 0x8000_0000;
            }
        }
        let mut t = r.ors.to_be_bytes().to_vec();
        t.extend(compressed.to_be_bytes());
        t.extend(r.rpb.to_be_bytes());
        t.extend(r.function.to_be_bytes());
        t.extend(a.of.to_be_bytes());
        t.extend(a.class.to_be_bytes());
        t.extend(a.code.to_be_bytes());
        reply::frame(h, id::REPLY, t, params)
    }

    /// The SQLCA and messages a request asks for.
    fn results(r: &Req, ca: &Sqlca, with_sqlca: bool) -> Vec<Param> {
        let mut v = Vec::new();
        if ca.sqlcode != 0 {
            if r.wants(ors::MESSAGE_ID) {
                let mut d = 37u16.to_be_bytes().to_vec();
                d.extend(text::ebcdic(&ca.msgid()));
                v.push(Param::new(db_reply::MESSAGE_ID, d));
            }
            let msg = if ca.sqlerrmc.is_empty() {
                format!("The statement ended with SQLCODE {}.", ca.sqlcode)
            } else {
                ca.sqlerrmc.clone()
            };
            if r.wants(ors::FIRST_LEVEL_TEXT) {
                v.push(Param::new(db_reply::FIRST_LEVEL_TEXT, short(37, &text::ebcdic(&msg))));
            }
            if r.wants(ors::SECOND_LEVEL_TEXT) {
                let help = format!("SQLSTATE {}: correct the statement or the objects it names and run it again.", ca.sqlstate);
                v.push(Param::new(db_reply::SECOND_LEVEL_TEXT, short(37, &text::ebcdic(&help))));
            }
        }
        if with_sqlca && r.wants(ors::SQLCA) {
            v.push(Param::new(db_reply::SQLCA, sqlca::encode(ca)));
        }
        v
    }

    fn class_of(ca: &Sqlca) -> (u16, i32) {
        if ca.sqlcode == 0 { (0, 0) } else { (1, ca.sqlcode) }
    }

    fn sql_allowed(&mut self, function: i32, rpb: &Rpb, text: &str) -> bool {
        let s = Sql {
            statement: rpb.stmt.clone(),
            cursor: rpb.cursor.clone(),
            text: text.to_string(),
            default_lib: self.default_lib.clone(),
            commit: if self.commit == 0 { 'N' } else { 'C' },
            ..Sql::default()
        };
        let w = who(&self.user(), function);
        self.exits.allow(EXIT_SQL1, &ExitRequest::host("ZDAQ0100", zdaq0100(&w, &s)))
            && self.exits.allow(EXIT_SQL2, &ExitRequest::host("ZDAQ0200", zdaq0200(&w, &s)))
    }

    fn set_attributes(&mut self, f: &Frame, params: &[Param]) -> Answer {
        let mut iface = (String::new(), String::new(), String::new());
        for p in params {
            match p.cp {
                attr::COMMIT => self.commit = p.as_u16().unwrap_or(0),
                attr::AUTOCOMMIT => self.autocommit = p.data.first() != Some(&0xD5),
                attr::NAMING => self.sql_naming = p.as_u16().unwrap_or(0) == 0,
                attr::DATE_FORMAT => self.date_format = p.as_u16().unwrap_or(1),
                attr::DEFAULT_LIBRARY => self.default_lib = db_text(p),
                attr::INTERFACE_TYPE => iface.0 = db_text(p),
                attr::INTERFACE_NAME => iface.1 = db_text(p),
                attr::INTERFACE_LEVEL => iface.2 = db_text(p),
                _ => {}
            }
        }
        let _ = f;
        let req = ExitRequest::host(
            "ZDAI0100",
            zdai0100(&who(&self.user(), func::DB_INIT), (&iface.0, &iface.1, &iface.2)),
        );
        if !self.exits.allow(EXIT_INIT, &req) {
            return Answer {
                of: id::SET_ATTRIBUTES,
                class: 8,
                code: -1,
                params: Vec::new(),
            };
        }
        Answer {
            of: id::SET_ATTRIBUTES,
            class: 0,
            code: 0,
            params: vec![Param::new(db_reply::SERVER_ATTRIBUTES, self.server_attributes())],
        }
    }

    fn server_attributes(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(118);
        v.extend(0u16.to_be_bytes());
        v.extend(self.date_format.to_be_bytes());
        for _ in 0..4 {
            v.extend(0u16.to_be_bytes());
        }
        v.extend(u16::from(!self.sql_naming).to_be_bytes());
        v.extend(1u16.to_be_bytes());
        v.extend(self.commit.to_be_bytes());
        v.extend(1u16.to_be_bytes());
        v.push(0xF0);
        v.extend(37u16.to_be_bytes());
        v.extend(0u16.to_be_bytes());
        v.extend(text::ebcdic("ENU"));
        v.extend(text::ebcdic_fixed("*HEX", 10));
        v.extend(text::ebcdic_fixed("", 10));
        v.extend(text::ebcdic("2924"));
        v.extend(text::ebcdic("V7R1M00014"));
        v.extend(text::ebcdic_fixed(&self.config.rdb, 18));
        v.extend(text::ebcdic_fixed(&self.default_lib, 10));
        let (n, u, num) = &self.config.job;
        v.extend(text::ebcdic_fixed(n, 10));
        v.extend(text::ebcdic_fixed(u, 10));
        v.extend(text::ebcdic_fixed(num, 6));
        v.extend(0u16.to_be_bytes());
        v
    }

    fn create_rpb(&mut self, r: &Req, params: &[Param]) {
        let e = self.rpbs.entry(r.rpb).or_default();
        for p in params {
            match p.cp {
                db_request::STATEMENT_NAME => e.stmt = db_text(p),
                db_request::CURSOR_NAME => e.cursor = db_text(p),
                _ => {}
            }
        }
        if e.stmt.is_empty() {
            e.stmt = format!("STMT{:04}", r.rpb);
        }
        if e.cursor.is_empty() {
            e.cursor = format!("CRSR{:04}", r.rpb);
        }
    }

    fn rpb(&mut self, r: &Req) -> Rpb {
        if !self.rpbs.contains_key(&r.rpb) {
            self.create_rpb(r, &[]);
        }
        self.rpbs[&r.rpb].clone()
    }

    fn markers_of(&self, r: &Req, rpb: &Rpb, params: &[Param]) -> Vec<Vec<Value>> {
        let fmt = self.descriptors.get(&r.descriptor).cloned().unwrap_or_else(|| rpb.markers.clone());
        let data = params
            .iter()
            .find(|p| p.cp == db_request::EXTENDED_MARKER_DATA || p.cp == db_request::MARKER_DATA)
            .map(|p| p.data.clone());
        match data {
            Some(d) if !fmt.is_empty() => data::decode_rows(&d, &fmt),
            _ => vec![Vec::new()],
        }
    }

    fn prepare(&mut self, r: &Req, params: &[Param], describe: bool) -> Answer {
        let mut rpb = self.rpb(r);
        let text = match statement_text(params) {
            Some(Ok(s)) => s.to_text().unwrap_or_default(),
            _ => rpb.text.clone(),
        };
        rpb.text = text.trim().to_string();
        rpb.results = None;
        rpb.cols.clear();
        rpb.markers.clear();
        rpb.call = catalog::parse(&rpb.text);
        let fail = |ca: Sqlca| (ca, Vec::new(), Vec::new());
        let (ca, cols, markers) = if !self.sql_allowed(i32::from(r.function), &rpb, &rpb.text.clone()) {
            fail(denied())
        } else if rpb.text.len() > 5 && rpb.text[..5].eq_ignore_ascii_case("CALL ") && rpb.call.is_none() {
            // QS/400 has no stored procedures of its own yet.
            let mut ca = code(-204, "42704");
            ca.sqlerrmc = format!("{} is not a procedure here.", rpb.text[5..].split('(').next().unwrap_or("").trim());
            fail(ca)
        } else if let Some(c) = &rpb.call {
            let m = (0..c.markers())
                .map(|i| ColType::varchar("", if i < 3 { 128 } else { 4000 }, false))
                .collect();
            (Sqlca::default(), Vec::new(), m)
        } else {
            let d = Desc {
                name: rpb.stmt.clone(),
                into: true,
                ..self.desc(Op::Prepare)
            };
            let rep = self.run(d, vec![Value::Str(rpb.text.clone())]);
            if rep.sqlca.is_error() {
                fail(rep.sqlca)
            } else {
                let cols = cols_of(&rep.rows);
                let d = Desc {
                    name: rpb.stmt.clone(),
                    orient: "INPUT".into(),
                    ..self.desc(Op::Describe)
                };
                let m = self.run(d, Vec::new());
                let markers: Vec<ColType> = cols_of(&m.rows)
                    .into_iter()
                    .map(|mut c| {
                        c.name.clear();
                        c.column = true;
                        c
                    })
                    .collect();
                (Sqlca::default(), cols, markers)
            }
        };
        rpb.select = !cols.is_empty();
        rpb.cols = cols;
        rpb.markers = markers;
        let mut ca = ca;
        if !ca.is_error() {
            // SQLERRD(5): how many markers the statement has.
            ca.sqlerrd[4] = i32::try_from(rpb.markers.len()).unwrap_or(0);
        }
        self.rpbs.insert(r.rpb, rpb.clone());
        let (class, code) = Self::class_of(&ca);
        let mut params = Self::results(r, &ca, true);
        if describe && r.wants(ors::DATA_FORMAT) {
            let d = if rpb.cols.is_empty() { Vec::new() } else { format::encode(&rpb.cols, true) };
            params.push(Param::new(db_reply::SUPER_DATA_FORMAT, d));
        }
        if describe && r.wants(ors::MARKER_FORMAT) {
            let d = if rpb.markers.is_empty() { Vec::new() } else { format::encode(&rpb.markers, false) };
            params.push(Param::new(db_reply::SUPER_MARKER_FORMAT, d));
        }
        self.last = Last { function: r.function, sqlca: ca };
        Answer { of: r.function, class, code, params }
    }

    fn execute(&mut self, r: &Req, params: &[Param]) -> Answer {
        let mut rpb = self.rpb(r);
        let rows = self.markers_of(r, &rpb, params);
        let mut extra = Vec::new();
        let ca = if let Some(c) = rpb.call.clone() {
            let args = c.values(rows.first().map_or(&[][..], Vec::as_slice));
            let rdb = self.config.rdb.clone();
            let results = catalog::answer(&c, &args, &rdb, &mut |q: &str| self.query(q));
            rpb.results = Some(results);
            rpb.pos = 0;
            if r.wants(ors::CURSOR_ATTRIBUTES) {
                extra.push(cursor_attributes());
            }
            Sqlca {
                sqlcode: 466,
                sqlstate: "0100C".into(),
                sqlerrd: [0, 1, 0, 0, 0, 0],
                ..Sqlca::default()
            }
        } else {
            let mut total = 0;
            let mut last = Sqlca::default();
            for row in rows {
                let d = Desc {
                    name: rpb.stmt.clone(),
                    ..self.desc(Op::Execute)
                };
                let rep = self.run(d, row);
                total += rep.sqlca.sqlerrd[2];
                last = rep.sqlca;
                if last.is_error() {
                    break;
                }
            }
            if !last.is_error() {
                last.sqlerrd[2] = total;
            }
            last
        };
        self.rpbs.insert(r.rpb, rpb);
        self.finish(r, ca, extra)
    }

    fn finish(&mut self, r: &Req, ca: Sqlca, extra: Vec<Param>) -> Answer {
        let (class, code) = Self::class_of(&ca);
        let mut params = Self::results(r, &ca, true);
        params.extend(extra);
        self.last = Last { function: r.function, sqlca: ca };
        Answer { of: r.function, class, code, params }
    }

    /// Rows of a query the server runs for itself.
    fn query(&mut self, q: &str) -> Vec<Vec<Value>> {
        let name = "QZDAQRY".to_string();
        let d = Desc {
            name: name.clone(),
            text: q.into(),
            ..self.desc(Op::Open)
        };
        let o = self.run(d, Vec::new());
        if o.sqlca.is_error() {
            return Vec::new();
        }
        let d = Desc {
            name: name.clone(),
            rows: true,
            ..self.desc(Op::Fetch)
        };
        let f = self.run(d, vec![Value::int(1_000_000)]);
        let d = Desc {
            name,
            ..self.desc(Op::Close)
        };
        let _ = self.run(d, Vec::new());
        f.rows
    }

    fn exec_immediate(&mut self, r: &Req, params: &[Param]) -> Answer {
        let text = match statement_text(params) {
            Some(Ok(s)) => s.to_text().unwrap_or_default(),
            _ => String::new(),
        };
        let rpb = self.rpb(r);
        let ca = if self.sql_allowed(i32::from(r.function), &rpb, &text) {
            self.run(self.desc(Op::ExecImm), vec![Value::Str(text)]).sqlca
        } else {
            denied()
        };
        self.finish(r, ca, Vec::new())
    }

    fn open(&mut self, r: &Req, params: &[Param], fetch: bool) -> Answer {
        let mut rpb = self.rpb(r);
        let mut ca = Sqlca::default();
        if rpb.call.is_some() {
            if rpb.results.is_none() {
                ca = code(-501, "24501");
            }
            rpb.pos = 0;
        } else {
            let row = self.markers_of(r, &rpb, params).into_iter().next().unwrap_or_default();
            let d = Desc {
                name: rpb.cursor.clone(),
                stmt: rpb.stmt.clone(),
                hold: true,
                ..self.desc(Op::Open)
            };
            ca = self.run(d, row).sqlca;
        }
        rpb.open = !ca.is_error();
        let cols = rpb.results.as_ref().map_or_else(|| rpb.cols.clone(), |x| x.cols.clone());
        self.rpbs.insert(r.rpb, rpb);
        let mut params_out = Vec::new();
        let mut class_code = Self::class_of(&ca);
        if fetch && !ca.is_error() {
            let n = block_rows(params, &cols);
            let (rows, end) = self.fetch_rows(r.rpb, n);
            ca = Sqlca::rows(rows.len() as u64);
            ca.sqlerrd[3] = i32::try_from(cols.iter().map(|c| c.len).sum::<u32>()).unwrap_or(0);
            if end {
                ca.sqlerrd[4] = 100;
                class_code = (2, 700);
            }
            params_out.extend(Self::results(r, &ca, true));
            if r.wants(ors::CURSOR_ATTRIBUTES) {
                params_out.push(cursor_attributes());
            }
            if r.wants(ors::DATA_FORMAT) {
                params_out.push(Param::new(db_reply::SUPER_DATA_FORMAT, format::encode(&cols, true)));
            }
            params_out.push(Param::new(db_reply::EXTENDED_RESULT_DATA, data::encode_rows(&cols, &rows, 1)));
        } else {
            if r.wants(ors::SQLCA) {
                // A cursor that opened answers an empty SQLCA.
                let d = if ca.sqlcode == 0 { Vec::new() } else { sqlca::encode(&ca) };
                params_out.push(Param::new(db_reply::SQLCA, d));
            }
            if r.wants(ors::CURSOR_ATTRIBUTES) {
                params_out.push(cursor_attributes());
            }
            if r.wants(ors::DATA_FORMAT) && !ca.is_error() {
                params_out.push(Param::new(db_reply::SUPER_DATA_FORMAT, format::encode(&cols, true)));
            }
        }
        self.last = Last { function: r.function, sqlca: ca };
        Answer {
            of: r.function,
            class: class_code.0,
            code: class_code.1,
            params: params_out,
        }
    }

    /// Up to `n` rows from the RPB's cursor; whether the end was met.
    fn fetch_rows(&mut self, rpb_id: u16, n: usize) -> (Vec<Vec<Value>>, bool) {
        let Some(mut rpb) = self.rpbs.get(&rpb_id).cloned() else {
            return (Vec::new(), true);
        };
        let out = if let Some(res) = &rpb.results {
            let end = (rpb.pos + n).min(res.rows.len());
            let rows = res.rows[rpb.pos..end].to_vec();
            rpb.pos = end;
            (rows, end >= res.rows.len())
        } else {
            let d = Desc {
                name: rpb.cursor.clone(),
                rows: true,
                ..self.desc(Op::Fetch)
            };
            let rep = self.run(d, vec![Value::int(i64::try_from(n).unwrap_or(1))]);
            let end = rep.rows.len() < n || rep.sqlca.sqlcode == 100;
            (rep.rows, end)
        };
        self.rpbs.insert(rpb_id, rpb);
        out
    }

    fn fetch(&mut self, r: &Req, params: &[Param]) -> Answer {
        let rpb = self.rpb(r);
        let cols = rpb.results.as_ref().map_or_else(|| rpb.cols.clone(), |x| x.cols.clone());
        let n = block_rows(params, &cols);
        let (rows, _) = self.fetch_rows(r.rpb, n);
        if rows.is_empty() {
            let ca = Sqlca::no_data();
            let mut p = Self::results(r, &ca, true);
            p.push(Param::new(db_reply::EXTENDED_RESULT_DATA, Vec::new()));
            self.last = Last { function: r.function, sqlca: ca };
            return Answer { of: r.function, class: 1, code: 100, params: p };
        }
        let ca = Sqlca::rows(rows.len() as u64);
        let mut p = Self::results(r, &ca, true);
        p.push(Param::new(db_reply::EXTENDED_RESULT_DATA, data::encode_rows(&cols, &rows, 1)));
        self.last = Last { function: r.function, sqlca: ca };
        Answer { of: r.function, class: 0, code: 0, params: p }
    }

    fn close(&mut self, r: &Req) -> Answer {
        let mut rpb = self.rpb(r);
        let ca = if rpb.results.is_some() {
            rpb.results = None;
            Sqlca::default()
        } else if rpb.open {
            let d = Desc {
                name: rpb.cursor.clone(),
                ..self.desc(Op::Close)
            };
            self.run(d, Vec::new()).sqlca
        } else {
            Sqlca::default()
        };
        rpb.open = false;
        self.rpbs.insert(r.rpb, rpb);
        self.finish(r, ca, Vec::new())
    }

    fn transaction(&mut self, r: &Req, commit: bool) -> Answer {
        let mut ca = if self.commit == 0 || self.autocommit {
            Sqlca::default()
        } else {
            let d = Desc {
                text: if commit { "COMMIT" } else { "ROLLBACK" }.into(),
                ..self.desc(Op::Exec)
            };
            self.run(d, Vec::new()).sqlca
        };
        if ca.sqlcode == -7007 {
            ca = Sqlca::default();
        }
        self.finish(r, ca, Vec::new())
    }

    fn send_results(&mut self, r: &Req) -> Answer {
        let ca = self.last.sqlca.clone();
        let (class, code) = Self::class_of(&ca);
        Answer {
            of: self.last.function,
            class,
            code,
            params: Self::results(r, &ca, true),
        }
    }

    fn request(&mut self, f: &Frame) -> Outcome {
        let r = Req::of(f);
        let params = expanded(&f.params).unwrap_or_else(|_| f.params.clone());
        let ans = match r.function {
            id::SET_ATTRIBUTES | id::RETRIEVE_ATTRIBUTES => Some(self.set_attributes(f, &params)),
            id::CREATE_RPB | id::CHANGE_RPB | id::CREATE_RPB_BASED_ON => {
                self.create_rpb(&r, &params);
                None
            }
            id::RESET_RPB => {
                if let Some(x) = self.rpbs.get_mut(&r.rpb) {
                    x.text.clear();
                }
                None
            }
            id::DELETE_RPB => {
                if let Some(x) = self.rpbs.remove(&r.rpb)
                    && x.open
                    && x.results.is_none()
                {
                    let d = Desc {
                        name: x.cursor,
                        ..self.desc(Op::Close)
                    };
                    let _ = self.run(d, Vec::new());
                }
                Some(Answer { of: r.function, class: 0, code: 0, params: Vec::new() })
            }
            id::CHANGE_DESCRIPTOR => {
                if let Some(p) = params.iter().find(|p| p.cp == 0x381E || p.cp == 0x3801) {
                    self.descriptors.insert(r.descriptor, format::decode_client(&p.data));
                }
                None
            }
            id::DELETE_DESCRIPTOR => {
                self.descriptors.remove(&r.descriptor);
                None
            }
            id::PREPARE => Some(self.prepare(&r, &params, false)),
            id::PREPARE_DESCRIBE | id::DESCRIBE | id::DESCRIBE_MARKERS => {
                if r.function == id::PREPARE_DESCRIBE {
                    Some(self.prepare(&r, &params, true))
                } else {
                    let rpb = self.rpb(&r);
                    let mut p = Vec::new();
                    if r.wants(ors::DATA_FORMAT) || r.function == id::DESCRIBE {
                        p.push(Param::new(db_reply::SUPER_DATA_FORMAT, format::encode(&rpb.cols, true)));
                    }
                    if r.wants(ors::MARKER_FORMAT) || r.function == id::DESCRIBE_MARKERS {
                        p.push(Param::new(db_reply::SUPER_MARKER_FORMAT, format::encode(&rpb.markers, false)));
                    }
                    Some(Answer { of: r.function, class: 0, code: 0, params: p })
                }
            }
            id::EXECUTE => Some(self.execute(&r, &params)),
            id::PREPARE_EXECUTE => {
                let a = self.prepare(&r, &params, false);
                if a.class == 0 { Some(self.execute(&r, &params)) } else { Some(a) }
            }
            id::EXECUTE_IMMEDIATE => Some(self.exec_immediate(&r, &params)),
            id::OPEN_DESCRIBE => Some(self.open(&r, &params, false)),
            id::OPEN_DESCRIBE_FETCH => Some(self.open(&r, &params, true)),
            id::EXECUTE_OPEN_DESCRIBE => {
                let rpb = self.rpb(&r);
                if rpb.select || rpb.call.is_some() {
                    Some(self.open(&r, &params, false))
                } else {
                    Some(self.execute(&r, &params))
                }
            }
            id::FETCH => Some(self.fetch(&r, &params)),
            id::CLOSE => Some(self.close(&r)),
            id::COMMIT => Some(self.transaction(&r, true)),
            id::ROLLBACK => Some(self.transaction(&r, false)),
            id::SEND_RESULTS => Some(self.send_results(&r)),
            id::DELETE_RESULTS | id::TEST_CONNECTION => None,
            id::RETRIEVE_LOB | id::WRITE_LOB | id::FREE_LOB => {
                Some(self.finish(&r, code(-84, "42612"), Vec::new()))
            }
            id::CREATE_PACKAGE | id::CLEAR_PACKAGE | id::DELETE_PACKAGE | id::RETURN_PACKAGE => {
                Some(self.finish(&r, Sqlca::default(), Vec::new()))
            }
            id::END => return Outcome::close(),
            _ => Some(self.finish(&r, code(-84, "42612"), Vec::new())),
        };
        match ans {
            Some(a) if r.wants(ors::RETURN_DATA) => Outcome::one(Self::frame(&f.header, &r, a)),
            _ => Outcome::none(),
        }
    }
}

/// An SQLCA with a code and state.
fn code(sqlcode: i32, state: &str) -> Sqlca {
    Sqlca {
        sqlcode,
        sqlstate: state.into(),
        ..Sqlca::default()
    }
}

/// What an exit program's refusal answers.
fn denied() -> Sqlca {
    Sqlca {
        sqlcode: -551,
        sqlstate: "42501".into(),
        sqlerrmc: "An exit program did not allow the statement.".into(),
        ..Sqlca::default()
    }
}

/// The cursor attributes parameter: not scrollable, read only.
fn cursor_attributes() -> Param {
    let mut d = text::ebcdic("10000");
    d.extend([0; 5]);
    Param::new(db_reply::CURSOR_ATTRIBUTES, d)
}

/// Columns from `DESCRIBE` rows (type, length, name).
fn cols_of(rows: &[Vec<Value>]) -> Vec<ColType> {
    rows.iter()
        .enumerate()
        .map(|(i, r)| {
            let t = r.first().and_then(|v| v.as_i64().ok()).unwrap_or(449);
            let l = r.get(1).and_then(|v| v.as_i64().ok()).unwrap_or(0);
            let name = r.get(2).map(Value::as_text).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("{:05}", i + 1));
            let mut c = ColType::from_describe(t, l, &name);
            // A named column (not an expression's number) came from a table.
            c.column = !name.chars().all(|ch| ch.is_ascii_digit());
            c
        })
        .collect()
}

/// The rows a fetch asks for: the blocking factor, or the buffer size
/// over the row size.
fn block_rows(params: &[Param], cols: &[ColType]) -> usize {
    if let Some(n) = params.iter().find(|p| p.cp == db_request::BLOCKING_FACTOR).and_then(Param::as_u32) {
        return (n as usize).max(1);
    }
    let size: usize = cols.iter().map(|c| c.len as usize).sum::<usize>().max(1);
    params
        .iter()
        .find(|p| p.cp == db_request::BUFFER_SIZE)
        .and_then(Param::as_u32)
        .map_or(32, |b| (b as usize / size).max(1))
}

impl<A: Auth, E: Embedded, X: Exits> Handler for Database<A, E, X> {
    fn handle(&mut self, frame: &[u8]) -> Outcome {
        let Ok(f) = Frame::decode(frame) else {
            return Outcome::close();
        };
        if let Some(o) = self.session.common(&f, &mut self.auth) {
            return o;
        }
        if !self.session.started() {
            return Outcome::close();
        }
        self.request(&f)
    }

    /// Work not committed is rolled back; the connection's cursors and
    /// statements go.
    fn finish(&mut self) {
        if self.commit != 0 && !self.autocommit {
            let d = Desc {
                text: "ROLLBACK".into(),
                ..self.desc(Op::Exec)
            };
            let _ = self.run(d, Vec::new());
        }
        let d = self.desc(Op::End);
        let _ = self.run(d, Vec::new());
    }
}
