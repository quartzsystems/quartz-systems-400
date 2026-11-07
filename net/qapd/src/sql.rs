// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `Sql` service (version 4): operations 1 to 8 in the lane, on the
//! job's own thread ([`lane`]), the catalog lists 9 to 11 on a
//! secondary thread ([`catalog`]). The host runs the statements; this
//! module decodes, answers, and keeps the cancel rules: a started
//! `Execute` ends with its rows changed so far and no cursor, a started
//! `Fetch` with its rows so far, `end` 0 and the cursor closed, both
//! with the SQL message for the cancel ([`CANCELLED`]).

use ipc::Wire;
use qap_proto::ops::sql::{
    CursorRef, Execute, Executed, Fetch, Fetched, ListColumns, ListSchemas, ListTables, Prepare,
    SetOptions, SqlFailure, StatementRef,
};
use qap_proto::{Envelope, Op, ServerMessage, Status};

use crate::conn::Answer;
use crate::host::{Host, Refusal, ThreadHost};

/// The SQL message of a statement a cancel ended.
pub const CANCELLED: &str = "SQL0952";

/// An SQL statement failed: `SqlError`, the body `failure`, the
/// message the SQL message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlFailed {
    /// SQLSTATE, SQLCODE and the failing row.
    pub failure: SqlFailure,
    /// The SQL message (`SQL0803`, ...).
    pub message: ServerMessage,
}

/// How an `Sql` operation ended other than `Ok`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlStop<T> {
    /// The statement failed: `SqlError`.
    Failed(Box<SqlFailed>),
    /// Refused before or instead of running: `NotFound`,
    /// `NotAuthorized`, `InUse` or `Error`, with the message why.
    Refused(Refusal),
    /// Cancelled once started: the operation's own body as far as it
    /// got, and the message for the cancel.
    Cancelled(T, Box<ServerMessage>),
}

impl<T> SqlStop<T> {
    /// `SqlError` with `failure` and the SQL message.
    #[must_use]
    pub fn failed(failure: SqlFailure, message: ServerMessage) -> Self {
        SqlStop::Failed(Box::new(SqlFailed { failure, message }))
    }

    /// `Cancelled` with the body so far and the message for the cancel.
    #[must_use]
    pub fn cancelled(body: T, message: ServerMessage) -> Self {
        SqlStop::Cancelled(body, Box::new(message))
    }
}

/// An `Sql` operation's result.
pub type SqlResult<T> = Result<T, SqlStop<T>>;

/// The answer to a result.
fn answer<T: Wire>(r: SqlResult<T>) -> Answer {
    match r {
        Ok(b) => Answer::ok(&b),
        Err(SqlStop::Failed(f)) => Answer {
            status: Status::SqlError,
            body: ipc::encode(&f.failure),
            message: Some(f.message),
        },
        Err(SqlStop::Refused(r)) => Answer::refused(r),
        Err(SqlStop::Cancelled(b, m)) => Answer {
            status: Status::Cancelled,
            message: Some(*m),
            body: ipc::encode(&b),
        },
    }
}

/// The empty answer to an operation with no reply body.
fn unit(r: SqlResult<()>) -> Answer {
    match r {
        Ok(()) => Answer::status(Status::Ok),
        Err(SqlStop::Cancelled((), m)) => Answer {
            status: Status::Cancelled,
            message: Some(*m),
            body: Vec::new(),
        },
        Err(e) => answer(Err::<(), _>(e)),
    }
}

fn refused(r: Result<(), Refusal>) -> Answer {
    match r {
        Ok(()) => Answer::status(Status::Ok),
        Err(r) => Answer::refused(r),
    }
}

/// Run an operation of the lane (`Sql` 1 to 8) on the job's own thread.
/// `cancelled` says whether the client cancelled this request.
pub(crate) fn lane<H: Host + ?Sized>(
    host: &mut H,
    op: Op,
    req: &Envelope,
    cancelled: &dyn Fn() -> bool,
) -> Answer {
    fn body<B: Wire>(req: &Envelope) -> Option<B> {
        req.body_as::<B>().ok()
    }
    let bad = || Answer::status(Status::InvalidRequest);
    match op {
        Op::SetOptions => {
            body::<SetOptions>(req).map_or_else(bad, |b| refused(host.sql_set_options(&b)))
        }
        Op::Prepare => body::<Prepare>(req).map_or_else(bad, |b| answer(host.sql_prepare(&b.sql))),
        Op::Execute => body::<Execute>(req).map_or_else(bad, |b| {
            let r = host.sql_execute(&b, cancelled);
            answer(after_execute(host, r, cancelled))
        }),
        Op::Fetch => body::<Fetch>(req).map_or_else(bad, |b| {
            let r = host.sql_fetch(&b, cancelled);
            answer(after_fetch(host, b.cursor, r, cancelled))
        }),
        Op::CloseCursor => {
            body::<CursorRef>(req).map_or_else(bad, |b| refused(host.sql_close_cursor(b.cursor)))
        }
        Op::Release => {
            body::<StatementRef>(req).map_or_else(bad, |b| refused(host.sql_release(b.statement)))
        }
        Op::Commit => unit(host.sql_commit()),
        Op::Rollback => unit(host.sql_rollback()),
        _ => bad(),
    }
}

