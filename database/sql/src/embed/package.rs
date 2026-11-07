// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL packages for a requester ([`Op::Package`]): the action is the
//! descriptor's name, the library and package the first two inputs.
//!
//! - `CREATE`: kind, naming, commit, date format, date separator, time
//!   format, time separator, decimal point, default collection, text;
//!   SQL0601 when the package exists.
//! - `ADD`: statement type, text, columns and markers (SQLDA bytes), needs
//!   the default collection; one row, the statement's name (an existing
//!   statement with the same text keeps its own).
//! - `BIND`: the token, then each section's number and text: the package
//!   made again as a DRDA requester's.
//! - `RETURN`: the attributes as the first row (kind, naming, commit, date
//!   format, date separator, time format, time separator, decimal point,
//!   default collection, token), then a row per statement (name, type,
//!   text, columns, markers, needs the default collection, section).
//! - `CLEAR`: every statement goes. `DELETE`: the package goes.

// A refusal is the reply itself; it is not boxed.
#![allow(clippy::result_large_err)]

use db::sqlpkg::{self, Package, PkgAttr, PkgKind, PkgStmt};
use qsname::Name;
use store::obj::ObjectId;
use store::obj::ops;
use store::txn::Txn;

use super::{Reply, Session, code};
use crate::error::SqlError;
use crate::exec::SqlContext;
use crate::sqlca::Sqlca;
use crate::value::Value;

/// An input as text.
fn text(inputs: &[Value], i: usize) -> String {
    inputs
        .get(i)
        .map(Value::as_text)
        .unwrap_or_default()
        .trim_end()
        .to_string()
}

/// An input as a small number.
fn num(inputs: &[Value], i: usize) -> u8 {
    inputs
        .get(i)
        .and_then(|v| v.as_i64().ok())
        .and_then(|n| u8::try_from(n).ok())
        .unwrap_or(0)
}

/// An input's first byte (a separator).
fn byte(inputs: &[Value], i: usize) -> u8 {
    text(inputs, i).bytes().next().unwrap_or(0)
}

/// An input as bytes.
fn bin(inputs: &[Value], i: usize) -> Vec<u8> {
    match inputs.get(i) {
        Some(Value::Bin(b)) => b.clone(),
        _ => Vec::new(),
    }
}

fn failed(e: &SqlError) -> Reply {
    Reply::of(Sqlca::from_error(e))
}

/// Package `name` in library `lib`: the library's and the package's IDs.
fn locate(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    lib: &str,
    name: &str,
) -> Result<(ObjectId, Name, Option<ObjectId>), Reply> {
    let not_found = |what: &str, kind: &'static str| {
        failed(&SqlError::ObjectNotFound {
            kind,
            name: what.into(),
        })
    };
    let lib_name = Name::parse(lib).map_err(|_| not_found(lib, "schema"))?;
    let pkg = Name::parse(name).map_err(|_| not_found(name, "package"))?;
    let lib_id = ops::find_library(txn, ctx.qsys, lib_name)
        .map_err(|_| not_found(lib, "schema"))?
        .id;
    let id = sqlpkg::find(txn, lib_id, pkg).map_err(|e| failed(&e.into()))?;
    Ok((lib_id, pkg, id))
}

fn check(txn: &mut Txn<'_>, op: &'static str, id: ObjectId) -> Result<(), Reply> {
    db::auth::check(txn, op, id).map_err(|e| failed(&e.into()))
}

/// The attribute row of `RETURN`.
fn attr_row(a: &PkgAttr) -> Vec<Value> {
    let ch = |b: u8| {
        Value::Str(if b == 0 {
            String::new()
        } else {
            char::from(b).to_string()
        })
    };
    vec![
        Value::int(i64::from(u8::from(a.kind == PkgKind::Bound))),
        Value::int(i64::from(u8::from(a.sql_naming))),
        Value::int(i64::from(a.commit)),
        Value::int(i64::from(a.date_format)),
        ch(a.date_sep),
        Value::int(i64::from(a.time_format)),
        ch(a.time_sep),
        ch(a.decimal_point),
        Value::Str(a.default_collection.clone()),
        Value::Str(a.token.to_string()),
    ]
}

/// A statement row of `RETURN`.
fn stmt_row(s: &PkgStmt) -> Vec<Value> {
    vec![
        Value::Str(s.name.clone()),
        Value::int(i64::from(s.stmt_type)),
        Value::Str(s.text.clone()),
        Value::Bin(s.cols.clone()),
        Value::Bin(s.markers.clone()),
        Value::int(i64::from(u8::from(s.needs_collection))),
        Value::int(i64::from(s.section)),
    ]
}

/// The token of a package made now: the time in microseconds.
fn token(ctx: &SqlContext) -> u64 {
    let (secs, picos) = ctx.clock_picos();
    secs.saturating_mul(1_000_000)
        .saturating_add(picos / 1_000_000)
}

