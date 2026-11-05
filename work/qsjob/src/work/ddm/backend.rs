// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DDM server's backends over the storage service: the password
//! check as a sign-on that makes the job run as the user, SQL through
//! the job's embedded SQL session, record-level access through the
//! job's open data paths, and `S38CMD` through the command analyzer.

// The SQLCA is the error DRDA returns; it is not boxed.
#![allow(clippy::result_large_err)]

use std::collections::HashMap;

use ddm::sec::Proof;
use ddm::server::{
    Authenticator, FileBackend, FileError, FileInfo, FileOpen, FileRecord, ReadAt, Section, SqlBackend,
    SqlResult,
};
use ddm::sql::{ColType, Column, Kind, Sqlca, Value};
use hostsvr::common::rc;
use mi::io::{IoOp, IoRequest, IoResult, IoSource, KeyRef, OpenSpec};
use sql::embed::{Desc, Embedded, Op, Reply, Request};
use stg::remote::RemoteIo;
use stg::{Client, Shared};

use crate::console::sqlcall::CallingSql;
use crate::work::hostsvr::job::Job;

/// The scope the server's statements run in.
const SCOPE: &str = "QRWTSRVR:";

/// Signing on over the job's link; the job then runs as the user.
pub struct JobAuth(pub Shared);

/// A `SECCHKCD` for a refused sign-on.
fn secchkcd(code: u32) -> u8 {
    match code {
        rc::USER_UNKNOWN => 0x13,
        rc::USER_DISABLED => 0x14,
        rc::PASSWORD_EXPIRED => 0x0E,
        _ => 0x0F,
    }
}

impl Authenticator for JobAuth {
    fn check(&mut self, user: &str, proof: &Proof) -> Result<(), u8> {
        let r = match proof {
            Proof::Password(p) => self.0.borrow_mut().with(|l| l.ddm_signon(user.into(), p.clone(), true)),
            Proof::Substitute {
                substitute,
                client_seed,
                server_seed,
            } => {
                let sub = sec::signon::Substitute {
                    server_seed: *server_seed,
                    client_seed: *client_seed,
                    substitute: substitute.clone(),
                };
                self.0.borrow_mut().with(|l| l.host_signon(user.into(), sub, Vec::new(), true))
            }
        };
        r.map(|_| ()).map_err(|e| secchkcd(e.rc))
    }
}

/// SQL through the job's embedded SQL session in the service; a
/// `CALL` is made in the job.
pub struct JobSql {
    sql: CallingSql,
    /// The columns each open cursor returns.
    cursors: HashMap<String, Vec<Column>>,
    /// The last `CALL`'s procedure and its result sets: each one's
    /// cursor here (`RS#n`) and in the procedure.
    called: (String, Vec<(String, String)>),
    /// The result sets opened as a section's query: the section's
    /// cursor name and the set's.
    sets: HashMap<String, String>,
    /// The last `CALL`'s `OUT` and `INOUT` values by marker.
    outs: Vec<(usize, sql::Value)>,
}

impl JobSql {
    /// Over a link, calls made in `job`.
    pub fn new(link: Shared, job: Job) -> Self {
        Self {
            sql: CallingSql::new(link, job),
            cursors: HashMap::new(),
            called: (String::new(), Vec::new()),
            sets: HashMap::new(),
            outs: Vec::new(),
        }
    }

    fn run(&mut self, desc: Desc, inputs: Vec<sql::Value>) -> Reply {
        self.sql.run(&Request {
            scope: SCOPE.into(),
            desc,
            inputs,
        })
    }

    fn checked(&mut self, desc: Desc, inputs: Vec<sql::Value>) -> SqlResult<Reply> {
        let r = self.run(desc, inputs);
        if r.sqlca.sqlcode < 0 { Err(ca_of(&r.sqlca)) } else { Ok(r) }
    }
}

/// The statement name of a section.
fn stmt(s: &Section) -> String {
    format!("S{}_{}_{}", s.collection.trim(), s.package.trim(), s.number)
}

/// The cursor of a section.
fn cursor(s: &Section) -> String {
    format!("C{}_{}_{}", s.collection.trim(), s.package.trim(), s.number)
}

fn desc(op: Op, name: String) -> Desc {
    Desc {
        op,
        name,
        ..Desc::default()
    }
}

/// The wire's SQLCA from the engine's.
fn ca_of(c: &sql::Sqlca) -> Sqlca {
    let mut o = Sqlca::error(c.sqlcode, &c.sqlstate, &c.sqlerrmc);
    o.sqlerrd = c.sqlerrd;
    o
}

