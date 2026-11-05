// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Services over what QS/400 does not have, answering with no rows:
//! function usage registration (`FUNCTION_INFO`, `FUNCTION_USAGE`),
//! license keys (`LICENSE_INFO`), limit tracking (`SYSLIMTBL`,
//! `SYSLIMITS`) and the Electronic Service Agent
//! (`ELECTRONIC_SERVICE_AGENT_INFO`).

use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Args, Out};
use crate::error::Result;

/// A view with no rows.
pub fn view(_txn: &mut Txn<'_>, _ctx: &SqlContext, _out: &mut Out) -> Result<()> {
    Ok(())
}

/// A table function with no rows.
pub fn function(_txn: &mut Txn<'_>, _ctx: &SqlContext, _args: &Args, _out: &mut Out) -> Result<()> {
    Ok(())
}
