// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE [OR REPLACE] TABLE`: of columns (`ddl`), `AS (query) WITH
//! [NO] DATA`, `LIKE`, and the column system names IBM generates.

use db::file::{self, CreateFileOpts};
use db::sqlinf::body::TableDef;
use dds::FieldDef;
use qsname::Name;
use store::obj::ObjectId;
use store::txn::Txn;

use super::eval::ColInfo;
use super::{SqlContext, names};
use crate::ast::{ColumnDef, CreateTable, Ident};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The system names of a table's columns: `FOR COLUMN`, the name itself,
/// or generated in column order, numbered past the names taken.
pub fn column_names(cols: &[ColumnDef]) -> Result<Vec<Name>> {
    let mut out: Vec<Option<Name>> = Vec::with_capacity(cols.len());
    for c in cols {
        names::check_length(&c.name)?;
        out.push(match &c.system_name {
            Some(s) => Some(names::direct(s).ok_or_else(|| SqlError::SystemName { name: s.value.clone() })?),
            None => names::direct(&c.name).filter(|n| !n.is_quoted()),
        });
    }
    let mut taken: Vec<Name> = out.iter().flatten().copied().collect();
    for (i, c) in cols.iter().enumerate() {
        if out[i].is_none() {
            let n = names::generate_column(&c.name.value, &|n| taken.contains(n));
            taken.push(n);
            out[i] = Some(n);
        }
    }
    Ok(out.into_iter().map(|n| n.expect("named")).collect())
}

/// The system names of columns given by SQL name alone.
fn names_of(sql: &[String]) -> Vec<Name> {
    let mut taken: Vec<Name> = Vec::new();
    let mut out = Vec::with_capacity(sql.len());
    for s in sql {
        let direct = Name::parse(s).ok().filter(|n| n.to_string() == *s);
        let n = match direct {
            Some(n) if !taken.contains(&n) => n,
            _ => names::generate_column(s, &|n| taken.contains(n) || sql.iter().any(|o| o == &n.to_string())),
        };
        taken.push(n);
        out.push(n);
    }
    out
}

/// `CREATE TABLE`, every form.
pub fn create(txn: &mut Txn<'_>, ctx: &mut SqlContext, t: &CreateTable, params: &[Value]) -> Result<u64> {
    // `SESSION` is a declared temporary table's qualifier, no schema.
    if t.name.schema.as_ref().is_some_and(|s| s.value == super::qtemp::SESSION) {
        return Err(SqlError::ObjectNotFound { name: super::qtemp::SESSION.into(), kind: "schema" });
    }
    let replaced = if t.replace { replace_old(txn, ctx, t)? } else { None };
    let count = if let Some((of, all_null)) = &t.like {
        let info = ctx.resolve_table(txn, of)?;
        let mut fields = info.format.format.fields.clone();
        // `(LIKE t)` among the columns makes every column nullable.
        if *all_null {
            for f in &mut fields {
                f.allow_null = true;
            }
        }
        create_from_fields(txn, ctx, t, fields)?;
        0
    } else if let Some((query, data)) = &t.as_query {
        let (cols, rows) = super::query::columns_and_rows(txn, ctx, params, query)?;
        let fields = fields_of(&t.column_names, &cols)?;
        let id = create_from_fields(txn, ctx, t, fields)?;
        if *data { insert_rows(txn, ctx, id, rows)? } else { 0 }
    } else {
        super::ddl::create_table_def(txn, ctx, t)?;
        0
    };
    if let Some(old) = replaced {
        restore_rows(txn, ctx, t, old)?;
    }
    if t.volatile == Some(true) {
        let id = ctx.resolve_file(txn, &t.name)?;
        set_volatile(txn, ctx, id, true)?;
    }
    // `IN schema` Db2 for i ignores, saying so (no `SQL7905` then).
    if t.in_schema {
        ctx.warning = Some(crate::sqlca::Warning::coded(20_367, "01680", "IN"));
    }
    if super::qtemp::qualifier(&t.name) == Some(false)
        && let Ok(id) = ctx.resolve_file(txn, &t.name)
    {
        ctx.qtemp.plain_table(id);
    }
    Ok(count)
}