/// A column from a `DESCRIBE` row (SQLTYPE, SQLLEN, name, ...).
fn column(i: usize, row: &[sql::Value]) -> Column {
    let int = |v: Option<&sql::Value>| v.and_then(|v| v.as_i64().ok()).unwrap_or(0);
    let t = int(row.first());
    let len = int(row.get(1));
    let name = row
        .get(2)
        .map(sql::Value::as_text)
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("{:05}", i + 1));
    let nullable = t % 2 == 1;
    let small = |v: i64| u8::try_from(v).unwrap_or(0);
    let chars = u32::try_from(len.max(1)).unwrap_or(1);
    let ty = match t & !1 {
        500 => ColType::new(Kind::SmallInt, 0, 5, 0, nullable),
        496 => ColType::new(Kind::Integer, 0, 10, 0, nullable),
        492 => ColType::new(Kind::BigInt, 0, 19, 0, nullable),
        484 => ColType::new(Kind::Decimal, 0, small(len >> 8), small(len & 0xFF), nullable),
        488 => ColType::new(Kind::Numeric, 0, small(len >> 8), small(len & 0xFF), nullable),
        480 => ColType::new(Kind::Double, 0, 53, 0, nullable),
        452 => ColType::new(Kind::Char, chars, 0, 0, nullable),
        384 => ColType::new(Kind::Date, 0, 0, 0, nullable),
        388 => ColType::new(Kind::Time, 0, 0, 0, nullable),
        392 => ColType::new(Kind::Timestamp, 0, 0, 0, nullable),
        _ => ColType::new(Kind::VarChar, chars, 0, 0, nullable),
    };
    Column {
        name: name.trim().to_string(),
        ty,
        label: None,
    }
}

fn columns(rows: &[Vec<sql::Value>]) -> Vec<Column> {
    rows.iter().enumerate().map(|(i, r)| column(i, r)).collect()
}

/// A value for the wire, as the column describes it.
fn to_wire(v: &sql::Value, t: Option<&ColType>) -> Value {
    match v {
        sql::Value::Null => Value::Null,
        sql::Value::Num(d) => match t.map(|t| t.kind) {
            Some(Kind::SmallInt | Kind::Integer | Kind::BigInt) => {
                d.to_string().parse().map_or_else(|_| Value::Dec(d.clone()), Value::Int)
            }
            _ => Value::Dec(d.clone()),
        },
        sql::Value::Double(f) => Value::Double(*f),
        sql::Value::Real(f) => Value::Double(f64::from(*f)),
        other => Value::Text(other.as_text()),
    }
}

fn from_wire(v: &Value) -> sql::Value {
    match v {
        Value::Null => sql::Value::Null,
        Value::Int(i) => sql::Value::int(*i),
        Value::Dec(d) => sql::Value::Num(d.clone()),
        Value::Double(f) => sql::Value::Double(*f),
        Value::Text(t) => sql::Value::Str(t.clone()),
    }
}

impl SqlBackend for JobSql {
    fn prepare(&mut self, s: &Section, text: &str) -> SqlResult<Vec<Column>> {
        let d = Desc {
            into: true,
            ..desc(Op::Prepare, stmt(s))
        };
        let r = self.checked(d, vec![sql::Value::Str(text.to_string())])?;
        Ok(columns(&r.rows))
    }

    fn describe(&mut self, s: &Section) -> SqlResult<Vec<Column>> {
        let r = self.checked(desc(Op::Describe, stmt(s)), Vec::new())?;
        Ok(columns(&r.rows))
    }

    fn execute(&mut self, s: &Section, params: &[Value]) -> SqlResult<Sqlca> {
        let r = self.checked(desc(Op::Execute, stmt(s)), params.iter().map(from_wire).collect())?;
        self.outs = self.sql.take_outs();
        self.called = (String::new(), Vec::new());
        if r.sqlca.sqlcode == 466 {
            let text = |row: &[sql::Value], i: usize| row.get(i).map(sql::Value::as_text).unwrap_or_default();
            let routine = r.rows.first().map(|row| text(row, 3)).unwrap_or_default();
            self.called = (routine, r.rows.iter().map(|row| (text(row, 0), text(row, 1))).collect());
        }
        Ok(ca_of(&r.sqlca))
    }

