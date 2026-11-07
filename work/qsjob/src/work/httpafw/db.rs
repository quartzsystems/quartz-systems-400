// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `accessweb::env::db::Db` as the signed-on user: every statement runs
//! through QAP's `Sql` lane (`qapsql::Lane`) over the job's embedded
//! SQL, with the job swapped to the user's profile, so the engine checks
//! that user's authority. Each connection a page opens is a lane of its
//! own whose embedded scopes, and so its commitment definition
//! (`*SQLAFWn`), are its own; it lives until the page closes it or the
//! session ends. The catalog is queries over the `QSYS2` views. Saved
//! requests and the history are kept with the preferences, in user
//! index `QUSRSYS/QZXWPRF`, through the service's `web_store`: keys
//! `_q<id>.<part>` and `_h.<part>` of the profile, and the shared ones
//! listed under `*SHARED`.

#![allow(clippy::many_single_char_names)] // rows, lanes, engines

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use accessweb::env::db::{
    Column, Commit, Db, Entry, Kind, Options, Outcome, ReqType, Request, ResultColumn, Val,
};
use qap_proto::ServerMessage;
use qap_proto::ops::sql::{CommitLevel, Execute, Fetch, Naming, SetOptions, SqlValue};
use qapd::sql::SqlStop;
use qsweb::msg::{Kind as MsgKind, Message};
use sql::embed::{Reply, Request as SqlRequest};
use stg::Client;

use super::Link;
use crate::qapsql::{Engine, Lane};
use crate::work::qap::sql::{LaneDb, running};
use crate::work::web::bridge::Main;

/// The connections a session may have.
const MAX: usize = 16;
/// The store of requests and history.
const LIB: &str = "QUSRSYS";
/// Its name.
const INDEX: &str = "QZXWPRF";
/// A value's room in an entry: 128 less the 30-byte key.
const ROOM: usize = 98;

/// Cursors open: the statement each came from, its columns' scales.
type Cursors = HashMap<u32, (u32, Vec<u16>)>;

/// One connection's lane and cursors, taken out of the table while a
/// statement runs in it.
#[derive(Default)]
struct Conn {
    lane: Lane,
    cursors: Cursors,
}

/// Whose a connection is, and its cancel.
struct Owner {
    user: String,
    cancel: Arc<AtomicBool>,
}

/// The database as the signed-on user.
pub struct LiveDb {
    /// The job's thread.
    pub link: Link,
    /// The job's number (the engine's cancel file is the job's).
    pub number: u32,
    conns: Arc<Mutex<HashMap<u32, Conn>>>,
    owners: Mutex<HashMap<u32, Owner>>,
    next: AtomicU32,
    /// The connection whose statement runs now.
    busy: Arc<AtomicU32>,
    running: Arc<AtomicU8>,
}

impl LiveDb {
    /// Over the job's thread.
    pub fn new(link: Link, number: u32) -> Self {
        Self {
            link,
            number,
            conns: Arc::new(Mutex::new(HashMap::new())),
            owners: Mutex::new(HashMap::new()),
            next: AtomicU32::new(1),
            busy: Arc::new(AtomicU32::new(0)),
            running: Arc::new(AtomicU8::new(0)),
        }
    }
}

/// The lane's engine with its scopes made the connection's own.
struct Scoped<'a> {
    inner: LaneDb<'a>,
    prefix: String,
}

impl Engine for Scoped<'_> {
    fn run(&mut self, req: &SqlRequest) -> Reply {
        let mut r = req.clone();
        if let Some(rest) = r.scope.strip_prefix("QAP:") {
            r.scope = format!("{}:{rest}", self.prefix);
        } else if r.scope == "QAP:" {
            r.scope = format!("{}:", self.prefix);
        }
        self.inner.run(&r)
    }

    fn message(&mut self, id: &str, data: &[&str]) -> ServerMessage {
        self.inner.message(id, data)
    }
}

fn gone() -> Message {
    Message::new(
        MsgKind::Error,
        "CPF9897",
        "Access for Web is ending; the statement was not run.",
    )
}

fn message(m: &ServerMessage) -> Message {
    Message::new(
        if m.severity >= 30 {
            MsgKind::Error
        } else {
            MsgKind::Info
        },
        &m.id,
        &m.text,
    )
}

