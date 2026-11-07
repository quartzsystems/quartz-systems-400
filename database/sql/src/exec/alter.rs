// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ALTER TABLE`. Its actions run in IBM's order: constraints dropped
//! first, then the record format changed (the records copied into a new
//! member beside the old one, `db::chg`), then constraints added, so a
//! constraint over a column the statement adds can be added with it.

use db::cst::Kind;
use db::file::{self, FileKind};
use db::sqlinf::seq::{self, ColumnKind};
use qsname::Name;
use store::txn::Txn;

use super::ddl::{default_def, field_of};
use super::{SqlContext, TableInfo};
use crate::ast::{AlterAction, AlterTable, ColumnDef, DataType, DropKind, Ident, Identity};
use crate::error::{Result, SqlError};

fn err(sqlcode: i32, sqlstate: &'static str, what: impl Into<String>) -> SqlError {
    SqlError::Integrity {
        sqlcode,
        sqlstate,
        what: what.into(),
    }
}

/// `ALTER TABLE`.
#[allow(clippy::too_many_lines)]
pub fn alter_table(txn: &mut Txn<'_>, ctx: &mut SqlContext, stmt: &AlterTable) -> Result<()> {
    let table = ctx.resolve_table(txn, &stmt.name)?;
    if table.desc.attr.kind != FileKind::Physical {
        return Err(err(-7008, "55019", format!("{} is not a table", stmt.name)));
    }
    let id = table.id;
    // The ALTER privilege, before anything changes.
    db::auth::check(txn, "file.change", id)?;
    // 1. Constraints dropped, by the statement or with the columns it drops.
    for act in &stmt.actions {
        match act {
            AlterAction::DropPrimaryKey { cascade } => {
                super::cst::drop(txn, ctx, id, None, DropKind::Unique, *cascade)?;
            }
            AlterAction::DropConstraint {
                name,
                kind,
                cascade,
            } => super::cst::drop(txn, ctx, id, Some(name), *kind, *cascade)?,
            AlterAction::DropColumn { name, cascade } => {
                let col = super::cst::field_name(&table.format.format, name)?;
                let using = db::cst::ops::using_column(txn, id, col)?;
                if !using.is_empty() && !cascade {
                    return Err(err(
                        -478,
                        "42893",
                        format!("column {} is used by constraint {}", name.value, using[0]),
                    ));
                }
                for n in using {
                    super::cst::drop(
                        txn,
                        ctx,
                        id,
                        Some(&Ident {
                            value: n,
                            delimited: true,
                        }),
                        DropKind::Any,
                        true,
                    )?;
                }
            }
            _ => {}
        }
    }
    // 2. The record format.
    let mut rf = table.format.format.clone();
    let mut cols = seq::columns(txn, id)?;
    let mut touched = false;
    let mut restart: Option<i128> = None;
    // An identity column added: the rows there are numbered, in RRN order.
    let mut number: Option<Name> = None;
    let find = |rf: &dds::RecordFormat, name: &Ident| -> Result<usize> {
        let n = super::cst::field_name(rf, name)?;
        Ok(rf.field(&n).expect("found").0)
    };
    for act in &stmt.actions {
        match act {
            AlterAction::AddColumn(c) => {
                super::column::check_generated(c)?;
                let f = field_of(c)?;
                if rf.field(&f.name).is_some()
                    || rf
                        .fields
                        .iter()
                        .any(|x| x.alias.as_deref() == Some(c.name.value.as_str()))
                {
                    return Err(err(
                        -612,
                        "42711",
                        format!("column {} is already in table {}", c.name.value, stmt.name),
                    ));
                }
                if let Some(e) = super::column::entry_of(c, &f)? {
                    if matches!(e.kind, ColumnKind::Identity(_)) {
                        if cols
                            .iter()
                            .any(|x| matches!(x.kind, ColumnKind::Identity(_)))
                            || rf.fields.iter().any(|x| x.identity.is_some())
                        {
                            return Err(err(
                                -372,
                                "428C1",
                                "a table has one identity column at most",
                            ));
                        }
                        number = Some(f.name);
                        if let ColumnKind::Identity(a) = e.kind {
                            restart = Some(a.start);
                        }
                    }
                    cols.push(e);
                }
                rf.fields.push(f);
                touched = true;
            }
            AlterAction::DropColumn { name, .. } => {
                let i = find(&rf, name)?;
                if rf.fields.len() == 1 {
                    return Err(err(
                        -195,
                        "42814",
                        format!("column {} is the table's last", name.value),
                    ));
                }
                let gone = rf.fields.remove(i);
                cols.retain(|c| c.field != gone.name);
                touched = true;
            }
            AlterAction::SetDataType(name, ts) => {
                let i = find(&rf, name)?;
                let old = rf.fields[i].clone();
                if !db::cst::ops::using_column(txn, id, old.name)?.is_empty()
                    && is_key_column(txn, id, old.name)?
                {
                    return Err(SqlError::Unsupported {
                        what: format!(
                            "changing the type of {}, a key of a constraint; drop the constraint first",
                            name.value
                        ),
                    });
                }
                let c = ColumnDef {
                    name: name.clone(),
                    system_name: None,
                    type_spec: ts.clone(),
                    not_null: !old.allow_null,
                    default: None,
                    identity: None,
                    generated: None,
                    hidden: false,
                    constraints: Vec::new(),
                };
                let mut f = field_of(&c)?;
                if old.identity.is_some() && super::column::type_range(&ts.data_type).is_none() {
                    return Err(err(
                        -846,
                        "42815",
                        format!(
                            "identity column {} must stay an integer or DECIMAL(n, 0)",
                            name.value
                        ),
                    ));
                }
                f.name = old.name;
                f.text = old.text;
                f.alias = old.alias;
                f.colhdg = old.colhdg;
                f.default = old.default;
                f.identity = old.identity;
                rf.fields[i] = f;
                touched = true;
            }
            AlterAction::SetNotNull(name, yes) => {
                let i = find(&rf, name)?;
                if !yes
                    && db::cst::list(txn, id)?.iter().any(|c| {
                        c.kind == Kind::PrimaryKey && c.columns.contains(&rf.fields[i].name)
                    })
                {
                    return Err(err(
                        -542,
                        "42831",
                        format!("column {} is in the primary key", name.value),
                    ));
                }
                rf.fields[i].allow_null = !yes;
                touched = true;
            }
            AlterAction::SetDefault(name, d) => {
                let i = find(&rf, name)?;
                if cols
                    .iter()
                    .any(|c| c.field == rf.fields[i].name && !matches!(c.kind, ColumnKind::Plain))
                    || rf.fields[i].identity.is_some()
                {
                    return Err(err(
                        -574,
                        "42894",
                        format!("column {} is generated: it has no default", name.value),
                    ));
                }
                let data_type = rf.fields[i].data_type;
                rf.fields[i].default = match d {
                    Some(d) => default_def(d, &data_type_of(&rf.fields[i], data_type))?,
                    None => None,
                };
                touched = true;
            }
            AlterAction::AlterIdentity {
                column,
                always,
                options,
            } => {
                let i = find(&rf, column)?;
                let f = &mut rf.fields[i];
                let Some(ident) = f.identity.as_mut() else {
                    return Err(err(
                        -190,
                        "42837",
                        format!("column {} is not an identity column", column.value),
                    ));
                };
                if let Some(a) = always {
                    ident.always = *a;
                }
                let range = seq::field_range(f);
                let fname = f.name;
                let at = cols
                    .iter()
                    .position(|c| c.field == fname && matches!(c.kind, ColumnKind::Identity(_)));
                let base = if let Some(ColumnKind::Identity(a)) = at.map(|k| &cols[k].kind) {
                    *a
                } else {
                    let def = f.identity.clone().expect("identity");
                    super::column::attrs(
                        &Identity {
                            start: Some(def.start.into()),
                            increment: Some(def.increment.into()),
                            cycle: Some(def.cycle),
                            ..Identity::default()
                        },
                        range,
                        &column.value,
                    )?
                };
                let mut attrs = super::seq::merge(
                    &base,
                    options,
                    range,
                    &format!("identity column {}", column.value),
                )?;
                attrs.always = f.identity.as_ref().is_some_and(|x| x.always);
                let block = at.and_then(|k| cols[k].block_end);
                if let Some(r) = options.restart {
                    restart = Some(r.unwrap_or(attrs.start));
                } else if let Some(be) = block {
                    // Values IBM had cached are passed over.
                    restart = attrs.after(be).or(Some(be));
                }
                let entry = seq::SqlColumn {
                    field: fname,
                    kind: ColumnKind::Identity(attrs),
                    hidden: false,
                    block_end: None,
                };
                match at {
                    Some(k) => cols[k] = entry,
                    None => cols.push(entry),
                }
                // The field keeps what its four bytes hold.
                let kept = f.identity.as_mut().expect("identity");
                kept.increment = i64::try_from(attrs.increment).unwrap_or(1);
                kept.start = i64::try_from(attrs.start).unwrap_or(1);
                kept.cycle = attrs.cycle;
                touched = true;
            }
            AlterAction::DropIdentity(column) => {
                let i = find(&rf, column)?;
                if rf.fields[i].identity.take().is_none() {
                    return Err(err(
                        -190,
                        "42837",
                        format!("column {} is not an identity column", column.value),
                    ));
                }
                let n = rf.fields[i].name;
                cols.retain(|c| c.field != n);
                touched = true;
            }
            AlterAction::DropGenerated(column) => {
                let i = find(&rf, column)?;
                let n = rf.fields[i].name;
                let before = cols.len();
                cols.retain(|c| {
                    !(c.field == n
                        && matches!(
                            c.kind,
                            ColumnKind::RowChange(_) | ColumnKind::Audit(..) | ColumnKind::Expr(_)
                        ))
                });
                if cols.len() == before {
                    return Err(err(
                        -190,
                        "42837",
                        format!("column {} is not generated", column.value),
                    ));
                }
            }
            _ => {}
        }
    }
    if touched {
        reformat(txn, ctx, &table, rf)?;
    }
    seq::store_columns(txn, id, &cols, ctx.clock())?;
    if let Some(r) = restart {
        super::column::set_counter(txn, id, r)?;
    }
    if let Some(field) = number {
        number_rows(txn, id, field)?;
    }
    // 3. Constraints added, over the new format.
    let new_rf = file::describe(txn, id)?.formats[0].format.clone();
    for act in &stmt.actions {
        match act {
            AlterAction::AddConstraint(c) => super::cst::add(txn, ctx, id, &new_rf, c)?,
            AlterAction::AddColumn(c) => {
                for k in &c.constraints {
                    super::cst::add(txn, ctx, id, &new_rf, k)?;
                }
            }
            _ => {}
        }
    }
    // 4. `VOLATILE` and `NOT VOLATILE`, the last one given.
    if let Some(on) = stmt.actions.iter().rev().find_map(|a| match a {
        AlterAction::Volatile(on) => Some(*on),
        _ => None,
    }) {
        super::table::set_volatile(txn, ctx, id, on)?;
    }
    Ok(())
}

