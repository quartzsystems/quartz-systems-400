// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The objects `GENERATE_SQL` finds, and the SQL of each, from the
//! catalog views and the file descriptions. QS/400 keeps a table's
//! comment as its label: no `COMMENT ON` is generated.

use std::fmt::Write as _;

use dds::DataType;
use store::txn::Txn;

use super::super::super::super::SqlContext;
use super::format::{Line, Options, ident, literal};
use crate::ast::{Ident, ObjectName};
use crate::error::{Result, SqlError};
use crate::value::Value;
use crate::{Naming, Outcome};

/// An object found: its kind, SQL schema and name, system schema and
/// name.
#[derive(Clone, Debug)]
pub struct Object {
    kind: &'static str,
    schema: String,
    name: String,
    system_schema: String,
    system_name: String,
}

/// The rows of a catalog query run inside the call; the call's own
/// warning and naming kept.
pub fn query(txn: &mut Txn<'_>, ctx: &mut SqlContext, sql: &str, params: &[Value]) -> Result<Vec<Vec<Value>>> {
    let p = crate::prepare(sql, Naming::Sql)?;
    let (warning, naming) = (ctx.warning.take(), ctx.naming);
    let r = p.execute(txn, ctx, params);
    ctx.warning = warning;
    ctx.naming = naming;
    Ok(match r? {
        Outcome::Rows(rs) => rs.rows,
        _ => Vec::new(),
    })
}

fn text(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(v) => v.as_text().trim_end().to_string(),
    }
}

fn num(v: Option<&Value>) -> i64 {
    v.and_then(|v| v.as_i64().ok()).unwrap_or(0)
}

/// The objects named `name` (a `LIKE` pattern) of `kind` in `lib` (an
/// SQL or a system schema name), in name order.
pub fn find(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &str, lib: &str, kind: &str) -> Result<Vec<Object>> {
    let p = [Value::Str(lib.to_string()), Value::Str(name.to_string())];
    let (sql, kind): (&str, &'static str) = match kind {
        "TABLE" => (
            "SELECT TABLE_SCHEMA, TABLE_NAME, SYSTEM_TABLE_SCHEMA, SYSTEM_TABLE_NAME FROM QSYS2.SYSTABLES \
             WHERE (TABLE_SCHEMA = ?1 OR SYSTEM_TABLE_SCHEMA = ?1) AND (TABLE_NAME LIKE ?2 OR SYSTEM_TABLE_NAME LIKE ?2) \
             AND TABLE_TYPE IN ('T', 'P') ORDER BY TABLE_NAME",
            "TABLE",
        ),
        "VIEW" => (
            "SELECT TABLE_SCHEMA, TABLE_NAME, SYSTEM_TABLE_SCHEMA, SYSTEM_TABLE_NAME FROM QSYS2.SYSTABLES \
             WHERE (TABLE_SCHEMA = ?1 OR SYSTEM_TABLE_SCHEMA = ?1) AND (TABLE_NAME LIKE ?2 OR SYSTEM_TABLE_NAME LIKE ?2) \
             AND TABLE_TYPE = 'V' ORDER BY TABLE_NAME",
            "VIEW",
        ),
        "ALIAS" => (
            "SELECT TABLE_SCHEMA, TABLE_NAME, SYSTEM_TABLE_SCHEMA, SYSTEM_TABLE_NAME FROM QSYS2.SYSTABLES \
             WHERE (TABLE_SCHEMA = ?1 OR SYSTEM_TABLE_SCHEMA = ?1) AND (TABLE_NAME LIKE ?2 OR SYSTEM_TABLE_NAME LIKE ?2) \
             AND TABLE_TYPE = 'A' ORDER BY TABLE_NAME",
            "ALIAS",
        ),
        "INDEX" => (
            "SELECT INDEX_SCHEMA, INDEX_NAME, SYSTEM_INDEX_SCHEMA, SYSTEM_INDEX_NAME FROM QSYS2.SYSINDEXES \
             WHERE (INDEX_SCHEMA = ?1 OR SYSTEM_INDEX_SCHEMA = ?1) AND (INDEX_NAME LIKE ?2 OR SYSTEM_INDEX_NAME LIKE ?2) \
             ORDER BY INDEX_NAME",
            "INDEX",
        ),
        _ => (
            "SELECT SEQUENCE_SCHEMA, SEQUENCE_NAME, SYSTEM_SEQ_SCHEMA, SYSTEM_SEQ_NAME FROM QSYS2.SYSSEQUENCES \
             WHERE (SEQUENCE_SCHEMA = ?1 OR SYSTEM_SEQ_SCHEMA = ?1) AND (SEQUENCE_NAME LIKE ?2 OR SYSTEM_SEQ_NAME LIKE ?2) \
             ORDER BY SEQUENCE_NAME",
            "SEQUENCE",
        ),
    };
    let sql = sql.replace("?1", "?").replacen("?2", "?", 2);
    // The markers in order: schema twice, name twice.
    let params = [p[0].clone(), p[0].clone(), p[1].clone(), p[1].clone()];
    Ok(query(txn, ctx, &sql, &params)?
        .into_iter()
        .map(|r| Object {
            kind,
            schema: text(r.first()),
            name: text(r.get(1)),
            system_schema: text(r.get(2)),
            system_name: text(r.get(3)),
        })
        .collect())
}