/// A failed outcome from the lane's stop.
fn stopped<T>(s: SqlStop<T>) -> Outcome {
    match s {
        SqlStop::Failed(f) => Outcome {
            message: Some(message(&f.message)),
            sqlstate: f.failure.sqlstate.clone(),
            sqlcode: f.failure.sqlcode,
            row: f.failure.row.map(|r| r as usize),
            failed: true,
            ..Outcome::default()
        },
        SqlStop::Refused(r) => Outcome {
            message: Some(message(&r.message)),
            sqlstate: if r.message.id == "SQL0551" {
                "42501".into()
            } else {
                "58004".into()
            },
            sqlcode: if r.message.id == "SQL0551" {
                -551
            } else {
                -901
            },
            failed: true,
            ..Outcome::default()
        },
        SqlStop::Cancelled(_, m) => Outcome {
            message: Some(message(&m)),
            sqlstate: "57014".into(),
            sqlcode: -952,
            failed: true,
            ..Outcome::default()
        },
    }
}

/// Days since 1970 as `YYYY-MM-DD`.
fn date(ms: i64) -> String {
    let (y, m, d) = accessweb::util::civil(ms.div_euclid(86_400_000));
    format!("{y:04}-{m:02}-{d:02}")
}

fn clock(ms: i64, sep: char) -> String {
    let s = ms.rem_euclid(86_400_000) / 1000;
    format!("{:02}{sep}{:02}{sep}{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// A value as the pages take it.
fn val(v: &SqlValue, scale: u16) -> Val {
    match v {
        SqlValue::Null => Val::Null,
        SqlValue::Integer { value } => Val::Num(value.to_string()),
        SqlValue::Decimal { text } | SqlValue::Float { text } => Val::Num(text.clone()),
        SqlValue::Text { text } => Val::Text(text.clone()),
        SqlValue::Binary { data } => Val::Bin(data.clone()),
        SqlValue::Date { ms } => Val::Text(date(*ms)),
        SqlValue::Time { ms } => Val::Text(clock(*ms, '.')),
        SqlValue::Timestamp { ms, picos } => {
            let frac =
                u64::try_from(ms.rem_euclid(1000)).unwrap_or(0) * 1_000_000_000 + u64::from(*picos);
            let digits = format!("{frac:012}");
            let n = usize::from(scale.min(12));
            let f = if n == 0 {
                String::new()
            } else {
                format!(".{}", &digits[..n])
            };
            Val::Text(format!("{}-{}{f}", date(*ms), clock(*ms, '.')))
        }
        SqlValue::Boolean { value } => Val::Text(if *value {
            "TRUE".into()
        } else {
            "FALSE".into()
        }),
    }
}

/// A parameter for a marker of `typ`.
fn param(v: &Val, typ: &str) -> SqlValue {
    match v {
        Val::Null => SqlValue::Null,
        Val::Bin(b) => SqlValue::Binary { data: b.clone() },
        v if accessweb::env::db::is_numeric(typ) => SqlValue::Decimal {
            text: v.text().trim().to_string(),
        },
        v => SqlValue::Text { text: v.text() },
    }
}

fn commit_level(c: Commit) -> CommitLevel {
    match c {
        Commit::None => CommitLevel::None,
        Commit::ReadUncommitted => CommitLevel::ReadUncommitted,
        Commit::ReadCommitted => CommitLevel::ReadCommitted,
        Commit::RepeatableRead => CommitLevel::RepeatableRead,
        Commit::Serializable => CommitLevel::Serializable,
    }
}

/// One statement in a lane: prepared, run once per parameter row, a
/// query's first rows fetched; the statement released unless its cursor
/// stays open.
fn statement(
    e: &mut dyn Engine,
    lane: &mut Lane,
    cursors: &mut Cursors,
    sql: &str,
    params: &[Vec<Val>],
    max_rows: usize,
    cancel: &AtomicBool,
) -> Outcome {
    let t0 = Instant::now();
    let p = match lane.prepare(e, sql.trim().trim_end_matches(';')) {
        Ok(p) => p,
        Err(s) => return stopped(s),
    };
    let rows: Vec<Vec<SqlValue>> = params
        .iter()
        .map(|r| {
            r.iter()
                .zip(&p.parameters)
                .map(|(v, c)| param(v, &c.sql_type))
                .collect()
        })
        .collect();
    let cancelled = || cancel.load(Ordering::SeqCst);
    let ex = match lane.execute(
        e,
        &Execute {
            statement: p.statement,
            rows,
        },
        &cancelled,
    ) {
        Ok(x) => x,
        Err(s) => {
            let _ = lane.release(e, p.statement);
            return stopped(s);
        }
    };
    let mut o = Outcome {
        columns: p
            .columns
            .iter()
            .map(|c| ResultColumn {
                name: c.name.clone(),
                typ: c.sql_type.clone(),
            })
            .collect(),
        changed: ex.updated,
        sqlstate: "00000".into(),
        ..Outcome::default()
    };
    if let Some(cur) = ex.cursor {
        let max = u32::try_from(max_rows).unwrap_or(0);
        match lane.fetch(
            e,
            &Fetch {
                cursor: cur,
                max_rows: max,
            },
            &cancelled,
        ) {
            Ok(f) => {
                o.rows = f
                    .rows
                    .iter()
                    .map(|r| {
                        r.iter()
                            .zip(&p.columns)
                            .map(|(v, c)| val(v, c.scale))
                            .collect()
                    })
                    .collect();
                if f.end {
                    let _ = lane.close_cursor(e, cur);
                    let _ = lane.release(e, p.statement);
                } else {
                    o.cursor = Some(cur);
                    cursors.insert(
                        cur,
                        (p.statement, p.columns.iter().map(|c| c.scale).collect()),
                    );
                }
            }
            Err(s) => {
                let _ = lane.close_cursor(e, cur);
                let _ = lane.release(e, p.statement);
                return stopped(s);
            }
        }
    } else {
        let _ = lane.release(e, p.statement);
    }
    o.ms = u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX);
    o
}

/// A catalog query in a lane of its own, as the user: its rows; `?`
/// stands for the next of `params`, written as a literal.
fn catalog(m: &mut Main, sql: &str, params: &[Val]) -> Result<Vec<Vec<Val>>, Message> {
    let mut lane = Lane::default();
    let (link, console) = (&m.link, &mut m.console);
    let mut e = Scoped {
        inner: LaneDb(link, console),
        prefix: "AFWCAT".into(),
    };
    let mut cursors = HashMap::new();
    // The values as literals, as the catalog views take them anywhere.
    let mut text = String::new();
    let mut vals = params.iter();
    for c in sql.chars() {
        match (c, vals.as_slice().first()) {
            ('?', Some(_)) => {
                let v = vals.next().map(Val::text).unwrap_or_default();
                let _ = write!(text, "'{}'", v.replace('\'', "''"));
            }
            _ => text.push(c),
        }
    }
    let o = statement(
        &mut e,
        &mut lane,
        &mut cursors,
        &text,
        &[],
        100_000,
        &AtomicBool::new(false),
    );
    lane.end(&mut e);
    if o.failed {
        return Err(o
            .message
            .unwrap_or_else(|| Message::new(MsgKind::Error, "SQL0901", sql)));
    }
    Ok(o.rows)
}

fn text(r: &[Val], i: usize) -> String {
    r.get(i)
        .map(Val::text)
        .unwrap_or_default()
        .trim_end()
        .to_string()
}

fn num(r: &[Val], i: usize) -> u64 {
    text(r, i)
        .split('.')
        .next()
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0)
}

