// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! STRSQL's storage requests (400-419): the job's SQL session started
//! and ended (naming, commitment control), one statement run with its
//! rows, and each user's saved session, kept in the user space
//! `QUSRSYS/QSQLSESS` as `user, length, text` records.

use std::collections::BTreeMap;

use db::cmt::LckLvl;
use qsname::Name;
use sql::ast::Naming;
use sql::{Outcome, Prepared, parse};
use stg::proto::IsqlReply;
use store::obj::library::Library;
use store::obj::ops::{self, CreateParams};
use store::obj::{ObjectId, types, usrspc};
use store::txn::Txn;

use crate::ipl::system::System;
use crate::svc::db::{State, db, db_err, now, with_txn};

/// The user space holding every user's saved session.
const SESSIONS: (&str, &str) = ("QUSRSYS", "QSQLSESS");

/// Run `f` with the job's console state (its SQL context) in place of
/// the system's.
pub fn in_job<T>(system: &mut System, svc: &mut State, f: impl FnOnce(&mut System) -> T) -> T {
    std::mem::swap(&mut system.svc, svc);
    let r = f(system);
    std::mem::swap(&mut system.svc, svc);
    r
}

fn naming_of(n: &str) -> Naming {
    if n.eq_ignore_ascii_case("*SQL") {
        Naming::Sql
    } else {
        Naming::Sys
    }
}

/// `isql_start`: naming set, commitment control started at `commit`'s
/// lock level unless the job has a definition already; whether one was
/// started.
pub fn start(system: &mut System, user: &str, naming: &str, commit: &str) -> Result<bool, String> {
    let (store, ctx) = db(system)?;
    ctx.naming = naming_of(naming);
    ctx.user = user.to_string();
    let Some(level) = LckLvl::parse(commit) else {
        return Ok(false);
    };
    if ctx.session.commit_def().is_some() {
        return Ok(false);
    }
    with_txn(store, |txn| {
        ctx.session
            .strcmtctl(txn, level, None, now())
            .map_err(|e| db_err(&e))
    })?;
    Ok(true)
}

/// `isql_run`: one statement, its rows cut at `max_rows`.
pub fn run(system: &mut System, text: &str, naming: &str, check: bool, max_rows: u32) -> IsqlReply {
    let naming = naming_of(naming);
    let statement = match parse(text, naming) {
        Ok(s) => s,
        Err(e) => {
            let ca = sql::Sqlca::from_error(&e);
            return IsqlReply {
                sqlcode: ca.sqlcode,
                sqlstate: ca.sqlstate,
                msgid: e.msgid(),
                text: ca.sqlerrmc,
                ..IsqlReply::default()
            };
        }
    };
    let name = sql::exec::statement_name(&statement).to_string();
    if check {
        return IsqlReply {
            statement: name,
            ..IsqlReply::default()
        };
    }
    let (store, ctx) = match db(system) {
        Ok(x) => x,
        Err(e) => return failed(&name, &e),
    };
    let p = Prepared { statement, naming };
    let mut txn = match store.begin() {
        Ok(t) => t,
        Err(e) => return failed(&name, &e.to_string()),
    };
    let (ca, outcome) = p.run(&mut txn, ctx, &[]);
    if ca.is_error() {
        txn.abort();
        return IsqlReply {
            sqlcode: ca.sqlcode,
            sqlstate: ca.sqlstate.clone(),
            msgid: ca.msgid(),
            text: ca.sqlerrmc,
            statement: name,
            ..IsqlReply::default()
        };
    }
    if let Err(e) = txn.commit() {
        return failed(&name, &e.to_string());
    }
    let mut reply = IsqlReply {
        sqlcode: ca.sqlcode,
        sqlstate: ca.sqlstate,
        statement: name,
        ..IsqlReply::default()
    };
    match outcome {
        Some(Outcome::Rows(rs)) => {
            reply.columns = rs
                .columns
                .iter()
                .map(|c| (c.name.clone(), c.data_type.clone()))
                .collect();
            let max = max_rows as usize;
            reply.truncated = rs.rows.len() > max;
            reply.count = rs.rows.len() as u64;
            reply.rows = rs
                .rows
                .into_iter()
                .take(max)
                .map(|r| {
                    r.into_iter()
                        .map(|v| (!v.is_null()).then(|| v.to_string()))
                        .collect()
                })
                .collect();
        }
        Some(Outcome::Count(n)) => reply.count = n,
        Some(Outcome::Done) | None => {}
    }
    reply
}

/// A statement that could not reach the database.
fn failed(statement: &str, why: &str) -> IsqlReply {
    IsqlReply {
        sqlcode: -901,
        sqlstate: "58004".into(),
        msgid: "SQL0901".into(),
        text: why.to_string(),
        statement: statement.to_string(),
        ..IsqlReply::default()
    }
}

