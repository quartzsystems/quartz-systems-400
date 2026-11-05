// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.GENERATE_SQL` and `GENERATE_SQL_OBJECTS`: the SQL that makes
//! an object again, read from the catalog, laid out as pub400 lays it
//! out (`tools/conformance/sql/expected/services/GENSQL.out`): one
//! logical line per clause, a tab before each continuation, a blank
//! and a line feed after each line when formatted; records of 80
//! characters, a line too long for one carried on in the next after a
//! tab. The lines go to a source member and come back as a result set.

mod format;
mod objects;
mod output;

use store::txn::Txn;

use super::super::super::SqlContext;
use super::Args;
use crate::error::{Result, SqlError};
use crate::value::Value;

pub use format::Options;

/// The object types and the generator of each.
const TYPES: &[&str] = &["TABLE", "VIEW", "INDEX", "SEQUENCE", "ALIAS"];

/// The types IBM generates that QS/400 does not yet.
const NOT_YET: &[&str] =
    &["FUNCTION", "PROCEDURE", "TRIGGER", "TYPE", "VARIABLE", "CONSTRAINT", "SCHEMA", "MASK", "PERMISSION", "MQT", "XSR"];

fn invalid(what: String) -> SqlError {
    SqlError::Coded { code: -443, state: "22023", what }
}

/// `GENERATE_SQL(DATABASE_OBJECT_NAME, DATABASE_OBJECT_LIBRARY_NAME,
/// DATABASE_OBJECT_TYPE, ...)`.
pub fn generate_sql(txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args) -> Result<Vec<Value>> {
    let opts = Options::of(args)?;
    let name = args.text("DATABASE_OBJECT_NAME").unwrap_or_default();
    let lib = args.text("DATABASE_OBJECT_LIBRARY_NAME").unwrap_or_default();
    let ty = args.word("DATABASE_OBJECT_TYPE").unwrap_or_default();
    let mut objects = Vec::new();
    find(txn, ctx, &name, &lib, &ty, &mut objects)?;
    finish(txn, ctx, args, &opts, &objects)
}

/// `GENERATE_SQL_OBJECTS(SYSTEM_TABLE_NAME, SYSTEM_TABLE_SCHEMA, ...)`:
/// the objects a table lists (`OBJECT_SCHEMA`, `OBJECT_NAME`,
/// `SQL_OBJECT_TYPE`), in its order.
pub fn generate_sql_objects(txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args) -> Result<Vec<Value>> {
    let opts = Options::of(args)?;
    let table = args.word("SYSTEM_TABLE_NAME").unwrap_or_default();
    let schema = args.word("SYSTEM_TABLE_SCHEMA").unwrap_or_else(|| "QTEMP".into());
    let list = objects::query(
        txn,
        ctx,
        &format!("SELECT OBJECT_SCHEMA, OBJECT_NAME, SQL_OBJECT_TYPE FROM \"{schema}\".\"{table}\""),
        &[],
    )?;
    let mut found = Vec::new();
    for r in list {
        let t = |i: usize| r.get(i).map(|v| v.as_text().trim_end().to_string()).unwrap_or_default();
        find(txn, ctx, &t(1), &t(0), &t(2).to_ascii_uppercase(), &mut found)?;
    }
    finish(txn, ctx, args, &opts, &found)
}

/// The objects `name` (a name or a `LIKE` pattern) of type `ty` in
/// library `lib`.
fn find(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &str, lib: &str, ty: &str, out: &mut Vec<objects::Object>) -> Result<()> {
    if NOT_YET.contains(&ty) {
        // BLOCKED(sql): routines, triggers, types, variables, schemas,
        // constraints alone, masks and permissions are not generated
        // yet.
        return Err(SqlError::Coded { code: -84, state: "42612", what: format!("GENERATE_SQL of a {ty} is not supported yet") });
    }
    if !TYPES.contains(&ty) {
        return Err(invalid(format!("{ty} is not a database object type")));
    }
    out.extend(objects::find(txn, ctx, name.trim(), lib.trim(), ty)?);
    Ok(())
}

/// Lay out the objects' SQL, keep it, answer it.
fn finish(txn: &mut Txn<'_>, ctx: &mut SqlContext, args: &Args, opts: &Options, objects: &[objects::Object]) -> Result<Vec<Value>> {
    if objects.is_empty() {
        // As pub400: no object is SQL0462, a warning, and no result.
        ctx.warning = Some(crate::sqlca::Warning::coded(462, "01532", "no object was found to generate SQL for"));
        return Ok(Vec::new());
    }
    let mut lines = Vec::new();
    if opts.header {
        lines.extend(format::header(ctx));
    }
    for (i, o) in objects.iter().enumerate() {
        if i > 0 {
            lines.push(format::Line::blank());
        }
        lines.extend(objects::generate(txn, ctx, o, opts)?);
    }
    let records = format::records(&lines, opts.formatted);
    output::keep(txn, ctx, args, &records)?;
    output::answer(ctx, &records);
    Ok(Vec::new())
}