/// `lib.name`, or `name` when names are left unqualified.
fn qual(opts: &Options, schema: &str, name: &str) -> String {
    if opts.unqualified { ident(name) } else { format!("{}.{}", ident(schema), ident(name)) }
}

/// The SQL of one object.
pub fn generate(txn: &mut Txn<'_>, ctx: &mut SqlContext, o: &Object, opts: &Options) -> Result<Vec<Line>> {
    let mut out = Vec::new();
    let me = qual(opts, &o.schema, &o.name);
    if opts.drop {
        out.push(Line::top(format!("DROP {} IF EXISTS {me} ;", o.kind)));
        out.push(Line::blank());
    }
    match o.kind {
        "TABLE" => table(txn, ctx, o, opts, &me, &mut out)?,
        "VIEW" => view(txn, ctx, o, opts, &me, &mut out)?,
        "INDEX" => index(txn, ctx, o, opts, &me, &mut out)?,
        "SEQUENCE" => sequence(txn, ctx, o, opts, &me, &mut out)?,
        _ => alias(txn, ctx, o, opts, &me, &mut out)?,
    }
    Ok(out)
}

fn object_name(o: &Object) -> ObjectName {
    ObjectName { schema: Some(Ident::new(&o.system_schema)), name: Ident::new(&o.system_name) }
}

/// The file's description.
fn describe(txn: &mut Txn<'_>, ctx: &SqlContext, o: &Object) -> Result<db::file::FileDesc> {
    let id = ctx.resolve_file(txn, &object_name(o))?;
    Ok(db::file::describe(txn, id)?)
}

/// `CREATE ... name (` with `FOR SYSTEM NAME` when the names differ.
fn create_line(what: &str, o: &Object, opts: &Options, me: &str) -> String {
    let verb = if opts.or_replace { "CREATE OR REPLACE" } else { "CREATE" };
    if opts.system_names && o.system_name != o.name {
        format!("{verb} {what} {me} FOR SYSTEM NAME {}", o.system_name)
    } else {
        format!("{verb} {what} {me}")
    }
}

/// A column's definition: one line, or an identity column's five.
fn column(f: &dds::FieldDef, file_ccsid: u16, opts: &Options) -> Vec<String> {
    let name = f.alias.clone().unwrap_or_else(|| f.name.to_string());
    let mut head = ident(&name);
    if opts.system_names && f.alias.is_some() && name != f.name.to_string() {
        let _ = write!(head, " FOR COLUMN {}", f.name);
    }
    let mut def = format!("{head} {}", crate::exec::eval::sql_type(f));
    let character = matches!(f.data_type, DataType::Char | DataType::Graphic | DataType::DbcsOpen | DataType::Clob | DataType::Dbclob);
    match f.ccsid.unwrap_or(file_ccsid) {
        65535 if f.data_type == DataType::Char => def.push_str(" FOR BIT DATA"),
        c if opts.ccsids && character && c != 0 => {
            let _ = write!(def, " CCSID {c}");
        }
        _ => {}
    }
    if let Some(id) = &f.identity {
        return vec![
            format!("{def} GENERATED {} AS IDENTITY (", if id.always { "ALWAYS" } else { "BY DEFAULT" }),
            format!("START WITH {} INCREMENT BY {}", id.start, id.increment),
            "NO MINVALUE NO MAXVALUE".into(),
            format!("{} NO ORDER", if id.cycle { "CYCLE" } else { "NO CYCLE" }),
            "CACHE 20 )".into(),
        ];
    }
    if !f.allow_null {
        def.push_str(" NOT NULL");
    }
    match &f.default {
        Some(d) => {
            let _ = write!(def, " DEFAULT {}", crate::exec::catalog::sys::default_text(d));
        }
        None if f.allow_null => def.push_str(" DEFAULT NULL"),
        None => {}
    }
    vec![def]
}