/// An `Execute` the client cancelled while the host finished it anyway:
/// the cursor it opened is closed, the rows it changed stay counted.
fn after_execute<H: Host + ?Sized>(
    host: &mut H,
    r: SqlResult<Executed>,
    cancelled: &dyn Fn() -> bool,
) -> SqlResult<Executed> {
    match r {
        Ok(mut e) if cancelled() => {
            if let Some(c) = e.cursor.take() {
                let _ = host.sql_close_cursor(c);
            }
            Err(SqlStop::cancelled(e, host.message(CANCELLED, &[])))
        }
        r => r,
    }
}

/// A `Fetch` the client cancelled while the host finished it anyway:
/// the rows go back, `end` 0, and the cursor is closed.
fn after_fetch<H: Host + ?Sized>(
    host: &mut H,
    cursor: u32,
    r: SqlResult<Fetched>,
    cancelled: &dyn Fn() -> bool,
) -> SqlResult<Fetched> {
    match r {
        Ok(mut f) if cancelled() => {
            let _ = host.sql_close_cursor(cursor);
            f.end = false;
            Err(SqlStop::cancelled(f, host.message(CANCELLED, &[])))
        }
        r => r,
    }
}

/// Answer a catalog list (`Sql` 9 to 11) on a secondary thread.
pub(crate) fn catalog(host: &mut dyn ThreadHost, op: Op, req: &Envelope) -> Answer {
    let ok = |r: Result<_, Refusal>| -> Answer {
        match r {
            Ok(b) => b,
            Err(r) => Answer::refused(r),
        }
    };
    match op {
        Op::ListSchemas => match req.body_as::<ListSchemas>() {
            Ok(b) => ok(host.list_schemas(&b).map(|r| Answer::ok(&r))),
            Err(_) => Answer::status(Status::InvalidRequest),
        },
        Op::ListTables => match req.body_as::<ListTables>() {
            Ok(b) => ok(host.list_tables(&b).map(|r| Answer::ok(&r))),
            Err(_) => Answer::status(Status::InvalidRequest),
        },
        Op::ListColumns => match req.body_as::<ListColumns>() {
            Ok(b) => ok(host.list_columns(&b).map(|r| Answer::ok(&r))),
            Err(_) => Answer::status(Status::InvalidRequest),
        },
        _ => Answer::status(Status::InvalidRequest),
    }
}

/// Entries `start` to `start + max` (`max` 0 for all) of a list, and how
/// many there are: the paging every long list has.
#[must_use]
pub fn page<T>(all: Vec<T>, start: u32, max: u32) -> (u32, Vec<T>) {
    let total = u32::try_from(all.len()).unwrap_or(u32::MAX);
    let from = usize::try_from(start).unwrap_or(usize::MAX);
    let n = if max == 0 {
        usize::MAX
    } else {
        usize::try_from(max).unwrap_or(usize::MAX)
    };
    (total, all.into_iter().skip(from).take(n).collect())
}

/// Whether `name` matches the SQL `LIKE` pattern `pattern`: `%` any run,
/// `_` one character; no escape. An empty pattern matches everything.
#[must_use]
pub fn like(name: &str, pattern: &str) -> bool {
    fn go(n: &[char], p: &[char]) -> bool {
        match p.split_first() {
            None => n.is_empty(),
            Some(('%', rest)) => (0..=n.len()).any(|i| go(&n[i..], rest)),
            Some(('_', rest)) => !n.is_empty() && go(&n[1..], rest),
            Some((c, rest)) => n.first() == Some(c) && go(&n[1..], rest),
        }
    }
    if pattern.is_empty() {
        return true;
    }
    let n: Vec<char> = name.chars().collect();
    let p: Vec<char> = pattern.chars().collect();
    go(&n, &p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn like_patterns() {
        assert!(like("HLINV", "HL%"));
        assert!(like("HLINV", "%"));
        assert!(like("HLINV", ""));
        assert!(like("HLINV", "HL_NV"));
        assert!(!like("HLINV", "HL_V"));
        assert!(!like("QGPL", "HL%"));
        assert!(like("A_B", "A_B"));
    }

    #[test]
    fn pages() {
        let v: Vec<u32> = (0..10).collect();
        assert_eq!(page(v.clone(), 0, 0), (10, v.clone()));
        assert_eq!(page(v.clone(), 8, 5), (10, vec![8, 9]));
        assert_eq!(page(v.clone(), 20, 5), (10, vec![]));
        assert_eq!(page(v, 2, 3), (10, vec![2, 3, 4]));
    }

    #[test]
    fn sql_error_is_error_before_version_4() {
        assert_eq!(Status::SqlError.for_version(3), Status::Error);
        assert_eq!(Status::SqlError.for_version(4), Status::SqlError);
    }
}