/// Give each row of the table the identity column's next value.
fn number_rows(txn: &mut Txn<'_>, id: store::obj::ObjectId, field: Name) -> Result<()> {
    let desc = file::describe(txn, id)?;
    for e in &desc.members {
        let mut m = db::member::Member::open(txn, &desc, e)?;
        let (fi, f) = m
            .format()
            .format
            .field(&field)
            .map(|(i, f)| (i, f.clone()))
            .expect("added");
        let _ = fi;
        let mut rrn = 1;
        loop {
            let slots = m.scan(txn, rrn, 256)?;
            let Some(last) = slots.last() else { break };
            rrn = last.rrn + 1;
            for s in &slots {
                let v = m.next_identity(txn)?.unwrap_or(1);
                let mut rec = s.record.clone();
                let d = qdecimal::Decimal::new(i128::from(v), 0).expect("fits");
                db::record::write_field(&f, &mut rec, &db::Value::Num(d))?;
                m.update(txn, s.rrn, &rec, &s.null_map)?;
            }
        }
    }
    Ok(())
}

fn is_key_column(txn: &mut Txn<'_>, id: store::obj::ObjectId, col: Name) -> Result<bool> {
    Ok(db::cst::list(txn, id)?
        .iter()
        .any(|c| c.kind.keyed() && c.columns.contains(&col)))
}