/// The columns of a constraint, by name.
fn key_columns(txn: &mut Txn<'_>, ctx: &mut SqlContext, schema: &str, name: &str) -> Result<String> {
    let rows = query(
        txn,
        ctx,
        "SELECT COLUMN_NAME FROM QSYS2.SYSKEYCST WHERE CONSTRAINT_SCHEMA = ? AND CONSTRAINT_NAME = ? ORDER BY ORDINAL_POSITION",
        &[Value::Str(schema.into()), Value::Str(name.into())],
    )?;
    Ok(rows.iter().map(|r| ident(&text(r.first()))).collect::<Vec<_>>().join(" , "))
}

#[allow(clippy::too_many_lines)] // the table, its constraints, labels and privileges in IBM's order
fn table(txn: &mut Txn<'_>, ctx: &mut SqlContext, o: &Object, opts: &Options, me: &str, out: &mut Vec<Line>) -> Result<()> {
    let desc = describe(txn, ctx, o)?;
    let Some(fmt) = desc.formats.first() else {
        return Err(SqlError::Database { what: format!("{} has no record format", o.name), msgid: None });
    };
    let fields = fmt.format.fields.clone();
    let csts = query(
        txn,
        ctx,
        "SELECT CONSTRAINT_SCHEMA, CONSTRAINT_NAME, CONSTRAINT_TYPE FROM QSYS2.SYSCST WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? ORDER BY CONSTRAINT_NAME",
        &[Value::Str(o.schema.clone()), Value::Str(o.name.clone())],
    )?;
    let csts: Vec<(String, String, String)> = csts.iter().map(|r| (text(r.first()), text(r.get(1)), text(r.get(2)))).collect();
    let pk = csts.iter().find(|c| c.2 == "PRIMARY KEY").filter(|_| opts.constraints);
    out.push(Line::top(format!("{} (", create_line("TABLE", o, opts, me))));
    for (i, f) in fields.iter().enumerate() {
        let last = i + 1 == fields.len() && pk.is_none();
        let lines = column(f, desc.attr.ccsid, opts);
        let identity = lines.len() > 1;
        let n = lines.len();
        for (j, l) in lines.into_iter().enumerate() {
            if j + 1 < n || identity {
                out.push(Line::tab(l));
            } else {
                out.push(Line::tab(format!("{l} {}", if last { ")  " } else { "," })));
            }
        }
        if identity {
            out.push(Line::tab(if last { ")  " } else { "," }));
        }
    }
    if let Some((s, n, _)) = pk {
        let cols = key_columns(txn, ctx, s, n)?;
        out.push(Line::tab(format!("CONSTRAINT {} PRIMARY KEY( {cols} ) )  ", qual(opts, s, n))));
    }
    out.push(Line::Empty);
    out.push(Line::tab(if opts.system_names { format!("RCDFMT {:<10} ;", fmt.format.name.to_string()) } else { ";".into() }));
    out.push(Line::blank());
    if opts.constraints {
        let order = |t: &str| ["UNIQUE", "FOREIGN KEY", "CHECK"].iter().position(|x| *x == t).unwrap_or(9);
        let mut rest: Vec<&(String, String, String)> = csts.iter().filter(|c| c.2 != "PRIMARY KEY").collect();
        rest.sort_by_key(|c| (order(&c.2), c.1.clone()));
        for (s, n, t) in rest {
            out.push(Line::top(format!("ALTER TABLE {me}")));
            out.push(Line::tab(format!("ADD CONSTRAINT {}", qual(opts, s, n))));
            let p = [Value::Str(s.clone()), Value::Str(n.clone())];
            match t.as_str() {
                "UNIQUE" => out.push(Line::tab(format!("UNIQUE( {} ) ;", key_columns(txn, ctx, s, n)?))),
                "FOREIGN KEY" => {
                    out.push(Line::tab(format!("FOREIGN KEY( {} )", key_columns(txn, ctx, s, n)?)));
                    let r = query(
                        txn,
                        ctx,
                        "SELECT UNIQUE_CONSTRAINT_SCHEMA, UNIQUE_CONSTRAINT_NAME, UPDATE_RULE, DELETE_RULE FROM QSYS2.SYSREFCST WHERE CONSTRAINT_SCHEMA = ? AND CONSTRAINT_NAME = ?",
                        &p,
                    )?;
                    let r = r.first().cloned().unwrap_or_default();
                    let (us, un) = (text(r.first()), text(r.get(1)));
                    let parent = query(
                        txn,
                        ctx,
                        "SELECT TABLE_SCHEMA, TABLE_NAME FROM QSYS2.SYSCST WHERE CONSTRAINT_SCHEMA = ? AND CONSTRAINT_NAME = ?",
                        &[Value::Str(us.clone()), Value::Str(un.clone())],
                    )?;
                    let parent = parent.first().cloned().unwrap_or_default();
                    out.push(Line::tab(format!(
                        "REFERENCES {} ( {} )",
                        qual(opts, &text(parent.first()), &text(parent.get(1))),
                        key_columns(txn, ctx, &us, &un)?
                    )));
                    out.push(Line::tab(format!("ON DELETE {}", text(r.get(3)))));
                    out.push(Line::tab(format!("ON UPDATE {} ;", text(r.get(2)))));
                }
                _ => {
                    let r = query(txn, ctx, "SELECT CHECK_CLAUSE FROM QSYS2.SYSCHKCST WHERE CONSTRAINT_SCHEMA = ? AND CONSTRAINT_NAME = ?", &p)?;
                    out.push(Line::tab(format!("CHECK( {} ) ;", text(r.first().and_then(|r| r.first())))));
                }
            }
            out.push(Line::blank());
        }
    }
    if opts.labels {
        let label = ccsid::decode_trimmed(&desc.header.text);
        if !label.is_empty() {
            out.push(Line::top(format!("LABEL ON TABLE {me}")));
            out.push(Line::tab(format!("IS {} ;", literal(&label))));
            out.push(Line::blank());
        }
        column_labels(&fields, me, out);
    }
    if opts.privileges {
        grants(txn, ctx, o, me, false, out)?;
    }
    Ok(())
}