    fn call_outputs(&mut self, s: &Section) -> SqlResult<Vec<(ColType, Value)>> {
        let outs = std::mem::take(&mut self.outs);
        let d = Desc {
            orient: "INPUT".into(),
            ..desc(Op::Describe, stmt(s))
        };
        let markers = self.checked(d, Vec::new())?.rows;
        Ok(markers
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let ty = column(i, row).ty;
                // Mode 1 is IN: the reply has no value for it.
                let input = row.get(9).and_then(|v| v.as_i64().ok()).unwrap_or(1) == 1;
                let v = if input { None } else { outs.iter().find(|(m, _)| *m == i).map(|(_, v)| to_wire(v, Some(&ty))) };
                (ty, v.unwrap_or(Value::Null))
            })
            .collect())
    }

    fn bind(&mut self, collection: &str, package: &str, token: [u8; 8], stmts: &[(u16, String)]) -> SqlResult<()> {
        let mut inputs = vec![
            sql::Value::Str(collection.trim().to_string()),
            sql::Value::Str(package.trim().to_string()),
            sql::Value::Str(u64::from_be_bytes(token).to_string()),
        ];
        for (n, text) in stmts {
            inputs.push(sql::Value::int(i64::from(*n)));
            inputs.push(sql::Value::Str(text.clone()));
        }
        self.checked(desc(Op::Package, "BIND".into()), inputs).map(|_| ())
    }

    fn result_sets(&mut self) -> (String, Vec<String>) {
        (self.called.0.clone(), self.called.1.iter().map(|(_, c)| c.clone()).collect())
    }

    fn open_result_set(&mut self, s: &Section, index: usize) -> SqlResult<Vec<Column>> {
        let Some((set, _)) = self.called.1.get(index).cloned() else {
            return Err(Sqlca::error(-501, "24501", "no such result set"));
        };
        let d = Desc {
            orient: "CURSOR".into(),
            ..desc(Op::Describe, set.clone())
        };
        let cols = columns(&self.checked(d, Vec::new())?.rows);
        self.sets.insert(cursor(s), set);
        self.cursors.insert(cursor(s), cols.clone());
        Ok(cols)
    }

    fn execute_immediate(&mut self, text: &str) -> SqlResult<Sqlca> {
        let r = self.checked(desc(Op::ExecImm, String::new()), vec![sql::Value::Str(text.to_string())])?;
        Ok(ca_of(&r.sqlca))
    }

    fn open(&mut self, s: &Section, params: &[Value]) -> SqlResult<Vec<Column>> {
        let cols = self.describe(s)?;
        let d = Desc {
            stmt: stmt(s),
            ..desc(Op::Open, cursor(s))
        };
        self.checked(d, params.iter().map(from_wire).collect())?;
        self.cursors.insert(cursor(s), cols.clone());
        Ok(cols)
    }

    fn fetch(&mut self, s: &Section, max: usize) -> SqlResult<Vec<Vec<Value>>> {
        let name = self.sets.get(&cursor(s)).cloned().unwrap_or_else(|| cursor(s));
        let d = Desc {
            rows: true,
            ..desc(Op::Fetch, name)
        };
        let r = self.checked(d, vec![sql::Value::int(i64::try_from(max.max(1)).unwrap_or(1))])?;
        let cols = self.cursors.get(&cursor(s)).cloned().unwrap_or_default();
        Ok(r
            .rows
            .iter()
            .map(|row| row.iter().enumerate().map(|(i, v)| to_wire(v, cols.get(i).map(|c| &c.ty))).collect())
            .collect())
    }

    fn close(&mut self, s: &Section) -> SqlResult<()> {
        self.cursors.remove(&cursor(s));
        let name = self.sets.remove(&cursor(s)).unwrap_or_else(|| cursor(s));
        self.checked(desc(Op::Close, name), Vec::new()).map(|_| ())
    }

    fn commit(&mut self) -> SqlResult<()> {
        self.transaction("COMMIT")
    }

    fn rollback(&mut self) -> SqlResult<()> {
        self.transaction("ROLLBACK")
    }
}

impl JobSql {
    fn transaction(&mut self, what: &str) -> SqlResult<()> {
        let d = Desc {
            text: what.into(),
            ..desc(Op::Exec, String::new())
        };
        let r = self.run(d, Vec::new());
        // Not under commitment control: nothing to commit.
        if r.sqlca.sqlcode < 0 && r.sqlca.sqlcode != -7007 {
            return Err(ca_of(&r.sqlca));
        }
        Ok(())
    }
}

/// An open file: its handle in the service and whether it has been
/// read since it was opened.
struct Open {
    handle: u32,
    fields: usize,
}

/// Record-level access through the job's open data paths.
pub struct JobFiles {
    link: Shared,
    io: RemoteIo,
    job: Job,
    open: HashMap<u32, Open>,
}