/// `isql_end`: pending changes committed or rolled back, the
/// definition the session started ended; the changes there were.
pub fn end(system: &mut System, commit: bool) -> Result<u64, String> {
    let (store, ctx) = db(system)?;
    let Some(pending) = ctx.session.commit_def().map(|c| c.borrow().changes()) else {
        return Ok(0);
    };
    with_txn(store, |txn| {
        if commit {
            ctx.session.commit(txn, None).map_err(|e| db_err(&e))?;
        } else {
            ctx.session.rollbck(txn, None).map_err(|e| db_err(&e))?;
        }
        sql::exec::cursor::end_of_unit(ctx);
        ctx.session.endcmtctl(txn, None).map_err(|e| db_err(&e))?;
        Ok(pending)
    })
}

/// The sessions as stored: `user length\n` then the text, per user.
fn unpack(bytes: &[u8]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut rest = bytes;
    while let Some(nl) = rest.iter().position(|&b| b == b'\n') {
        let head = String::from_utf8_lossy(&rest[..nl]).to_string();
        let Some((user, len)) = head.split_once(' ') else {
            break;
        };
        let Ok(len) = len.trim().parse::<usize>() else {
            break;
        };
        let body = &rest[nl + 1..];
        if body.len() < len {
            break;
        }
        out.insert(
            user.to_string(),
            String::from_utf8_lossy(&body[..len]).to_string(),
        );
        rest = &body[len..];
    }
    out
}

fn pack(map: &BTreeMap<String, String>) -> Vec<u8> {
    let mut out = Vec::new();
    for (u, t) in map {
        out.extend_from_slice(format!("{u} {}\n", t.len()).as_bytes());
        out.extend_from_slice(t.as_bytes());
    }
    out
}

/// The sessions space, made when `make` and missing.
fn space(txn: &mut Txn<'_>, qsys: ObjectId, make: bool) -> Result<Option<ObjectId>, String> {
    let lib_name = Name::parse(SESSIONS.0).map_err(|e| e.to_string())?;
    let name = Name::parse(SESSIONS.1).map_err(|e| e.to_string())?;
    let lib = ops::find_library(txn, qsys, lib_name)
        .map_err(|e| e.to_string())?
        .id;
    let found = Library::load(txn, lib)
        .and_then(|l| l.lookup(txn, types::USRSPC, name))
        .map_err(|e| e.to_string())?;
    if found.is_some() || !make {
        return Ok(found);
    }
    let p = CreateParams {
        library: lib,
        name,
        type_code: types::USRSPC,
        attribute: String::new(),
        text: "Interactive SQL sessions".into(),
        owner: "QSYS".into(),
        system: "QS400".into(),
        now: now(),
        by_system: true,
    };
    usrspc::create(txn, &p, 0, true, 0)
        .map(Some)
        .map_err(|e| e.to_string())
}

fn read_all(txn: &mut Txn<'_>, id: ObjectId) -> Result<BTreeMap<String, String>, String> {
    let size = usrspc::describe(txn, id).map_err(|e| e.to_string())?.size;
    let len = usize::try_from(size).map_err(|e| e.to_string())?;
    let bytes = usrspc::read(txn, id, 0, len).map_err(|e| e.to_string())?;
    Ok(unpack(&bytes))
}

/// The user the request runs for.
fn user() -> Result<String, String> {
    sec::guard::current()
        .map(|s| s.borrow().principal.user.clone())
        .ok_or_else(|| "no user".to_string())
}

/// `isql_session`: the job's user's saved session.
pub fn session(system: &mut System) -> Option<String> {
    let u = user().ok()?;
    let (store, ctx) = db(system).ok()?;
    let qsys = ctx.qsys;
    sec::guard::as_system(|| {
        with_txn(store, |txn| {
            let Some(id) = space(txn, qsys, false)? else {
                return Ok(None);
            };
            Ok(read_all(txn, id)?.remove(&u))
        })
    })
    .ok()
    .flatten()
}

/// `isql_session_put`: the job's user's session stored, or dropped.
pub fn session_put(system: &mut System, text: Option<&str>) -> Result<(), String> {
    let u = user()?;
    let (store, ctx) = db(system)?;
    let qsys = ctx.qsys;
    sec::guard::as_system(|| {
        with_txn(store, |txn| {
            let Some(id) = space(txn, qsys, text.is_some())? else {
                return Ok(());
            };
            let mut all = read_all(txn, id)?;
            match text {
                Some(t) => {
                    all.insert(u.clone(), t.to_string());
                }
                None => {
                    all.remove(&u);
                }
            }
            let bytes = pack(&all);
            usrspc::change_size(txn, id, bytes.len() as u64).map_err(|e| e.to_string())?;
            if !bytes.is_empty() {
                usrspc::write(txn, id, 0, &bytes).map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_pack() {
        let mut m = BTreeMap::new();
        m.insert("QPGMR".to_string(), "QSQLSESS 1\nS select 1\n".to_string());
        m.insert("BOB".to_string(), String::new());
        let back = unpack(&pack(&m));
        assert_eq!(back, m);
        assert!(unpack(b"junk").is_empty());
    }
}
