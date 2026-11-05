// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.SYSTABAUTH` and `QSYS2.SYSCOLAUTH`: the privileges on tables
//! and views as their object authorities say (`exec::grant`), the
//! columns pub400's. SYSCOLAUTH is empty: the store keeps no column
//! authorities.

use dds::{DataType, FieldDef};
use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::super::SqlContext;
use super::super::eval::{ColInfo, Row, Schema};
use super::Table;
use crate::ast::ObjectName;
use crate::error::Result;
use crate::value::Value;

const VIEWS: &[&str] = &["SYSTABAUTH", "SYSCOLAUTH"];

/// Whether `name` is one of these views.
pub fn has(name: &ObjectName) -> bool {
    name.schema.as_ref().is_none_or(|s| s.value == "QSYS2") && VIEWS.contains(&name.name.value.as_str())
}

fn col(alias: &str, name: &str, len: u32, varying: bool, nullable: bool) -> ColInfo {
    let mut f = FieldDef::new(Name::parse(&name[..name.len().min(10)]).unwrap_or_else(|_| Name::parse("COL").expect("valid")), DataType::Char, len, 0);
    f.allow_null = nullable;
    if varying {
        f.varlen = Some(dds::VarLen { allocated: None });
    }
    if name.len() > 10 {
        f.alias = Some(name.to_string());
    }
    ColInfo { name: name.to_string(), table: Some(alias.to_string()), field: Some(f), nullable, hidden: false }
}

fn s(v: impl Into<String>) -> Value {
    Value::Str(v.into())
}

/// A system name blank padded to ten, as a CHAR(10) column holds it.
fn sys(n: &str) -> Value {
    s(format!("{n:<10}"))
}

/// The rows and columns, or `None` when `name` is not one of these.
pub fn virtual_table(txn: &mut Txn<'_>, ctx: &SqlContext, name: &ObjectName, alias: &str) -> Result<Option<Table>> {
    if !has(name) {
        return Ok(None);
    }
    let column = name.name.value == "SYSCOLAUTH";
    let mut cols = vec![
        col(alias, "GRANTOR", 128, true, true),
        col(alias, "GRANTEE", 128, true, false),
        col(alias, "TABLE_SCHEMA", 128, true, false),
        col(alias, "TABLE_NAME", 128, true, false),
    ];
    if column {
        cols.push(col(alias, "COLUMN_NAME", 128, true, false));
    }
    cols.extend([
        col(alias, "PRIVILEGE_TYPE", 10, true, false),
        col(alias, "IS_GRANTABLE", 3, true, false),
        col(alias, "AUTHORIZATION_LIST", 10, true, true),
        col(alias, "SYSTEM_TABLE_SCHEMA", 10, false, false),
        col(alias, "SYSTEM_TABLE_NAME", 10, false, false),
    ]);
    if column {
        cols.push(col(alias, "SYSTEM_COLUMN_NAME", 10, true, false));
        return Ok(Some((Schema { cols }, Vec::new())));
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut libs: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?.into_iter().map(|(_, id)| id).collect();
    libs.insert(0, ctx.qsys);
    for lib in libs {
        let Ok((_, lh)) = ops::read_header(txn, lib) else { continue };
        let Ok(files) = ops::list(txn, lib, Some(types::FILE)) else { continue };
        let schema = crate::exec::names::schema_name(txn, lib);
        for (_, id) in files {
            let Ok(d) = db::file::describe(txn, id) else { continue };
            let is_view = crate::exec::view::is_view(txn, id);
            let is_index = crate::exec::names::kind_of(txn, id) == Some(db::sqlinf::Kind::Index);
            if is_index {
                continue;
            }
            let table = crate::exec::names::sql_name(txn, id);
            let autl = ccsid::decode_trimmed(&d.header.autl);
            for (who, bits) in crate::exec::grant::grantees(txn, id) {
                for (p, grantable) in crate::exec::grant::privileges(bits, is_view) {
                    rows.push(vec![
                        Value::Null,
                        s(who.clone()),
                        s(schema.clone()),
                        s(table.clone()),
                        s(p),
                        s(if grantable { "YES" } else { "NO" }),
                        if autl.is_empty() { Value::Null } else { s(autl.clone()) },
                        sys(&lh.name.to_string()),
                        sys(&d.name().to_string()),
                    ]);
                }
            }
        }
    }
    Ok(Some((Schema { cols }, rows)))
}