/// `2026-10-29-13.58.02.000000` (or with blanks and colons) as seconds
/// since 1970; the catalog's stamps are the system's local time.
fn stamp(t: &str, offset: i64) -> u64 {
    let p: Vec<i64> = t
        .split(|c: char| !c.is_ascii_digit())
        .filter(|x| !x.is_empty())
        .filter_map(|x| x.parse().ok())
        .collect();
    let [y, mo, d, h, mi, s, ..] = p[..] else {
        return 0;
    };
    let days = accessweb::util::days(
        y,
        u32::try_from(mo).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    );
    u64::try_from(days * 86_400 + h * 3600 + mi * 60 + s - offset).unwrap_or(0)
}

fn kind_of(t: &str) -> Kind {
    match t {
        "V" => Kind::View,
        "A" => Kind::Alias,
        "P" => Kind::Physical,
        "L" => Kind::Logical,
        _ => Kind::Table,
    }
}

/// The catalog's type names as SQL writes them.
fn type_name(t: &str) -> String {
    match t {
        "TIMESTMP" => "TIMESTAMP".into(),
        "INT" => "INTEGER".into(),
        t => t.to_string(),
    }
}

fn tables(m: &mut Main, schema: &str, offset: i64) -> Result<Vec<Entry>, Message> {
    let s = Val::Text(schema.to_string());
    let stats: HashMap<String, u64> = catalog(
        m,
        "SELECT TABLE_NAME, NUMBER_ROWS FROM QSYS2.SYSTABLESTAT WHERE TABLE_SCHEMA = ?",
        std::slice::from_ref(&s),
    )
    .unwrap_or_default()
    .iter()
    .map(|r| (text(r, 0), num(r, 1)))
    .collect();
    let rows = catalog(
        m,
        "SELECT TABLE_NAME, TABLE_TYPE, TABLE_TEXT, LAST_ALTERED_TIMESTAMP FROM QSYS2.SYSTABLES WHERE TABLE_SCHEMA = ? AND FILE_TYPE = 'D' ORDER BY TABLE_NAME",
        &[s],
    )?;
    Ok(rows
        .iter()
        .map(|r| {
            let name = text(r, 0);
            let kind = kind_of(&text(r, 1));
            Entry {
                rows: stats.get(&name).copied().unwrap_or(0),
                changed: if kind == Kind::View {
                    0
                } else {
                    stamp(&text(r, 3), offset)
                },
                name,
                kind,
                text: text(r, 2),
            }
        })
        .collect())
}