/// The table takes format `rf`: the records converted when the level
/// changed (`CHGPF SRCFILE`'s rebuild beside the member), else the
/// format replaced.
fn reformat(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    table: &TableInfo,
    mut rf: dds::RecordFormat,
) -> Result<()> {
    rf.lay_out();
    let def = dds::PfDefinition {
        format: rf,
        unique: table.format.unique,
        dup_order: table.format.dup_order,
        reference: None,
        refaccpth: None,
        altseq: None,
        ccsid: None,
        text: None,
    };
    let job = ctx.job;
    match db::chg::chgpf_srcfile_paused(txn, job, table.id, &def, &mut |t| ctx.pause(t)) {
        Err(db::DbError::Definition { what }) if what.contains("dependent logical files") => {
            Err(SqlError::Unsupported {
                what: format!(
                    "an ALTER TABLE that changes the columns of {}, which has indexes or views over it; BLOCKED(rel): dependent files are not rebuilt",
                    table.desc.name()
                ),
            })
        }
        r => Ok(r?),
    }
}

/// The SQL data type a field's DDS type stands for, for its default.
fn data_type_of(f: &dds::FieldDef, t: dds::DataType) -> DataType {
    match t {
        dds::DataType::Date => DataType::Date,
        dds::DataType::Time => DataType::Time,
        dds::DataType::Timestamp => {
            DataType::Timestamp(dds::DataType::timestamp_precision(f.length))
        }
        dds::DataType::DecFloat => DataType::DecFloat(if f.length <= 16 { 16 } else { 34 }),
        dds::DataType::Bytes => DataType::Varbinary(f.length),
        dds::DataType::Boolean => DataType::Boolean,
        dds::DataType::Blob => DataType::Blob(f.length),
        dds::DataType::Clob => DataType::Clob(f.length),
        dds::DataType::Dbclob => DataType::Dbclob(f.length),
        t if t.is_numeric() => DataType::Decimal(u8::try_from(f.length).unwrap_or(63), f.decimals),
        _ => DataType::Char(f.length),
    }
}