/// `LABEL ON COLUMN` for the headings, then for the texts.
fn column_labels(fields: &[dds::FieldDef], me: &str, out: &mut Vec<Line>) {
    let name = |f: &dds::FieldDef| ident(&f.alias.clone().unwrap_or_else(|| f.name.to_string()));
    let headings: Vec<String> = fields
        .iter()
        .filter(|f| !f.colhdg.is_empty())
        .map(|f| {
            let parts: Vec<String> = f.colhdg.iter().map(|p| format!("{p:<20}")).collect();
            format!("{} IS {}", name(f), literal(parts.concat().trim_end()))
        })
        .collect();
    let texts: Vec<String> = fields
        .iter()
        .filter_map(|f| f.text.as_ref().filter(|t| !t.trim().is_empty()).map(|t| format!("{} TEXT IS {}", name(f), literal(t.trim_end()))))
        .collect();
    for list in [headings, texts] {
        if list.is_empty() {
            continue;
        }
        out.push(Line::top(format!("LABEL ON COLUMN {me}")));
        let n = list.len();
        for (i, l) in list.into_iter().enumerate() {
            let open = if i == 0 { "( " } else { "" };
            let close = if i + 1 == n { " ) ;" } else { " ," };
            out.push(Line::top(format!("{open}{l}{close}")));
        }
        out.push(Line::blank());
    }
}

/// The `GRANT`s of a table or view from `SYSTABAUTH`, one per
/// grantee.
fn grants(txn: &mut Txn<'_>, ctx: &mut SqlContext, o: &Object, me: &str, view: bool, out: &mut Vec<Line>) -> Result<()> {
    let rows = query(
        txn,
        ctx,
        "SELECT GRANTEE, PRIVILEGE_TYPE, IS_GRANTABLE FROM QSYS2.SYSTABAUTH WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?",
        &[Value::Str(o.schema.clone()), Value::Str(o.name.clone())],
    )?;
    let mut by: Vec<(String, Vec<String>, bool)> = Vec::new();
    for r in rows {
        let (g, p, yes) = (text(r.first()), text(r.get(1)), text(r.get(2)) == "YES");
        if view && p == "INDEX" {
            continue;
        }
        match by.iter_mut().find(|(x, _, _)| *x == g) {
            Some((_, ps, all)) => {
                ps.push(p);
                *all &= yes;
            }
            None => by.push((g, vec![p], yes)),
        }
    }
    for (g, mut ps, grantable) in by {
        ps.sort();
        ps.dedup();
        out.push(Line::top(format!("GRANT {}  ", ps.join(" , "))));
        let to = if g == "PUBLIC" || g == "*PUBLIC" { "PUBLIC".to_string() } else { g };
        out.push(Line::top(format!("ON {me} TO {to}{} ;", if grantable { " WITH GRANT OPTION" } else { "" })));
        out.push(Line::blank());
    }
    Ok(())
}