fn columns(m: &mut Main, schema: &str, table: &str) -> Result<Vec<Column>, Message> {
    let (s, t) = (Val::Text(schema.to_string()), Val::Text(table.to_string()));
    let rows = catalog(
        m,
        "SELECT COLUMN_NAME, DATA_TYPE, LENGTH, NUMERIC_SCALE, IS_NULLABLE, COLUMN_DEFAULT, HAS_DEFAULT, IS_IDENTITY FROM QSYS2.SYSCOLUMNS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
        &[s.clone(), t.clone()],
    )?;
    if rows.is_empty() {
        return Err(Message::new(
            MsgKind::Error,
            "SQL0204",
            &format!("{table} in {schema} not found."),
        ));
    }
    let keys: Vec<String> = catalog(
        m,
        "SELECT K.COLUMN_NAME FROM QSYS2.SYSCST C JOIN QSYS2.SYSKEYCST K ON K.CONSTRAINT_SCHEMA = C.CONSTRAINT_SCHEMA AND K.CONSTRAINT_NAME = C.CONSTRAINT_NAME WHERE C.TABLE_SCHEMA = ? AND C.TABLE_NAME = ? AND C.CONSTRAINT_TYPE = 'PRIMARY KEY' ORDER BY K.ORDINAL_POSITION",
        &[s, t],
    )
    .unwrap_or_default()
    .iter()
    .map(|r| text(r, 0))
    .collect();
    Ok(rows
        .iter()
        .map(|r| {
            let typ = type_name(&text(r, 1));
            let decimal = matches!(typ.as_str(), "DECIMAL" | "NUMERIC");
            let identity = text(r, 7) == "YES";
            let default = match (text(r, 5), text(r, 6).as_str(), identity) {
                (_, _, true) => "GENERATED ALWAYS AS IDENTITY".into(),
                (d, _, _) if !d.is_empty() => d,
                (_, "Y", _) => "DEFAULT".into(),
                _ => String::new(),
            };
            let name = text(r, 0);
            Column {
                key: keys.contains(&name),
                name,
                length: u32::try_from(num(r, 2)).unwrap_or(0),
                scale: (decimal && !text(r, 3).is_empty())
                    .then(|| u32::try_from(num(r, 3)).unwrap_or(0)),
                nullable: text(r, 4) == "Y",
                default,
                choices: Vec::new(),
                typ,
            }
        })
        .collect())
}