impl JobFiles {
    /// Over a link, running commands in `job`.
    pub fn new(link: Shared, job: Job) -> Self {
        Self {
            io: RemoteIo::new(link.clone()),
            link,
            job,
            open: HashMap::new(),
        }
    }

    fn handle(&self, h: u32) -> Result<u32, FileError> {
        self.open.get(&h).map(|o| o.handle).ok_or_else(|| FileError {
            id: "CPF4520".into(),
            text: "No file is open under that handle.".into(),
        })
    }

    /// One operation, its trigger programs called in the job's runtime.
    fn op(&mut self, h: u32, req: &IoRequest<'_>) -> Result<IoResult, FileError> {
        let handle = self.handle(h)?;
        let io = &mut self.io;
        let mut console = self.job.borrow_mut();
        console.job();
        console.swapped();
        Ok(mi::io::trigger::run(console.job(), req, &mut |_, r| io.io(handle, r)))
    }
}

fn request(op: IoOp) -> IoRequest<'static> {
    IoRequest {
        op,
        record: None,
        key: KeyRef::None,
        format: None,
        nolock: true,
        nulls: None,
        resume: None,
    }
}

/// The failure an operation reported, if it did: end of data and a
/// missing record are not failures.
fn failed(r: &IoResult) -> Option<FileError> {
    if r.status < 1000 || r.eof || matches!(r.status, 1211 | 1218 | 1221) && r.msgid.is_none() {
        return None;
    }
    let id = r.msgid.clone().unwrap_or_else(|| "CPF5029".into());
    if matches!(id.as_str(), "CPF5001" | "CPF5006" | "CPF5025") {
        return None;
    }
    Some(FileError {
        text: format!("The operation ended with status {:05}.", r.status),
        id,
    })
}

fn record_of(r: IoResult) -> Option<FileRecord> {
    let data = r.record?;
    let nulls = r.nulls.unwrap_or_default().iter().map(|b| *b == 0xF1).collect();
    Some(FileRecord {
        rrn: u32::try_from(r.rrn).unwrap_or(0),
        data,
        nulls,
    })
}

fn null_map(nulls: &[bool], fields: usize) -> Vec<u8> {
    let mut m: Vec<u8> = nulls.iter().map(|n| if *n { 0xF1 } else { 0xF0 }).collect();
    m.resize(fields, 0xF0);
    m
}

impl FileBackend for JobFiles {
    fn open(&mut self, req: &FileOpen) -> Result<(u32, FileInfo), FileError> {
        let usage = match (req.output, req.update || req.delete, req.input) {
            (true, false, false) => qir::FileUsage::Output,
            (false, true, _) => qir::FileUsage::Update,
            (false, false, _) => qir::FileUsage::Input,
            (true, _, _) => qir::FileUsage::All,
        };
        let lib = if req.library.trim().is_empty() { "*LIBL".to_string() } else { req.library.trim().to_string() };
        let member = req.member.trim();
        let spec = OpenSpec {
            // Trigger programs are called in the job (`op`).
            triggers: true,
            file: req.file.trim().to_string(),
            lib,
            usage,
            keyed: req.keyed,
            lvlchk: None,
            member: (!member.is_empty() && !member.eq_ignore_ascii_case("*FIRST")).then(|| member.to_string()),
            actgrp: String::new(),
            commit: false,
        };
        let (h, info) = self.io.open(&spec).map_err(|f| FileError {
            text: format!("{} in library {} could not be opened.", spec.file, spec.lib),
            id: f.msgid,
        })?;
        let formats = self.link.borrow_mut().with(|l| l.formats(info.lib.clone(), info.file.clone()));
        let fields = formats.first().map_or(0, |f| f.fields.len());
        let file = self.link.borrow_mut().with(|l| l.file(info.lib.clone(), info.file.clone()));
        let members = self.link.borrow_mut().with(|l| l.members(info.lib.clone(), info.file.clone()));
        let records = members.iter().find(|m| m.name == info.member).or(members.first()).map_or(0, |m| m.records);
        self.open.insert(h, Open { handle: h, fields });
        Ok((
            h,
            FileInfo {
                file: info.file,
                library: info.lib,
                member: info.member,
                format: info.format,
                reclen: u16::try_from(info.rec_len).unwrap_or(u16::MAX),
                fields: u16::try_from(fields).unwrap_or(0),
                records,
                keyed: file.is_some_and(|f| f.keyed),
                ccsid: 37,
            },
        ))
    }

