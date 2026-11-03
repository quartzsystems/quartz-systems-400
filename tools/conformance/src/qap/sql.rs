// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Sql` service for steps 89 and 90: statements prepared and run,
//! rows fetched, the failures' bodies, and a long query cancelled.

use std::time::{Duration, Instant};

use qap_proto::ops::sql::{
    CommitLevel, CursorRef, Execute, Executed, Fetch, Fetched, Naming, Prepare, Prepared, SetOptions, SqlFailure,
    SqlValue, StatementRef,
};
use qap_proto::{Envelope, Op, Status};

use super::client::{Conn, WAIT, brief, msgid};

/// `SetOptions`.
pub fn options(c: &mut Conn, commit: CommitLevel, schema: Option<&str>) -> Result<(), String> {
    c.ok::<_, ()>(
        Op::SetOptions,
        &SetOptions {
            commit,
            naming: Naming::Sql,
            default_schema: schema.map(str::to_string),
        },
    )
}

/// `Prepare`.
pub fn prepare(c: &mut Conn, sql: &str) -> Result<Prepared, String> {
    c.ok(Op::Prepare, &Prepare { sql: sql.into() }).map_err(|e| format!("{sql}: {e}"))
}

/// `Execute` of `statement` over `rows`: the reply as it came.
pub fn execute(c: &mut Conn, statement: u32, rows: Vec<Vec<SqlValue>>) -> Result<Envelope, String> {
    c.call(Op::Execute, &Execute { statement, rows })
}

/// Prepare and run `sql` once, which must succeed; rows changed and a
/// query's cursor.
pub fn run(c: &mut Conn, sql: &str) -> Result<Executed, String> {
    let p = prepare(c, sql)?;
    let r = execute(c, p.statement, Vec::new())?;
    if r.status != Status::Ok {
        return Err(format!("{sql}: {}", failure(&r)));
    }
    let e: Executed = r.body_as().map_err(|e| e.to_string())?;
    if e.cursor.is_none() {
        c.ok::<_, ()>(Op::Release, &StatementRef { statement: p.statement })?;
    }
    Ok(e)
}

/// Run `sql`, which may fail only with `allowed` SQLSTATEs (an object
/// a previous run left).
pub fn run_or(c: &mut Conn, sql: &str, allowed: &[&str]) -> Result<(), String> {
    let p = prepare(c, sql)?;
    let r = execute(c, p.statement, Vec::new())?;
    let state = r.body_as::<SqlFailure>().map(|f| f.sqlstate).unwrap_or_default();
    if r.status == Status::Ok || (r.status == Status::SqlError && allowed.contains(&state.as_str())) {
        return Ok(());
    }
    Err(format!("{sql}: {}", failure(&r)))
}

/// A failed reply in a line, with its `SqlFailure` when it is one.
pub fn failure(r: &Envelope) -> String {
    match r.body_as::<SqlFailure>() {
        Ok(f) if r.status == Status::SqlError => {
            format!("{} SQLSTATE {} SQLCODE {} row {:?}", brief(r), f.sqlstate, f.sqlcode, f.row)
        }
        _ => brief(r),
    }
}

/// Every row of a query.
pub fn query(c: &mut Conn, sql: &str) -> Result<Vec<Vec<SqlValue>>, String> {
    let stmt = prepare(c, sql)?;
    let reply = execute(c, stmt.statement, Vec::new())?;
    if reply.status != Status::Ok {
        return Err(format!("{sql}: {}", failure(&reply)));
    }
    let ran: Executed = reply.body_as().map_err(|e| e.to_string())?;
    let cursor = ran.cursor.ok_or_else(|| format!("{sql}: no cursor"))?;
    let rows: Fetched = c.ok(Op::Fetch, &Fetch { cursor, max_rows: 0 })?;
    if !rows.end {
        return Err(format!("{sql}: Fetch of every row did not end the cursor"));
    }
    let _ = c.call(Op::CloseCursor, &CursorRef { cursor })?;
    c.ok::<_, ()>(Op::Release, &StatementRef { statement: stmt.statement })?;
    Ok(rows.rows)
}

/// A one-value query's integer.
pub fn count(c: &mut Conn, sql: &str) -> Result<i64, String> {
    let rows = query(c, sql)?;
    match rows.first().and_then(|r| r.first()) {
        Some(SqlValue::Integer { value }) => Ok(*value),
        Some(SqlValue::Decimal { text }) => text.parse().map_err(|_| format!("{sql}: {text}")),
        other => Err(format!("{sql}: {other:?}")),
    }
}

/// An integer value.
pub fn int(v: i64) -> SqlValue {
    SqlValue::Integer { value: v }
}

/// A text value.
pub fn text(v: &str) -> SqlValue {
    SqlValue::Text { text: v.into() }
}

