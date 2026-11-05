// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a column's definition says beyond its field: an identity's full
//! attributes, row change timestamps and auditing columns, kept as the
//! table's `COLUMN` entries (`db::sqlinf::seq`), and what they put in a
//! row as it is inserted or updated.

use db::sqlinf::seq::{self, ColumnKind, SeqAttrs, SqlColumn};
use dds::FieldDef;
use store::obj::ObjectId;
use store::txn::Txn;

use super::SqlContext;
use crate::ast::{self, CreateTable, DataType, Generated};
use crate::error::{Result, SqlError};
use crate::value::Value;

fn err(sqlcode: i32, sqlstate: &'static str, what: impl Into<String>) -> SqlError {
    SqlError::Integrity { sqlcode, sqlstate, what: what.into() }
}

/// The range a column of SQL type `t` holds, `None` for a type an
/// identity or sequence cannot have.
#[must_use]
pub fn type_range(t: &DataType) -> Option<(i128, i128)> {
    Some(match t {
        DataType::SmallInt => (i128::from(i16::MIN), i128::from(i16::MAX)),
        DataType::Integer => (i128::from(i32::MIN), i128::from(i32::MAX)),
        DataType::BigInt => (i128::from(i64::MIN), i128::from(i64::MAX)),
        DataType::Decimal(p, 0) | DataType::Numeric(p, 0) => {
            let top = 10i128.pow(u32::from(*p).min(38)) - 1;
            (-top, top)
        }
        _ => return None,
    })
}

/// The attributes `id` gives over a range, IBM's defaults filled in and
/// checked as IBM checks them (`SQL0815`-family codes: settled on the
/// primary, `oracle/integrity/`).
pub fn attrs(id: &ast::Identity, range: (i128, i128), what: &str) -> Result<SeqAttrs> {
    let increment = id.increment.unwrap_or(1);
    let up = increment > 0;
    let min = id.min.flatten().unwrap_or(if up { 1 } else { range.0 });
    let max = id.max.flatten().unwrap_or(if up { range.1 } else { -1 });
    let start = id.start.unwrap_or(if up { min } else { max });
    for v in [min, max, start] {
        if v < range.0 || v > range.1 {
            return Err(err(-846, "42815", format!("value {v} is outside the range of {what}")));
        }
    }
    if min > max {
        return Err(err(-846, "42815", format!("MINVALUE must be less than MAXVALUE for {what}")));
    }
    if start < min || start > max {
        return Err(err(-846, "42815", format!("START WITH {start} is outside MINVALUE and MAXVALUE of {what}")));
    }
    // The counter is eight bytes: a DECIMAL identity past them waits.
    if i64::try_from(min).is_err() || i64::try_from(max).is_err() {
        return Err(SqlError::Unsupported {
            what: format!("an identity or sequence range past 19 digits ({what}); BLOCKED(sql): an 8-byte counter"),
        });
    }
    if id.order == Some(true) {
        // One storage service gives values in order anyway.
    }
    Ok(SeqAttrs {
        start,
        increment,
        min,
        max,
        cycle: id.cycle.unwrap_or(false),
        cache: id.cache.unwrap_or(20),
        order: id.order.unwrap_or(false),
        always: id.always,
    })
}

/// Refuse what this release does not do with a generated column.
pub fn check_generated(c: &ast::ColumnDef) -> Result<()> {
    if c.hidden {
        return Err(SqlError::Unsupported {
            what: "IMPLICITLY HIDDEN columns; BLOCKED(sql): queries have no implicitly hidden column yet".into(),
        });
    }
    match &c.generated {
        // IBM generates identities, row change timestamps and auditing
        // columns only: an expression is a syntax error there.
        Some(Generated::Expr(_)) => Err(err(-104, "42601", "GENERATED ALWAYS AS ( expects IDENTITY or an auditing value")),
        Some(Generated::RowChangeTimestamp { .. }) if !matches!(c.type_spec.data_type, DataType::Timestamp(_)) => {
            Err(err(-683, "42842", format!("column {} must be a TIMESTAMP to be a row change timestamp", c.name.value)))
        }
        Some(Generated::Audit { what, .. }) if what == "DATA CHANGE OPERATION" && !matches!(c.type_spec.data_type, DataType::Char(1)) => {
            Err(err(-574, "42894", format!("column {} must be CHAR(1) to record the data change operation", c.name.value)))
        }
        _ if c.generated.is_some() && c.identity.is_some() => {
            Err(err(-574, "42894", format!("column {} cannot be generated twice", c.name.value)))
        }
        _ if c.generated.is_some() && c.default.is_some() => {
            Err(err(-574, "42894", format!("a generated column {} cannot have a default", c.name.value)))
        }
        _ => Ok(()),
    }
}

