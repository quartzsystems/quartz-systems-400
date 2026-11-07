// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SAVEPOINT`, `RELEASE SAVEPOINT` and `ROLLBACK TO SAVEPOINT` over the
//! commit definition in effect. Without commitment control a savepoint
//! is kept by name and marks nothing: a rollback to it undoes nothing,
//! as on the primary under autocommit.

use db::cmt::SetSavepoint;
use store::txn::Txn;

use super::{Outcome, SqlContext};
use crate::ast::SavepointStmt;
use crate::error::{Result, SqlError};

/// The prefix of the savepoints of `ATOMIC` compounds: one nests over
/// another of its name, so a routine may call itself.
pub const ATOMIC_SAVEPOINT: &str = "#ATOMIC#";

fn not_found(name: &str) -> SqlError {
    SqlError::Coded {
        code: -880,
        state: "3B001",
        what: format!("savepoint {name} does not exist or is not valid here"),
    }
}

/// Run a savepoint statement.
pub(crate) fn run(txn: &mut Txn<'_>, ctx: &mut SqlContext, st: &SavepointStmt) -> Result<Outcome> {
    let def = if ctx.no_commit {
        None
    } else {
        ctx.session.commit_def().cloned()
    };
    let Some(def) = def else {
        return loose(ctx, st);
    };
    match st {
        SavepointStmt::Set { name, unique } if name.value.starts_with(ATOMIC_SAVEPOINT) => {
            def.borrow_mut().nest_savepoint(txn, &name.value, *unique)?;
        }
        SavepointStmt::Set { name, unique } => {
            if def.borrow_mut().set_savepoint(txn, &name.value, *unique)? == SetSavepoint::Exists {
                return Err(SqlError::Coded {
                    code: -881,
                    state: "3B501",
                    what: format!("savepoint {} already exists", name.value),
                });
            }
        }
        SavepointStmt::Release(name) => {
            if !def.borrow_mut().release_savepoint(&name.value) {
                return Err(not_found(&name.value));
            }
        }
        SavepointStmt::RollbackTo(name) => {
            let none = || match name {
                Some(n) => not_found(&n.value),
                None => SqlError::Coded {
                    code: -882,
                    state: "3B502",
                    what: "no savepoint is set".into(),
                },
            };
            let now = ctx.now;
            let mut session = std::mem::replace(
                &mut ctx.session,
                db::odp::Session::new(ctx.job, ctx.locks.clone()),
            );
            let r = session.rollback_to_savepoint(
                txn,
                name.as_ref().map(|n| n.value.as_str()),
                now,
                &mut |t| ctx.pause(t),
            );
            ctx.session = session;
            if r?.is_none() {
                return Err(none());
            }
        }
    }
    Ok(Outcome::Done)
}

/// A savepoint statement without commitment control.
fn loose(ctx: &mut SqlContext, st: &SavepointStmt) -> Result<Outcome> {
    let list = &mut ctx.loose_savepoints;
    let find = |list: &[(String, bool)], n: &str| list.iter().rposition(|(m, _)| m == n);
    match st {
        SavepointStmt::Set { name, unique } => {
            if let Some(i) =
                find(list, &name.value).filter(|_| !name.value.starts_with(ATOMIC_SAVEPOINT))
            {
                if *unique || list[i].1 {
                    return Err(SqlError::Coded {
                        code: -881,
                        state: "3B501",
                        what: format!("savepoint {} already exists", name.value),
                    });
                }
                list.remove(i);
            }
            list.push((name.value.clone(), *unique));
        }
        SavepointStmt::Release(name) => match find(list, &name.value) {
            Some(i) => list.truncate(i),
            None => return Err(not_found(&name.value)),
        },
        SavepointStmt::RollbackTo(name) => {
            let at = match name {
                Some(n) => find(list, &n.value).ok_or_else(|| not_found(&n.value))?,
                None => list.len().checked_sub(1).ok_or_else(|| SqlError::Coded {
                    code: -882,
                    state: "3B502",
                    what: "no savepoint is set".into(),
                })?,
            };
            list.truncate(at + 1);
        }
    }
    Ok(Outcome::Done)
}
