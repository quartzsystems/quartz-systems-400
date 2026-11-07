// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The output file: made with `QAQQDBMN`'s columns and record format
//! `QQQDBMN`, or checked to have them; records written with `INSERT`,
//! outside the job's commitment control.

use qsname::Name;
use store::txn::Txn;

use crate::ast::{Naming, ObjectName};
use crate::exec::SqlContext;
use crate::value::Value;

use super::columns::{BINARY, COLUMNS, NOT_NULL};
use super::{MonError, Record, StartSpec, err, take_due};

/// The record format every monitor file has.
pub const FORMAT: &str = "QQQDBMN";

/// `CREATE TABLE` text for a monitor file.
#[must_use]
pub fn create_text(lib: Option<&str>, file: &str) -> String {
    let cols: Vec<String> = COLUMNS
        .iter()
        .enumerate()
        .map(|(i, (n, t))| {
            let mut c = format!("{n} {}", t.sql());
            if *n == BINARY {
                c.push_str(" FOR BIT DATA");
            }
            if i < NOT_NULL {
                c.push_str(" NOT NULL");
            }
            c
        })
        .collect();
    let name = lib.map_or_else(|| file.to_string(), |l| format!("{l}/{file}"));
    format!("CREATE TABLE {name} ({}) RCDFMT {FORMAT}", cols.join(", "))
}

fn name_of(lib: &str, file: &str) -> ObjectName {
    match lib {
        "" | "*LIBL" | "*CURLIB" => ObjectName::new(file),
        l => ObjectName::qualified(l, file),
    }
}

/// Run `text` as the job, without commitment control, `*SYS` naming.
fn run(txn: &mut Txn<'_>, ctx: &mut SqlContext, text: &str, params: &[Value]) -> crate::Result<()> {
    let (naming, nc, warning) = (ctx.naming, ctx.no_commit, ctx.warning.take());
    ctx.no_commit = true;
    let r = crate::exec::prepare(text, Naming::Sys)
        .and_then(|p| p.execute(txn, ctx, params))
        .map(|_| ());
    ctx.naming = naming;
    ctx.no_commit = nc;
    ctx.warning = warning;
    r
}

/// Make or check the monitor file of `spec`, its member ready. The
/// library it is in.
pub(super) fn prepare_file(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    spec: &StartSpec,
) -> Result<String, MonError> {
    let name = name_of(&spec.lib, &spec.file);
    let saved = ctx.naming;
    ctx.naming = Naming::Sys;
    if name.schema.is_some() && ctx.library_for(txn, &name).is_err() {
        ctx.naming = saved;
        return Err(err("CPF3064", format!("Library {} not found.", spec.lib)));
    }
    let found = ctx.resolve_table(txn, &name);
    ctx.naming = saved;
    let member = (spec.member != "*FIRST" && !spec.member.is_empty()).then(|| spec.member.clone());
    #[allow(clippy::single_match_else)] // the file found, or made
    let t = match found {
        Ok(t) => {
            let ok = t.format.format.name.to_string() == FORMAT
                && t.format.format.fields.len() == COLUMNS.len();
            if !ok {
                return Err(err(
                    "CPF3061",
                    format!(
                        "Record format {FORMAT} not found for outfile {}.",
                        spec.file
                    ),
                ));
            }
            if let Some(m) = &member
                && t.desc
                    .members
                    .first()
                    .is_some_and(|e| e.name.to_string() != *m)
            {
                // BLOCKED(rel): records are written with INSERT, which
                // reaches the file's first member only.
                return Err(err(
                    "CPF9898",
                    format!(
                        "Member {m} is not the first member of {}: records go to the first member.",
                        spec.file
                    ),
                ));
            }
            if !spec.add {
                db::util::clrpfm(txn, ctx.job, t.id, qsname::MemberSpec::First)
                    .map_err(|e| err("CPF9898", e.to_string()))?;
            }
            t
        }
        Err(_) => {
            let lib = match spec.lib.as_str() {
                "" | "*LIBL" | "*CURLIB" => None,
                l => Some(l),
            };
            run(txn, ctx, &create_text(lib, &spec.file), &[]).map_err(|e| {
                err(
                    "CPF9898",
                    format!("The monitor file {} was not made: {e}", spec.file),
                )
            })?;
            let saved = ctx.naming;
            ctx.naming = Naming::Sys;
            let t = ctx.resolve_table(txn, &name);
            ctx.naming = saved;
            let t = t.map_err(|e| err("CPF9898", e.to_string()))?;
            if let Some(m) = &member
                && *m != spec.file
            {
                let to = Name::parse(m).map_err(|e| err("CPD0078", format!("OUTMBR({m}): {e}")))?;
                db::file::rename_member(txn, t.id, t.member.name, to, ctx.clock())
                    .map_err(|e| err("CPF9898", e.to_string()))?;
            }
            t
        }
    };
    Ok(t.desc.library_name.to_string())
}

/// Write `records` to `lib/file`.
pub(super) fn write(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    lib: &str,
    file: &str,
    records: &[Record],
) -> Result<(), MonError> {
    for r in records {
        let cols: Vec<&str> = r.0.iter().map(|(c, _)| *c).collect();
        let marks = vec!["?"; cols.len()].join(", ");
        let text = format!(
            "INSERT INTO {lib}/{file} ({}) VALUES ({marks})",
            cols.join(", ")
        );
        let vals: Vec<Value> = r.0.iter().map(|(_, v)| v.clone()).collect();
        run(txn, ctx, &text, &vals).map_err(|e| {
            err(
                "CPF9898",
                format!("A monitor record was not written to {lib}/{file}: {e}"),
            )
        })?;
    }
    Ok(())
}

/// Write what the monitors hold: those holding their `FRCRCD` count,
/// or all of them with `force`.
pub fn flush(txn: &mut Txn<'_>, ctx: &mut SqlContext, force: bool) -> Result<(), MonError> {
    for (lib, file, recs) in take_due(force) {
        write(txn, ctx, &lib, &file, &recs)?;
    }
    Ok(())
}