/// The profile the shared requests are listed under: key
/// `_s<owner>.<id>`, the group or `*PUBLIC` as the value.
const SHARED: &str = "*SHARED";

/// `who`'s entries in the store: key and value.
fn entries(m: &Main, who: &str) -> Vec<(String, String)> {
    let w = who.to_string();
    m.ask(|l| l.web_store(INDEX.into(), w, None))
        .unwrap_or_default()
}

/// Replace `who`'s entries whose keys start with `what.` by `value` in
/// parts (none: removed); every other entry stays.
fn put(m: &Main, who: &str, what: &str, value: Option<&str>) -> Result<(), String> {
    let mut all = entries(m, who);
    let head = format!("{what}.");
    all.retain(|(k, _)| !k.starts_with(&head));
    if let Some(v) = value {
        all.extend(
            parts(v)
                .into_iter()
                .enumerate()
                .map(|(i, p)| (format!("{what}.{i:03}"), p)),
        );
    }
    let w = who.to_string();
    m.ask(|l| l.web_store(INDEX.into(), w, Some(all)))
        .map(|_| ())
}

/// `who`'s entries whose keys start with `what`, as (owner, key, value).
fn find(m: &Main, who: &str, what: &str) -> Vec<(String, String, String)> {
    entries(m, who)
        .into_iter()
        .filter(|(k, _)| k.starts_with(what))
        .map(|(k, v)| (who.to_string(), k, v))
        .collect()
}

/// A text in parts that fit an entry, cut between characters.
fn parts(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in s.chars() {
        if cur.len() + c.len_utf8() > ROOM {
            out.push(std::mem::take(&mut cur));
        }
        cur.push(c);
    }
    if !cur.is_empty() || out.is_empty() {
        out.push(cur);
    }
    out
}

/// `owner`'s request `id`, if it has one.
fn request_of(m: &Main, owner: &str, id: u32) -> Option<Request> {
    let name = format!("_q{id:04}");
    blobs(find(m, owner, &format!("{name}.")))
        .into_iter()
        .find(|(_, n, _)| *n == name)
        .and_then(|(_, _, b)| of_blob(owner, id, &b))
}

/// List (or stop listing) `owner`'s request `id` as shared with `with`.
fn share(m: &Main, owner: &str, id: u32, with: &str) -> Result<(), String> {
    let mut all = entries(m, SHARED);
    let k = format!("_s{owner}.{id:04}");
    all.retain(|(x, _)| *x != k);
    if !with.is_empty() {
        all.push((k, with.to_string()));
    }
    m.ask(|l| l.web_store(INDEX.into(), SHARED.into(), Some(all)))
        .map(|_| ())
}

/// Blobs by owner and name, their parts in order.
fn blobs(rows: Vec<(String, String, String)>) -> Vec<(String, String, String)> {
    let mut by: std::collections::BTreeMap<(String, String), Vec<(String, String)>> =
        std::collections::BTreeMap::new();
    for (who, k, v) in rows {
        let (name, part) = k.rsplit_once('.').unwrap_or((&k, "000"));
        by.entry((who, name.to_string()))
            .or_default()
            .push((part.to_string(), v));
    }
    by.into_iter()
        .map(|((who, name), mut ps)| {
            ps.sort();
            (who, name, ps.into_iter().map(|(_, v)| v).collect())
        })
        .collect()
}

fn enc(s: &str) -> String {
    qsweb::http::form::encode(s)
}

fn dec(s: &str) -> String {
    qsweb::http::form::decode(s, false)
}

/// A request as its blob: the fields on the first line, the body after.
fn to_blob(r: &Request) -> String {
    format!(
        "{}&{}&{}&{}&{}\n{}",
        enc(&r.name),
        r.typ.key(),
        enc(&r.format),
        r.last_run,
        enc(&r.shared),
        r.body
    )
}

fn of_blob(owner: &str, id: u32, b: &str) -> Option<Request> {
    let (head, body) = b.split_once('\n').unwrap_or((b, ""));
    let f: Vec<&str> = head.split('&').collect();
    let [name, typ, format, last, shared] = f[..] else {
        return None;
    };
    Some(Request {
        id,
        owner: owner.to_string(),
        name: dec(name),
        typ: ReqType::of_key(typ)?,
        body: body.to_string(),
        format: dec(format),
        last_run: last.parse().unwrap_or(0),
        shared: dec(shared),
    })
}

