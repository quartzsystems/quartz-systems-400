// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Scalar functions: the datetime family.

use super::super::SqlContext;
use super::super::rtype::SqlType;
use crate::ast::Function;
use crate::error::Result;
use crate::value::Value;

/// Calls `name` when this family has it.
pub(crate) fn call(name: &str, args: &[Value], ctx: &SqlContext) -> Option<Result<Value>> {
    let _ = (name, args, ctx);
    None
}

/// The result type of `name` when this family has it.
pub(crate) fn result_type(name: &str, args: &[SqlType], f: &Function) -> Option<SqlType> {
    let _ = (name, args, f);
    None
}