fn view(txn: &mut Txn<'_>, ctx: &mut SqlContext, o: &Object, opts: &Options, me: &str, out: &mut Vec<Line>) -> Result<()> {
    let desc = describe(txn, ctx, o)?;
    let fields = desc.formats.first().map(|f| f.format.fields.clone()).unwrap_or_default();
    let rows = query(
        txn,
        ctx,
        "SELECT VIEW_DEFINITION, CHECK_OPTION FROM QSYS2.SYSVIEWS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?",
        &[Value::Str(o.schema.clone()), Value::Str(o.name.clone())],
    )?;
    let r = rows.first().cloned().unwrap_or_default();
    out.push(Line::top(format!("{} (", create_line("VIEW", o, opts, me))));
    for (i, f) in fields.iter().enumerate() {
        let n = ident(&f.alias.clone().unwrap_or_else(|| f.name.to_string()));
        out.push(Line::tab(format!("{n} {}", if i + 1 == fields.len() { ")" } else { "," })));
    }
    out.push(Line::tab("AS"));
    out.push(Line::tab(format!("{}  ", text(r.first()))));
    match text(r.get(1)).as_str() {
        "C" | "CASCADED" => out.push(Line::tab("WITH CASCADED CHECK OPTION")),
        "L" | "LOCAL" => out.push(Line::tab("WITH LOCAL CHECK OPTION")),
        _ => {}
    }
    let fmt = desc.formats.first().map(|f| f.format.name.to_string()).unwrap_or_default();
    out.push(Line::tab(if opts.system_names { format!("RCDFMT {fmt:<10} ;") } else { ";".into() }));
    out.push(Line::blank());
    if opts.labels {
        let label = ccsid::decode_trimmed(&desc.header.text);
        if !label.is_empty() {
            out.push(Line::top(format!("LABEL ON TABLE {me}")));
            out.push(Line::tab(format!("IS {} ;", literal(&label))));
            out.push(Line::blank());
        }
        column_labels(&fields, me, out);
    }
    if opts.privileges {
        grants(txn, ctx, o, me, true, out)?;
    }
    Ok(())
}

fn index(txn: &mut Txn<'_>, ctx: &mut SqlContext, o: &Object, opts: &Options, me: &str, out: &mut Vec<Line>) -> Result<()> {
    let p = [Value::Str(o.schema.clone()), Value::Str(o.name.clone())];
    let rows = query(
        txn,
        ctx,
        "SELECT TABLE_SCHEMA, TABLE_NAME, IS_UNIQUE, SEARCH_CONDITION, INDEX_TEXT FROM QSYS2.SYSINDEXES WHERE INDEX_SCHEMA = ? AND INDEX_NAME = ?",
        &p,
    )?;
    let r = rows.first().cloned().unwrap_or_default();
    let keys = query(
        txn,
        ctx,
        "SELECT COLUMN_NAME, ORDERING, COLUMN_IS_EXPRESSION, KEY_EXPRESSION FROM QSYS2.SYSKEYS WHERE INDEX_SCHEMA = ? AND INDEX_NAME = ? ORDER BY ORDINAL_POSITION",
        &p,
    )?;
    let keys: Vec<String> = keys
        .iter()
        .map(|k| {
            let col = if text(k.get(2)) == "Y" { text(k.get(3)) } else { ident(&text(k.first())) };
            format!("{col} {}", if text(k.get(1)) == "D" { "DESC" } else { "ASC" })
        })
        .collect();
    let unique = match text(r.get(2)).as_str() {
        "U" | "Y" => "UNIQUE ",
        "V" => "UNIQUE WHERE NOT NULL ",
        _ => "",
    };
    let verb = if opts.or_replace { "CREATE OR REPLACE" } else { "CREATE" };
    out.push(Line::top(format!("{verb} {unique}INDEX {me}")));
    out.push(Line::tab(format!("ON {} ( {} )  ", qual(opts, &text(r.first()), &text(r.get(1))), keys.join(" , "))));
    let cond = text(r.get(3));
    if !cond.is_empty() {
        out.push(Line::tab(format!("WHERE {cond}")));
    }
    out.push(Line::tab(";"));
    out.push(Line::blank());
    let label = text(r.get(4));
    if opts.labels && !label.is_empty() {
        out.push(Line::top(format!("LABEL ON INDEX {me}")));
        out.push(Line::tab(format!("IS {} ;", literal(&label))));
        out.push(Line::blank());
    }
    Ok(())
}