impl LiveDb {
    /// `f` with the connection's lane and engine, as `user`, on the
    /// job's thread.
    fn with_conn<T: Send + 'static>(
        &self,
        user: &str,
        conn: u32,
        f: impl FnOnce(&mut dyn Engine, &mut Lane, &mut Cursors, &AtomicBool) -> T + Send + 'static,
    ) -> Option<T> {
        let cancel = {
            let owners = self.owners.lock().expect("owners");
            let o = owners.get(&conn).filter(|o| o.user == user)?;
            o.cancel.store(false, Ordering::SeqCst);
            o.cancel.clone()
        };
        let conns = self.conns.clone();
        let (busy, flag, number) = (self.busy.clone(), self.running.clone(), self.number);
        let u = user.to_string();
        self.link
            .bridge
            .call(move |m| {
                let mut c = conns.lock().expect("conns").remove(&conn)?;
                let r = m.as_user(&u, |m| {
                    let (link, console) = (&m.link, &mut m.console);
                    let mut e = Scoped {
                        inner: LaneDb(link, console),
                        prefix: format!("AFW{conn}"),
                    };
                    busy.store(conn, Ordering::SeqCst);
                    let r = running(&flag, number, || {
                        f(&mut e, &mut c.lane, &mut c.cursors, &cancel)
                    });
                    busy.store(0, Ordering::SeqCst);
                    r
                });
                conns.lock().expect("conns").insert(conn, c);
                r.ok()
            })
            .flatten()
    }
}

impl Db for LiveDb {
    fn schemas(&self, user: &str, pattern: &str) -> Vec<String> {
        let p = Val::Text(pattern.to_string());
        self.link.read(user, move |m| {
            catalog(m, "SELECT SCHEMA_NAME FROM QSYS2.SYSSCHEMAS WHERE SCHEMA_NAME LIKE ? ORDER BY SCHEMA_NAME", &[p])
                .unwrap_or_default()
                .iter()
                .map(|r| text(r, 0))
                .collect()
        })
    }

    fn tables(&self, user: &str, schema: &str) -> Result<Vec<Entry>, Message> {
        let s = schema.to_string();
        let u = user.to_string();
        self.link
            .bridge
            .call(move |m| {
                let now = m.ask(Client::now);
                let offset = i64::from(now.offset_min) * 60;
                m.as_user(&u, |m| tables(m, &s, offset)).and_then(|r| r)
            })
            .unwrap_or_else(|| Err(gone()))
    }

    fn columns(&self, user: &str, schema: &str, table: &str) -> Result<Vec<Column>, Message> {
        let (s, t, u) = (schema.to_string(), table.to_string(), user.to_string());
        self.link
            .bridge
            .call(move |m| m.as_user(&u, |m| columns(m, &s, &t)).and_then(|r| r))
            .unwrap_or_else(|| Err(gone()))
    }

    fn open(&self, user: &str) -> Result<u32, Message> {
        let mut owners = self.owners.lock().expect("owners");
        if owners.values().filter(|o| o.user == user).count() >= MAX {
            return Err(Message::new(
                MsgKind::Error,
                accessweb::msgs::NO_CONNECTIONS,
                "No more connections allowed to this system. Close a window and try again.",
            ));
        }
        let n = self.next.fetch_add(1, Ordering::SeqCst);
        owners.insert(
            n,
            Owner {
                user: user.to_string(),
                cancel: Arc::new(AtomicBool::new(false)),
            },
        );
        self.conns.lock().expect("conns").insert(n, Conn::default());
        Ok(n)
    }

    fn close(&self, user: &str, conn: u32) {
        let _ = self.with_conn(user, conn, |e, lane, cursors, _| {
            cursors.clear();
            lane.end(e);
        });
        self.conns.lock().expect("conns").remove(&conn);
        self.owners.lock().expect("owners").remove(&conn);
    }