impl Session {
    /// Run a package action.
    #[allow(clippy::too_many_lines)] // one arm per action
    pub(super) fn package(&mut self, req: &super::Request) -> Reply {
        let action = req.desc.name.to_ascii_uppercase();
        let inputs = &req.inputs;
        let (lib, name) = (text(inputs, 0), text(inputs, 1));
        self.with_txn(&req.scope, |txn, ctx, _| {
            let (lib_id, pkg_name, id) = match locate(txn, ctx, &lib, &name) {
                Ok(x) => x,
                Err(r) => return r,
            };
            let missing = || {
                failed(&SqlError::ObjectNotFound {
                    kind: "package",
                    name: format!("{lib}/{name}"),
                })
            };
            let r: Result<Reply, Reply> = (|| match action.as_str() {
                "CREATE" => {
                    if id.is_some() {
                        return Ok(Reply::of(code(-601, "42710")));
                    }
                    let attr = PkgAttr {
                        kind: if num(inputs, 2) == 1 {
                            PkgKind::Bound
                        } else {
                            PkgKind::Extended
                        },
                        sql_naming: num(inputs, 3) == 1,
                        commit: num(inputs, 4),
                        date_format: num(inputs, 5),
                        date_sep: byte(inputs, 6),
                        time_format: num(inputs, 7),
                        time_sep: byte(inputs, 8),
                        decimal_point: byte(inputs, 9),
                        token: token(ctx),
                        default_collection: text(inputs, 10),
                    };
                    let now = ctx.clock();
                    sqlpkg::create(
                        txn,
                        lib_id,
                        pkg_name,
                        &attr,
                        &ctx.user,
                        &text(inputs, 11),
                        now,
                    )
                    .map_err(|e| failed(&e.into()))?;
                    Ok(Reply::default())
                }
                "BIND" => {
                    let now = ctx.clock();
                    if let Some(old) = id {
                        check(txn, "obj.delete", old)?;
                        sqlpkg::delete(txn, ctx.job, old).map_err(|e| failed(&e.into()))?;
                    }
                    let attr = PkgAttr {
                        kind: PkgKind::Bound,
                        sql_naming: true,
                        token: text(inputs, 2).parse().unwrap_or_else(|_| token(ctx)),
                        ..PkgAttr::default()
                    };
                    let stmts = inputs[3.min(inputs.len())..]
                        .chunks(2)
                        .filter(|c| c.len() == 2)
                        .map(|c| {
                            let section = c[0]
                                .as_i64()
                                .ok()
                                .and_then(|n| u16::try_from(n).ok())
                                .unwrap_or(0);
                            PkgStmt {
                                name: format!("S{section}"),
                                section,
                                text: c[1].as_text(),
                                ..PkgStmt::default()
                            }
                        })
                        .collect();
                    let new = sqlpkg::create(txn, lib_id, pkg_name, &attr, &ctx.user, "", now)
                        .map_err(|e| failed(&e.into()))?;
                    sqlpkg::save(txn, new, &Package { attr, stmts })
                        .map_err(|e| failed(&e.into()))?;
                    Ok(Reply::default())
                }
                "ADD" => {
                    let id = id.ok_or_else(missing)?;
                    check(txn, "sqlpkg.change", id)?;
                    let mut p = sqlpkg::load(txn, id).map_err(|e| failed(&e.into()))?;
                    let stmt_text = text(inputs, 3);
                    let name = if let Some(s) = p.by_text(&stmt_text) {
                        s.name.clone()
                    } else {
                        let name = p.next_name();
                        p.stmts.push(PkgStmt {
                            name: name.clone(),
                            stmt_type: num(inputs, 2),
                            needs_collection: num(inputs, 6) == 1,
                            section: 0,
                            text: stmt_text,
                            cols: bin(inputs, 4),
                            markers: bin(inputs, 5),
                        });
                        sqlpkg::save(txn, id, &p).map_err(|e| failed(&e.into()))?;
                        name
                    };
                    Ok(Reply {
                        sqlca: Sqlca::default(),
                        rows: vec![vec![Value::Str(name)]],
                        call: None,
                    })
                }
                "RETURN" => {
                    let id = id.ok_or_else(missing)?;
                    check(txn, "sqlpkg.use", id)?;
                    let p = sqlpkg::load(txn, id).map_err(|e| failed(&e.into()))?;
                    let mut rows = vec![attr_row(&p.attr)];
                    rows.extend(p.stmts.iter().map(stmt_row));
                    Ok(Reply {
                        sqlca: Sqlca::default(),
                        rows,
                        call: None,
                    })
                }
                "CLEAR" => {
                    let id = id.ok_or_else(missing)?;
                    check(txn, "sqlpkg.change", id)?;
                    let mut p = sqlpkg::load(txn, id).map_err(|e| failed(&e.into()))?;
                    p.stmts.clear();
                    sqlpkg::save(txn, id, &p).map_err(|e| failed(&e.into()))?;
                    Ok(Reply::default())
                }
                "DELETE" => {
                    let id = id.ok_or_else(missing)?;
                    check(txn, "obj.delete", id)?;
                    sqlpkg::delete(txn, ctx.job, id).map_err(|e| failed(&e.into()))?;
                    Ok(Reply::default())
                }
                _ => Ok(Reply::of(code(-84, "42612"))),
            })();
            r.unwrap_or_else(|e| e)
        })
    }
}
