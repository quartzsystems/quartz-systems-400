// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*READ` triggers of a query's tables, as the primary fires them
//! (`tools/conformance/sql/oracle/trigger/p7.out`): once for each row a
//! table's scan keeps, its own selection applied (a row the scan's
//! predicates reject fires nothing; one a join or a later filter drops
//! does), a view's or index's rows as their table's, and none for the
//! reads of an `UPDATE` or `DELETE`. A scan holds its members open and
//! cannot stop in a row to call a program, so the calls are made when the
//! scan is over, in the order the rows were read, each record read again
//! by its number; a call suspends the statement as a trigger of a change
//! does (`routine::midcall`).
//! BLOCKED(sql): the primary reads a join's inner table by the join's
//! condition, so its rows fire only as they match; here the inner scan
//! keeps what its own predicates keep and each of those fires once. A
//! scalar `VALUES (subquery)` over such a table is `SQL0255` there and
//! runs here.

use db::file::{self, FileKind};
use db::member::Member;
use db::trg::{Event, Time};
use store::obj::ops;
use store::txn::Txn;

use super::eval::Row;
use super::{SqlContext, TableInfo};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// `SQL0084`: no trigger program can be called from here.
fn not_callable(t: &TableInfo, why: &str) -> SqlError {
    SqlError::Unsupported { what: format!("a query of {}: it has read triggers, and {why}", t.desc.name()) }
}

/// Whether a query here may read `t`, whose reads fire triggers.
pub fn callable(ctx: &SqlContext, t: &TableInfo) -> Result<()> {
    if ctx.trigger_caller().is_none() && !quiet(t.id) {
        // BLOCKED(sql): a context with no job (the service console, the
        // kit) has nothing to call a program with.
        return Err(not_callable(t, "no trigger program can be called from this statement"));
    }
    Ok(())
}

/// The physical member whose records `t`'s rows are: its own, or a
/// logical file's one based-on member.
fn physical(txn: &mut Txn<'_>, t: &TableInfo) -> Result<Member> {
    let m = Member::open(txn, &t.desc, &t.member)?;
    if t.desc.attr.kind != FileKind::Logical {
        return Ok(m);
    }
    let [base] = m.bases.as_slice() else {
        // BLOCKED(sql): a join or multi-format logical file's rows are
        // not one member's records.
        return Err(not_callable(t, "it is a logical file over more than one member"));
    };
    let file_id = ops::read_header(txn, base.member)?.1.library;
    let desc = file::describe(txn, file_id)?;
    let md = *desc.members.iter().find(|d| d.object == base.member).ok_or_else(|| SqlError::ObjectNotFound {
        name: desc.name().to_string(),
        kind: "member",
    })?;
    Ok(Member::open(txn, &desc, &md)?)
}

/// The record number a scanned row carries first.
fn rrn_of(r: &Row) -> Option<u64> {
    match r.first()? {
        Value::Num(d) => d.to_string().parse().ok(),
        _ => None,
    }
}

/// Call `t`'s read triggers for `rows`, the rows its scan kept.
pub fn fire(txn: &mut Txn<'_>, ctx: &mut SqlContext, t: &TableInfo, rows: &[Row]) -> Result<()> {
    if quiet(t.id) {
        return Ok(());
    }
    let Some(caller) = ctx.trigger_caller() else {
        return Err(not_callable(t, "no trigger program can be called from this statement"));
    };
    let mut m = physical(txn, t)?;
    let Some(active) = m.trgs.clone() else { return Ok(()) };
    let lvl = db::odp::trigger::level_digit(ctx.session.commit_def().map(|c| c.borrow().lcklvl()));
    for r in rows {
        let Some(rrn) = rrn_of(r) else { continue };
        let Some(slot) = m.read(txn, rrn)? else { continue };
        if slot.deleted {
            continue;
        }
        let row = db::trg::fire::Row { event: Event::Read, rrn, old: Some((&slot.record, &slot.null_map)), new: None };
        db::trg::fire::call_all(txn, &caller, &active, &m, Time::After, &row, lvl)?;
        // The program's own requests may have changed the member.
        m.refresh(txn)?;
    }
    Ok(())
}

thread_local! {
    /// The files an `UPDATE` or `DELETE` is reading to change, whose reads
    /// fire no read triggers.
    static CHANGING: std::cell::RefCell<Vec<store::obj::ObjectId>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// While the guard lives, reads of `file` fire nothing: the reads of a
/// change are no reads (the primary).
pub fn changing(file: store::obj::ObjectId) -> Changing {
    CHANGING.with(|c| c.borrow_mut().push(file));
    Changing
}

/// The guard [`changing`] gives.
pub struct Changing;

impl Drop for Changing {
    fn drop(&mut self) {
        CHANGING.with(|c| {
            c.borrow_mut().pop();
        });
    }
}

fn quiet(file: store::obj::ObjectId) -> bool {
    CHANGING.with(|c| c.borrow().contains(&file))
}