    fn options(&self, user: &str, conn: u32, opts: &Options) -> Result<(), Message> {
        let o = SetOptions {
            commit: commit_level(opts.commit),
            naming: if opts.system_naming {
                Naming::System
            } else {
                Naming::Sql
            },
            default_schema: (!opts.schema.trim().is_empty())
                .then(|| opts.schema.trim().to_string()),
        };
        self.with_conn(user, conn, move |e, lane, _, _| {
            lane.set_options(e, &o).map_err(|r| message(&r.message))
        })
        .unwrap_or_else(|| Err(gone()))
    }

    fn run(
        &self,
        user: &str,
        conn: u32,
        sql: &str,
        params: &[Vec<Val>],
        max_rows: usize,
    ) -> Outcome {
        let (s, p) = (sql.to_string(), params.to_vec());
        self.with_conn(user, conn, move |e, lane, cursors, cancel| {
            statement(e, lane, cursors, &s, &p, max_rows, cancel)
        })
        .unwrap_or_else(|| Outcome {
            message: Some(gone()),
            sqlstate: "08003".into(),
            sqlcode: -901,
            failed: true,
            ..Outcome::default()
        })
    }

    fn fetch(&self, user: &str, conn: u32, cursor: u32, max_rows: usize) -> Outcome {
        self.with_conn(user, conn, move |e, lane, cursors, cancel| {
            let max = u32::try_from(max_rows).unwrap_or(0);
            let cancelled = || cancel.load(Ordering::SeqCst);
            match lane.fetch(
                e,
                &Fetch {
                    cursor,
                    max_rows: max,
                },
                &cancelled,
            ) {
                Ok(f) => {
                    let scales = cursors
                        .get(&cursor)
                        .map(|c| c.1.clone())
                        .unwrap_or_default();
                    let rows: Vec<Vec<Val>> = f
                        .rows
                        .iter()
                        .map(|r| {
                            r.iter()
                                .enumerate()
                                .map(|(i, v)| val(v, scales.get(i).copied().unwrap_or(6)))
                                .collect()
                        })
                        .collect();
                    let more = !f.end;
                    if f.end {
                        let _ = lane.close_cursor(e, cursor);
                        if let Some((st, _)) = cursors.remove(&cursor) {
                            let _ = lane.release(e, st);
                        }
                    }
                    Outcome {
                        columns: vec![ResultColumn::default(); rows.first().map_or(0, Vec::len)],
                        rows,
                        cursor: more.then_some(cursor),
                        sqlstate: "00000".into(),
                        ..Outcome::default()
                    }
                }
                Err(s) => {
                    cursors.remove(&cursor);
                    stopped(s)
                }
            }
        })
        .unwrap_or_else(|| Outcome {
            message: Some(gone()),
            failed: true,
            ..Outcome::default()
        })
    }

    fn end_unit(&self, user: &str, conn: u32, commit: bool) -> Outcome {
        self.with_conn(user, conn, move |e, lane, _, _| {
            match lane.end_unit(e, commit) {
                Ok(()) => Outcome {
                    sqlstate: "00000".into(),
                    ..Outcome::default()
                },
                Err(s) => stopped(s),
            }
        })
        .unwrap_or_else(|| Outcome {
            message: Some(gone()),
            failed: true,
            ..Outcome::default()
        })
    }

    fn cancel(&self, user: &str, conn: u32) {
        // Off the job's thread: the statement holds it.
        let owners = self.owners.lock().expect("owners");
        if let Some(o) = owners.get(&conn).filter(|o| o.user == user)
            && self.busy.load(Ordering::SeqCst) == conn
        {
            o.cancel.store(true, Ordering::SeqCst);
            crate::work::qap::sql::ask_cancel(self.number);
        }
    }

    fn requests(&self, user: &str, groups: &[String]) -> Vec<Request> {
        let (u, g) = (user.to_string(), groups.to_vec());
        self.link
            .job(move |m| {
                let mut out: Vec<Request> = blobs(find(m, &u, "_q"))
                    .into_iter()
                    .filter_map(|(who, name, b)| {
                        of_blob(&who, name.strip_prefix("_q")?.parse().ok()?, &b)
                    })
                    .collect();
                for (k, with) in entries(m, SHARED) {
                    let Some((owner, id)) = k.strip_prefix("_s").and_then(|r| r.split_once('.'))
                    else {
                        continue;
                    };
                    if owner == u || !(with == "*PUBLIC" || g.contains(&with)) {
                        continue;
                    }
                    if let Some(r) = id.parse().ok().and_then(|id| request_of(m, owner, id)) {
                        out.push(r);
                    }
                }
                out
            })
            .unwrap_or_default()
    }