/// Whether table `id` is `VOLATILE` (its `TABLE` entry says so).
pub(crate) fn is_volatile(txn: &mut Txn<'_>, id: ObjectId) -> bool {
    db::sqlinf::load(txn, id)
        .ok()
        .flatten()
        .and_then(|i| i.first(db::sqlinf::tag::TABLE).and_then(|b| TableDef::decode(b).ok()))
        .is_some_and(|d| d.volatile)
}

/// Make table `id` `VOLATILE` or not: its `TABLE` entry, dropped when
/// nothing is left in it. A file SQL did not make keeps its kind.
pub(crate) fn set_volatile(txn: &mut Txn<'_>, ctx: &SqlContext, id: ObjectId, on: bool) -> Result<()> {
    let mut info = db::sqlinf::load(txn, id)?.unwrap_or_else(|| db::sqlinf::SqlInfo::new(db::sqlinf::Kind::File));
    let mut d = info.first(db::sqlinf::tag::TABLE).and_then(|b| TableDef::decode(b).ok()).unwrap_or_default();
    d.volatile = on;
    if d == TableDef::default() {
        info.remove(db::sqlinf::tag::TABLE);
    } else {
        info.set(db::sqlinf::tag::TABLE, d.encode());
    }
    db::sqlinf::save(txn, id, &info, ctx.clock())?;
    Ok(())
}

/// The fields of a query's result: each column's field, its SQL name the
/// one the query gives it.
fn fields_of(given: &[Ident], cols: &[ColInfo]) -> Result<Vec<FieldDef>> {
    let mut sql = Vec::with_capacity(cols.len());
    let mut fields = Vec::with_capacity(cols.len());
    for (i, c) in cols.iter().enumerate() {
        // A result column with only its position for a name needs one given.
        if c.name.chars().all(|ch| ch.is_ascii_digit()) && given.get(i).is_none() {
            return Err(SqlError::ColumnListRequired);
        }
        let Some(f) = &c.field else {
            return Err(SqlError::Unsupported {
                what: format!("CREATE TABLE AS with the expression column {}: its type is not derived yet", c.name),
            });
        };
        let name = match given.get(i) {
            Some(g) => g.value.clone(),
            None if f.name.to_string() == c.name => f.alias.clone().unwrap_or_else(|| c.name.clone()),
            None => c.name.clone(),
        };
        let mut f = f.clone();
        f.allow_null = c.nullable;
        sql.push(name);
        fields.push(f);
    }
    // A column keeps its system name when its SQL name is the one it had.
    let generated = names_of(&sql);
    for ((f, s), g) in fields.iter_mut().zip(&sql).zip(generated) {
        let keeps = f.alias.as_deref() == Some(s.as_str()) || f.name.to_string() == *s;
        if !keeps {
            f.name = g;
            f.alias = (g.to_string() != *s).then(|| s.clone());
        }
    }
    Ok(fields)
}

/// A table of the fields given, without keys, identity or the
/// attributes of a logical field.
fn create_from_fields(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    t: &CreateTable,
    mut fields: Vec<FieldDef>,
) -> Result<ObjectId> {
    let library = ctx.library_for(txn, &t.name)?;
    let (name, sql) = names::new_object(txn, library, &t.name.name, t.system_name.as_ref())?;
    for f in &mut fields {
        f.identity = None;
        f.usage = None;
        f.reference = None;
        f.line = 0;
    }
    let rcdfmt = match &t.rcdfmt {
        Some(r) => SqlContext::sys_name(r)?,
        None => names::format_name(name),
    };
    let typed = super::udt::column::typed(&fields);
    let mut rf = dds::RecordFormat::new(rcdfmt);
    rf.fields = fields;
    rf.lay_out();
    let def = dds::PfDefinition {
        format: rf,
        unique: None,
        dup_order: None,
        reference: None,
        refaccpth: None,
        altseq: None,
        ccsid: None,
        text: None,
    };
    let id = file::create_pf(
        txn,
        library,
        name,
        &def,
        &CreateFileOpts { now: ctx.clock(), text: String::new(), size: file::Size::NOMAX, ..CreateFileOpts::default() },
    )?;
    super::ddl::finish_table(txn, ctx, library, id, sql.as_deref(), &t.name.name.value)?;
    super::udt::column::record(txn, id, &typed, db::sqlinf::Kind::Table, ctx.clock())?;
    Ok(id)
}