fn sequence(txn: &mut Txn<'_>, ctx: &mut SqlContext, o: &Object, opts: &Options, me: &str, out: &mut Vec<Line>) -> Result<()> {
    let rows = query(
        txn,
        ctx,
        "SELECT DATA_TYPE, START, INCREMENT, MINIMUM_VALUE, MAXIMUM_VALUE, CYCLE_OPTION, CACHE, \"ORDER\", SEQUENCE_DEFINER, SEQUENCE_TEXT, NUMERIC_PRECISION \
         FROM QSYS2.SYSSEQUENCES WHERE SEQUENCE_SCHEMA = ? AND SEQUENCE_NAME = ?",
        &[Value::Str(o.schema.clone()), Value::Str(o.name.clone())],
    )?;
    let r = rows.first().cloned().unwrap_or_default();
    let ty = match text(r.first()).as_str() {
        t @ ("DECIMAL" | "NUMERIC") => format!("{t}({}, 0)", num(r.get(10))),
        t => t.to_string(),
    };
    let verb = if opts.or_replace { "CREATE OR REPLACE" } else { "CREATE" };
    out.push(Line::top(format!("{verb} SEQUENCE {me}")));
    out.push(Line::tab(format!("AS {ty}")));
    out.push(Line::tab(format!("START WITH {}", text(r.get(1)))));
    out.push(Line::tab(format!("INCREMENT BY {}", text(r.get(2)))));
    out.push(Line::tab(format!("MINVALUE {}", text(r.get(3)))));
    out.push(Line::tab(format!("MAXVALUE {}", text(r.get(4)))));
    let cycle = if text(r.get(5)) == "YES" { "CYCLE" } else { "NO CYCLE" };
    let cache = match num(r.get(6)) {
        0 => "NO CACHE".to_string(),
        n => format!("CACHE {n}"),
    };
    let order = if text(r.get(7)) == "YES" { "ORDER" } else { "NO ORDER" };
    out.push(Line::tab(format!("{cycle} {cache} {order} ;")));
    out.push(Line::blank());
    let label = text(r.get(9));
    if opts.labels && !label.is_empty() {
        out.push(Line::top(format!("LABEL ON SEQUENCE {me}")));
        out.push(Line::tab(format!("IS {} ;", literal(&label))));
        out.push(Line::blank());
    }
    if opts.privileges {
        // The owner's privileges: a sequence's other users are not in
        // the catalog.
        out.push(Line::top("GRANT ALTER , USAGE  "));
        out.push(Line::top(format!("ON SEQUENCE {me}")));
        out.push(Line::top(format!("TO {} WITH GRANT OPTION ;", text(r.get(8)))));
        out.push(Line::blank());
    }
    Ok(())
}

fn alias(txn: &mut Txn<'_>, ctx: &mut SqlContext, o: &Object, opts: &Options, me: &str, out: &mut Vec<Line>) -> Result<()> {
    let id = ctx.resolve_file(txn, &object_name(o))?;
    let Some(def) = db::file::alias_of(txn, id)? else {
        return Err(SqlError::Database { what: format!("{} is not an alias", o.name), msgid: None });
    };
    let target = crate::exec::alias::target(&def);
    let schema = target.schema.as_ref().map(|s| s.value.clone()).unwrap_or_default();
    let mut to = qual(opts, &schema, &target.name.value);
    if let Some(m) = def.member {
        let _ = write!(to, " ( {m} )");
    }
    let verb = if opts.or_replace { "CREATE OR REPLACE" } else { "CREATE" };
    out.push(Line::top(format!("{verb} ALIAS {me}")));
    out.push(Line::tab(format!("FOR {to} ;")));
    out.push(Line::blank());
    Ok(())
}