    fn save_request(&self, user: &str, r: &Request) -> Result<u32, Message> {
        let (u, mut r) = (user.to_string(), r.clone());
        r.owner.clone_from(&u);
        let res = self.link.job(move |m| {
            let mine = blobs(find(m, &u, "_q"));
            let id = if r.id == 0 {
                mine.iter()
                    .filter_map(|(_, n, _)| n.strip_prefix("_q")?.parse::<u32>().ok())
                    .max()
                    .unwrap_or(0)
                    + 1
            } else if mine.iter().any(|(_, n, _)| *n == format!("_q{:04}", r.id)) {
                r.id
            } else {
                return Err("CPF9801".to_string());
            };
            r.id = id;
            put(m, &u, &format!("_q{id:04}"), Some(&to_blob(&r)))?;
            share(m, &u, id, &r.shared).map(|()| id)
        });
        res.unwrap_or_else(|| Err("CPF9897".into())).map_err(|id| {
            Message::new(
                MsgKind::Error,
                &id,
                &format!("The request could not be kept in {LIB}/{INDEX}."),
            )
        })
    }

    fn delete_request(&self, user: &str, id: u32) -> Result<(), Message> {
        let u = user.to_string();
        let r = self.link.job(move |m| {
            let had = request_of(m, &u, id).is_some();
            put(m, &u, &format!("_q{id:04}"), None)?;
            share(m, &u, id, "")?;
            if had {
                Ok(())
            } else {
                Err("CPF9801".to_string())
            }
        });
        r.unwrap_or_else(|| Err("CPF9897".into()))
            .map_err(|e| Message::new(MsgKind::Error, &e, &format!("Request {id} not found.")))
    }

    fn history(&self, user: &str) -> Vec<String> {
        let u = user.to_string();
        self.link
            .job(move |m| {
                blobs(find(m, &u, "_h"))
                    .into_iter()
                    .flat_map(|(_, _, b)| b.lines().map(dec).collect::<Vec<_>>())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn remember(&self, user: &str, statements: &[String]) {
        let mut h = self.history(user);
        for s in statements {
            let one = s.split_whitespace().collect::<Vec<_>>().join(" ");
            h.retain(|x| *x != one);
            h.insert(0, one);
        }
        h.truncate(100);
        let (u, blob) = (
            user.to_string(),
            h.iter().map(|s| enc(s)).collect::<Vec<_>>().join("\n"),
        );
        let _ = self.link.job(move |m| put(m, &u, "_h", Some(&blob)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_blobs_and_stamps() {
        assert_eq!(
            val(
                &SqlValue::Decimal {
                    text: "1200.05".into()
                },
                2
            ),
            Val::Num("1200.05".into())
        );
        assert_eq!(
            val(
                &SqlValue::Date {
                    ms: 1_793_491_200_000
                },
                0
            ),
            Val::Text("2026-11-01".into())
        );
        assert_eq!(
            val(
                &SqlValue::Timestamp {
                    ms: 1_793_491_200_123,
                    picos: 456_000_000
                },
                6
            ),
            Val::Text("2026-11-01-00.00.00.123456".into())
        );
        assert_eq!(
            param(&Val::Text(" 12 ".into()), "DECIMAL"),
            SqlValue::Decimal { text: "12".into() }
        );
        let r = Request {
            id: 3,
            owner: "BOB".into(),
            name: "A & B".into(),
            typ: ReqType::CopyTo,
            body: "line 1\nline 2".into(),
            format: "CSV".into(),
            last_run: 7,
            shared: "*PUBLIC".into(),
        };
        assert_eq!(of_blob("BOB", 3, &to_blob(&r)), Some(r));
        let long = "é".repeat(80);
        let p = parts(&long);
        assert!(p.iter().all(|x| x.len() <= ROOM) && p.concat() == long);
        assert_eq!(stamp("2026-11-01-01.00.00.000000", 3600), 1_793_491_200);
    }
}