/// Insert rows into a new table through the ordinary insert path.
fn insert_rows(txn: &mut Txn<'_>, ctx: &mut SqlContext, id: ObjectId, rows: Vec<Vec<Value>>) -> Result<u64> {
    if rows.is_empty() {
        return Ok(0);
    }
    let info = table_info(txn, id)?;
    let idx: Vec<usize> = (0..info.format.format.fields.len()).collect();
    let rows = rows.into_iter().map(|r| r.into_iter().map(Some).collect()).collect();
    super::dml::write_rows(txn, ctx, &[], &info, &idx, None, rows)
}

/// A file's first member and format as a table.
fn table_info(txn: &mut Txn<'_>, id: ObjectId) -> Result<super::TableInfo> {
    let desc = file::describe(txn, id)?;
    let member = *desc.members.first().ok_or_else(|| SqlError::ObjectNotFound { name: desc.name().to_string(), kind: "member" })?;
    let format = desc.formats.first().cloned().ok_or_else(|| SqlError::ObjectNotFound { name: desc.name().to_string(), kind: "table" })?;
    Ok(super::TableInfo { id, desc, member, format })
}

/// The rows of a table being replaced, by column SQL name.
struct Kept {
    columns: Vec<String>,
    rows: Vec<Vec<Value>>,
}

/// `OR REPLACE`: the table there now goes, its rows kept for `ON
/// REPLACE PRESERVE ROWS` (the default). A view or alias of the name is
/// `SQL0601`.
fn replace_old(txn: &mut Txn<'_>, ctx: &mut SqlContext, t: &CreateTable) -> Result<Option<Kept>> {
    let Ok(id) = ctx.resolve_file(txn, &t.name) else {
        return Ok(None);
    };
    if file::alias_of(txn, id)?.is_some() || file::describe(txn, id)?.attr.kind != file::FileKind::Physical {
        return Err(SqlError::Exists { name: t.name.name.value.clone(), kind: "*FILE" });
    }
    let old = table_info(txn, id)?;
    if !old.desc.dependents.is_empty() {
        // BLOCKED(rel): IBM keeps the views and indexes over a replaced
        // table; rebuilding them waits on ALTER TABLE's (SQL-2).
        return Err(SqlError::Unsupported { what: "CREATE OR REPLACE TABLE of a table with views or indexes over it".into() });
    }
    let columns = old.format.format.fields.iter().map(|f| f.alias.clone().unwrap_or_else(|| f.name.to_string())).collect();
    let rows = if t.preserve_rows {
        let (_, rows) = super::query::matching_rows(txn, ctx, &[], &old, "T", None, &super::eval::Env::default())?;
        // The relative record number comes first.
        rows.into_iter().map(|r| r.into_iter().skip(1).collect()).collect()
    } else {
        Vec::new()
    };
    file::delete_file(txn, ctx.job, id)?;
    Ok(Some(Kept { columns, rows }))
}

/// The rows a replaced table had, by column name, into the new one: a
/// column the new table lacks is dropped, a new one takes its default.
fn restore_rows(txn: &mut Txn<'_>, ctx: &mut SqlContext, t: &CreateTable, old: Kept) -> Result<()> {
    if old.rows.is_empty() {
        return Ok(());
    }
    let info = ctx.resolve_table(txn, &t.name)?;
    let mut idx = Vec::new();
    let mut from = Vec::new();
    for (i, c) in old.columns.iter().enumerate() {
        if let Some((fi, _)) = info.field(c) {
            idx.push(fi);
            from.push(i);
        }
    }
    let rows = old.rows.into_iter().map(|r| from.iter().map(|&i| Some(r[i].clone())).collect()).collect();
    super::dml::write_rows(txn, ctx, &[], &info, &idx, None, rows)?;
    Ok(())
}