    fn read(&mut self, h: u32, at: &ReadAt, lock: bool) -> Result<Option<FileRecord>, FileError> {
        let nolock = !lock;
        let r = match at {
            ReadAt::First => {
                let start = IoRequest {
                    key: KeyRef::Start,
                    ..request(IoOp::SetLL)
                };
                let s = self.op(h, &start)?;
                if let Some(e) = failed(&s) {
                    return Err(e);
                }
                self.op(h, &IoRequest { nolock, ..request(IoOp::Read) })?
            }
            ReadAt::Next => self.op(h, &IoRequest { nolock, ..request(IoOp::Read) })?,
            ReadAt::Key { key, .. } => self.op(
                h,
                &IoRequest {
                    key: KeyRef::Fields(key),
                    nolock,
                    ..request(IoOp::Chain)
                },
            )?,
            ReadAt::Rrn(n) => self.op(
                h,
                &IoRequest {
                    key: KeyRef::Rrn(u64::from(*n)),
                    nolock,
                    ..request(IoOp::Chain)
                },
            )?,
        };
        if let Some(e) = failed(&r) {
            return Err(e);
        }
        if r.eof || (matches!(at, ReadAt::Key { .. } | ReadAt::Rrn(_)) && !r.found) {
            return Ok(None);
        }
        Ok(record_of(r))
    }

    fn write(&mut self, h: u32, data: &[u8], nulls: &[bool]) -> Result<u32, FileError> {
        let fields = self.open.get(&h).map_or(0, |o| o.fields);
        let nm = null_map(nulls, fields);
        let r = self.op(
            h,
            &IoRequest {
                record: Some(data),
                nulls: Some(&nm),
                ..request(IoOp::Write)
            },
        )?;
        failed(&r).map_or(Ok(u32::try_from(r.rrn).unwrap_or(0)), Err)
    }

    fn update(&mut self, h: u32, data: &[u8], nulls: &[bool]) -> Result<(), FileError> {
        let fields = self.open.get(&h).map_or(0, |o| o.fields);
        let nm = null_map(nulls, fields);
        let r = self.op(
            h,
            &IoRequest {
                record: Some(data),
                nulls: Some(&nm),
                ..request(IoOp::Update)
            },
        )?;
        failed(&r).map_or(Ok(()), Err)
    }

    fn delete(&mut self, h: u32) -> Result<(), FileError> {
        let r = self.op(h, &request(IoOp::Delete))?;
        failed(&r).map_or(Ok(()), Err)
    }

    fn feod(&mut self, h: u32) -> Result<(), FileError> {
        let r = self.op(h, &request(IoOp::Feod))?;
        failed(&r).map_or(Ok(()), Err)
    }

    fn close(&mut self, h: u32) -> Result<(), FileError> {
        let handle = self.handle(h)?;
        self.open.remove(&h);
        let r = self.io.close(handle);
        failed(&r).map_or(Ok(()), Err)
    }

    fn command(&mut self, cmd: &str) -> Result<Vec<FileError>, FileError> {
        let (ok, msgs) = crate::work::hostsvr::job::run_command(&self.job, cmd);
        let list: Vec<FileError> = msgs
            .into_iter()
            .map(|m| FileError {
                id: m.id,
                text: m.text,
            })
            .collect();
        if ok {
            Ok(list)
        } else {
            Err(list.last().cloned().unwrap_or(FileError {
                id: "CPF9898".into(),
                text: format!("{cmd} ended in error."),
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_rows_become_columns() {
        let row = |t: i64, l: i64, n: &str| vec![sql::Value::int(t), sql::Value::int(l), sql::Value::Str(n.into())];
        let c = columns(&[row(497, 4, "ID"), row(484, 7 * 256 + 2, "AMT"), row(452, 10, "NAME")]);
        assert_eq!(c[0].ty.kind, Kind::Integer);
        assert!(c[0].ty.nullable);
        assert_eq!((c[1].ty.kind, c[1].ty.precision, c[1].ty.scale), (Kind::Decimal, 7, 2));
        assert_eq!((c[2].ty.kind, c[2].ty.length, c[2].name.as_str()), (Kind::Char, 10, "NAME"));
    }

    #[test]
    fn end_of_data_is_not_a_failure() {
        let eof = IoResult {
            status: 11,
            eof: true,
            ..IoResult::default()
        };
        assert!(failed(&eof).is_none());
        let bad = IoResult {
            status: 1299,
            msgid: Some("CPF5029".into()),
            ..IoResult::default()
        };
        assert_eq!(failed(&bad).unwrap().id, "CPF5029");
    }
}
