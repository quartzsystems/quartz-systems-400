// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The plan cache's procedures (`plan::cache`): `DUMP_PLAN_CACHE`
//! writes its entries to a database monitor file, `CLEAR_PLAN_CACHE`
//! empties it, `CHANGE_PLAN_CACHE_SIZE` bounds it. Each needs `*JOBCTL`
//! special authority or the `QIBM_DB_SQLADM` function usage
//! (`sec::fcnusg`), as on IBM i (CPF43A3 / SQLSTATE `38501`
//! otherwise).
//!
//! The arguments that pick entries (`PLAN_IDENTIFIER`, `QRO_HASH`,
//! `SQL_STATEMENT_TEXT_FILTER`, `IASP_NAME`, `INCLUDE_SYSTEM_QUERIES`)
//! are refused SQL0084 at any value but null.

use store::txn::Txn;

use super::super::super::SqlContext;
use super::super::super::plan::cache;
use super::Args;
use crate::error::{Result, SqlError};
use crate::value::Value;

/// `*JOBCTL` or `QIBM_DB_SQLADM`, or a store without profiles (tools,
/// tests).
fn authorized(txn: &mut Txn<'_>, ctx: &SqlContext) -> Result<()> {
    let user = ctx.user.to_ascii_uppercase();
    let Ok(p) = sec::auth::principal::Principal::load(txn, ctx.qsys, &user, 0) else {
        return Ok(());
    };
    if p.has(sec::profile::spcaut::SpecialAuth::JOBCTL)
        || sec::fcnusg::check(txn, ctx.qsys, &p, "QIBM_DB_SQLADM").unwrap_or(false)
    {
        return Ok(());
    }
    Err(SqlError::Coded { code: -443, state: "38501", what: "CPF43A3 *JOBCTL special authority or the QIBM_DB_SQLADM function usage is needed to use the plan cache.".into() })
}

fn refuse_filters(args: &Args, names: &[&str]) -> Result<()> {
    for n in names {
        if args.text(n).is_some() {
            return Err(SqlError::Coded {
                code: -84,
                state: "42612",
                what: format!("{n} on a plan cache procedure is not supported"),
            });
        }
    }
    Ok(())
}

/// `DUMP_PLAN_CACHE(FILESCHEMA, FILENAME, ...)`.
pub fn dump(txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args) -> Result<Vec<Value>> {
    authorized(txn, ctx)?;
    refuse_filters(
        args,
        &[
            "PLAN_IDENTIFIER",
            "SQL_STATEMENT_TEXT_FILTER",
            "INCLUDE_SYSTEM_QUERIES",
            "IASP_NAME",
            "QRO_HASH",
        ],
    )?;
    let lib = args.word("FILESCHEMA").unwrap_or_default();
    let file = args.word("FILENAME").unwrap_or_default();
    if lib.is_empty() || file.is_empty() {
        return Err(SqlError::Coded {
            code: -443,
            state: "38501",
            what: "FILESCHEMA and FILENAME are needed".into(),
        });
    }
    let records: Vec<crate::dbmon::Record> = cache::entries()
        .iter()
        .map(|e| {
            let at = |secs: u64| {
                crate::value::Timestamp::new(
                    db::record::datetime::Date::from_unix(secs),
                    db::record::datetime::Time::from_unix(secs),
                    0,
                )
            };
            crate::dbmon::record::cached(
                ctx,
                &e.text,
                e.id,
                (at(e.first), at(e.last)),
                e.naming == crate::ast::Naming::Sql,
            )
        })
        .collect();
    crate::dbmon::dump(txn, ctx, &lib, &file, &records).map_err(|e| SqlError::Coded {
        code: -443,
        state: "38501",
        what: e.to_string(),
    })?;
    Ok(Vec::new())
}

/// `CLEAR_PLAN_CACHE()`.
pub fn clear(txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args) -> Result<Vec<Value>> {
    authorized(txn, ctx)?;
    refuse_filters(args, &["QRO_HASH", "PLAN_IDENTIFIER"])?;
    cache::clear();
    Ok(Vec::new())
}

/// `CHANGE_PLAN_CACHE_SIZE(SIZE_IN_MEG)`.
pub fn change_size(txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args) -> Result<Vec<Value>> {
    authorized(txn, ctx)?;
    let mb = args
        .int("SIZE_IN_MEG")
        .filter(|n| *n >= 0)
        .ok_or_else(|| SqlError::Coded {
            code: -443,
            state: "38501",
            what: "SIZE_IN_MEG must be 0 or more".into(),
        })?;
    // 0 restores the size the system starts with.
    cache::resize(if mb == 0 {
        cache::DEFAULT_MB
    } else {
        u64::try_from(mb).unwrap_or(u64::MAX)
    });
    Ok(Vec::new())
}