/// A decimal value.
pub fn dec(v: &str) -> SqlValue {
    SqlValue::Decimal { text: v.into() }
}

/// The table step 90's long query reads: `rows` rows of one integer,
/// made once.
pub fn big_table(c: &mut Conn, schema: &str, rows: i64) -> Result<(), String> {
    run_or(c, &format!("CREATE SCHEMA {schema}"), &["42710", "58004"])?;
    run_or(c, &format!("CREATE TABLE {schema}.BIG (N INTEGER NOT NULL)"), &["42710", "58004"])?;
    let have = count(c, &format!("SELECT COUNT(*) FROM {schema}.BIG"))?;
    if have >= rows {
        return Ok(());
    }
    let p = prepare(c, &format!("INSERT INTO {schema}.BIG VALUES(?)"))?;
    let r = execute(c, p.statement, (have..rows).map(|n| vec![int(n)]).collect())?;
    if r.status != Status::Ok {
        return Err(format!("filling {schema}.BIG: {}", failure(&r)));
    }
    c.ok::<_, ()>(Op::Release, &StatementRef { statement: p.statement })
}

/// The query that runs long: correlated subqueries two deep that never
/// match, so each outer row scans `BIG` squared, a row at a time. Not a
/// join: the engine builds a join's rows in memory, and three copies of
/// 400 rows were 1.5 GB, more than the storage service could allocate.
pub fn long_query(schema: &str) -> String {
    format!(
        "SELECT COUNT(*) FROM {schema}.BIG A WHERE EXISTS (SELECT 1 FROM {schema}.BIG B WHERE EXISTS (SELECT 1 FROM {schema}.BIG C WHERE C.N > A.N + B.N + 1000000))"
    )
}

/// A request that must still be running after `settle`; `Err` when it
/// ended first (with how).
pub fn still_running(c: &mut Conn, id: u32, settle: Duration) -> Result<(), String> {
    match c.poll_reply(id, settle)? {
        None => Ok(()),
        Some(r) => Err(format!("the long query ended after {}s: {}", settle.as_secs(), failure(&r))),
    }
}

/// Start the long query on `c`: the request running it (an `Execute`,
/// or a `Fetch` when `Execute` returned at once with the cursor).
pub fn start_long(c: &mut Conn, schema: &str) -> Result<(u32, Op), String> {
    start_query(c, &long_query(schema))
}

/// A plain select that makes its rows slowly: each row's value is a
/// subquery as long as one outer row of [`long_query`].
pub fn slow_select(schema: &str) -> String {
    format!(
        "SELECT A.N, (SELECT COUNT(*) FROM {schema}.BIG B WHERE EXISTS (SELECT 1 FROM {schema}.BIG C WHERE C.N > A.N + B.N + 1000000)) FROM {schema}.BIG A"
    )
}

/// Start query `sql` on `c` as [`start_long`] does.
pub fn start_query(c: &mut Conn, sql: &str) -> Result<(u32, Op), String> {
    let p = prepare(c, sql)?;
    let id = c.request(
        Op::Execute,
        &Execute {
            statement: p.statement,
            rows: Vec::new(),
        },
    )?;
    if let Some(r) = c.poll_reply(id, Duration::from_secs(2))? {
        if r.status != Status::Ok {
            return Err(format!("the long query's Execute: {}", failure(&r)));
        }
        let e: Executed = r.body_as().map_err(|e| e.to_string())?;
        let cursor = e.cursor.ok_or("the long query has no cursor")?;
        let id = c.request(Op::Fetch, &Fetch { cursor, max_rows: 0 })?;
        return Ok((id, Op::Fetch));
    }
    Ok((id, Op::Execute))
}

/// Start statement `sql` on `c` without waiting for its reply: the
/// request running it.
pub fn start_exec(c: &mut Conn, sql: &str) -> Result<u32, String> {
    let p = prepare(c, sql)?;
    c.request(Op::Execute, &Execute { statement: p.statement, rows: Vec::new() })
}

/// The reply to `id`, which must be `Ok`, within `limit`.
pub fn done(c: &mut Conn, id: u32, what: &str, limit: Duration) -> Result<(), String> {
    match c.poll_reply(id, limit)? {
        Some(r) if r.status == Status::Ok => Ok(()),
        Some(r) => Err(format!("{what}: {}", failure(&r))),
        None => Err(format!("{what}: no reply in {} s", limit.as_secs())),
    }
}

/// Cancel `id` and wait for its `Cancelled` reply; how long that took.
pub fn cancel(c: &mut Conn, id: u32) -> Result<(Envelope, Duration), String> {
    let at = Instant::now();
    c.cancel(id)?;
    let r = c.reply(id, WAIT)?;
    if r.status != Status::Cancelled {
        return Err(format!("the cancelled query ended {} ({})", failure(&r), msgid(&r)));
    }
    Ok((r, at.elapsed()))
}
