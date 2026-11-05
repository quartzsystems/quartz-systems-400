// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `IDENTITY_VAL_LOCAL()`: the identity value the job's last one-row
//! `INSERT` gave.

use super::super::SqlContext;
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The family's functions.
pub(crate) fn call(name: &str, args: &[Value], ctx: &SqlContext) -> Option<Result<Value>> {
    if name != "IDENTITY_VAL_LOCAL" {
        return None;
    }
    if !args.is_empty() {
        return Some(Err(SqlError::Integrity {
            sqlcode: -170,
            sqlstate: "42605",
            what: "IDENTITY_VAL_LOCAL takes no arguments".into(),
        }));
    }
    // DECIMAL(31, 0) as IBM describes it; null before any insert.
    Some(Ok(match &ctx.seq.identity {
        Some(Value::Num(d)) => Value::Num(d.clone()),
        Some(v) => v.as_decimal().map_or(Value::Null, Value::Num),
        None => Value::Null,
    }))
}