/// The `COLUMN` entry a column definition makes, `None` when it makes
/// none.
pub fn entry_of(c: &ast::ColumnDef, f: &FieldDef) -> Result<Option<SqlColumn>> {
    let kind = if let Some(id) = &c.identity {
        let range = type_range(&c.type_spec.data_type).ok_or_else(|| {
            err(-846, "42815", format!("identity column {} must be an integer or DECIMAL(n, 0)", c.name.value))
        })?;
        ColumnKind::Identity(attrs(id, range, &format!("identity column {}", c.name.value))?)
    } else {
        match &c.generated {
            Some(Generated::RowChangeTimestamp { always }) => ColumnKind::RowChange(*always),
            Some(Generated::Audit { always, what }) => ColumnKind::Audit(*always, what.clone()),
            Some(Generated::Expr(_)) | None => return Ok(None),
        }
    };
    Ok(Some(SqlColumn { field: f.name, kind, hidden: c.hidden }))
}

/// `CREATE TABLE`: the `COLUMN` entries of `t`'s columns.
pub fn create(txn: &mut Txn<'_>, ctx: &SqlContext, file: ObjectId, t: &CreateTable) -> Result<()> {
    let desc = db::file::describe(txn, file)?;
    let mut entries = Vec::new();
    for (c, f) in t.columns.iter().zip(&desc.formats[0].format.fields) {
        check_generated(c)?;
        if let Some(e) = entry_of(c, f)? {
            entries.push(e);
        }
    }
    if entries.iter().filter(|e| matches!(e.kind, ColumnKind::Identity(_))).count() > 1 {
        return Err(err(-372, "428C1", "a table has one identity column at most"));
    }
    if entries.iter().filter(|e| matches!(e.kind, ColumnKind::RowChange(_))).count() > 1 {
        return Err(err(-372, "428C1", "a table has one row change timestamp column at most"));
    }
    if entries.is_empty() {
        return Ok(());
    }
    seq::store_columns(txn, file, &entries, ctx.clock())?;
    // The counter starts where the attributes say.
    if let Some(a) = entries.iter().find_map(|e| match e.kind {
        ColumnKind::Identity(a) => Some(a),
        _ => None,
    }) {
        set_counter(txn, file, a.start)?;
    }
    Ok(())
}

/// Set every member's identity counter to `next`.
pub fn set_counter(txn: &mut Txn<'_>, file: ObjectId, next: i128) -> Result<()> {
    let desc = db::file::describe(txn, file)?;
    for e in &desc.members {
        let mut core = db::member::Core::load(txn, e.object)?;
        core.attr.identity_next = i64::try_from(next).unwrap_or(i64::MAX);
        let (page, _) = store::obj::ops::read_header(txn, e.object)?;
        store::obj::ops::write_attr(txn, page, 0, &core.attr.encode())?;
    }
    Ok(())
}

/// What the table's generated columns are, for a change.
#[derive(Clone, Debug, Default)]
pub struct Generation {
    /// `(field index, entry)`.
    pub cols: Vec<(usize, SqlColumn)>,
}

impl Generation {
    /// The generated columns of a table.
    pub fn of(txn: &mut Txn<'_>, t: &super::TableInfo) -> Result<Self> {
        if t.desc.header.sql.is_none() {
            return Ok(Self::default());
        }
        let mut cols = Vec::new();
        for c in seq::columns(txn, t.id)? {
            if let Some((i, _)) = t.format.format.field(&c.field) {
                cols.push((i, c));
            }
        }
        Ok(Self { cols })
    }

    /// Fill the generated columns of a row inserted (`insert`) or
    /// updated: a given value for an `ALWAYS` column is refused
    /// (`SQL0798`) unless `overriding_system`.
    pub fn fill(
        &self,
        ctx: &SqlContext,
        fields: &[FieldDef],
        per_field: &mut [Option<Value>],
        insert: bool,
        overriding_system: bool,
    ) -> Result<()> {
        for (i, c) in &self.cols {
            let (always, value) = match &c.kind {
                ColumnKind::RowChange(always) => (*always, {
                    let (secs, picos) = ctx.clock_picos();
                    let (d, t) = (db::record::datetime::Date::from_unix(secs), db::record::datetime::Time::from_unix(secs));
                    Value::Timestamp(crate::value::Timestamp::new(d, t, picos))
                }),
                ColumnKind::Audit(always, what) => {
                    let v = match what.as_str() {
                        "DATA CHANGE OPERATION" => Value::Str(if insert { "I" } else { "U" }.into()),
                        "CURRENT_SERVER" | "CURRENT SERVER" => Value::Str("QS400".into()),
                        _ => Value::Str(ctx.user.clone()),
                    };
                    (*always, v)
                }
                ColumnKind::Identity(_) | ColumnKind::Plain | ColumnKind::Expr(_) => continue,
            };
            if per_field[*i].is_some() && always && !overriding_system {
                return Err(SqlError::IdentityValue { column: fields[*i].name.to_string() });
            }
            if per_field[*i].is_none() || always {
                per_field[*i] = Some(value);
            }
        }
        Ok(())
    }
}
