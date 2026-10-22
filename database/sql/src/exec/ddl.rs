// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE TABLE`, `CREATE INDEX` and `DROP`: the statement becomes DDS,
//! the DDS compiler checks it, the database creates the file.

use db::file::{self, CreateFileOpts, FileKind};
use dds::{CompileOpts, MapRefs, NoRefs, Source, compile_lf, compile_pf};
use qsname::Name;
use store::txn::Txn;

use super::SqlContext;
use crate::ast::{
    ColumnDef, ColumnDefault, Constraint, CreateIndex, CreateTable, DataType, Expr, Literal,
    ObjectName,
};
use crate::error::{Result, SqlError};

/// One DDS line: positional columns 6, 17, 19, 30-34, 35, 36-37 and 45.
fn dds(name_type: &str, name: &str, len: &str, ty: &str, dec: &str, kw: &str) -> String {
    let mut s = vec![' '; 80];
    let put = |s: &mut Vec<char>, col: usize, text: &str| {
        for (i, ch) in text.chars().enumerate() {
            if col - 1 + i < s.len() {
                s[col - 1 + i] = ch;
            }
        }
    };
    put(&mut s, 6, "A");
    put(&mut s, 17, name_type);
    put(&mut s, 19, name);
    put(&mut s, 35 - len.len(), len);
    put(&mut s, 35, ty);
    put(&mut s, 38 - dec.len(), dec);
    put(&mut s, 45, kw);
    s.into_iter().collect::<String>().trim_end().to_string()
}

/// A keyword line alone.
fn kw(k: &str) -> String {
    dds("", "", "", "", "", k)
}

/// The system name of an identifier: `FOR ... NAME` or the name itself.
fn sys_name(ident: &crate::ast::Ident, system: Option<&crate::ast::Ident>) -> Result<Name> {
    match system {
        Some(s) => SqlContext::sys_name(s),
        None => SqlContext::sys_name(ident),
    }
}

/// A DDS literal for `DFT`.
fn dds_literal(e: &Expr) -> Result<String> {
    Ok(match e {
        Expr::Literal(Literal::Str(s)) => format!("'{}'", s.replace('\'', "''")),
        Expr::Literal(Literal::Num(n)) => n.clone(),
        Expr::Literal(Literal::Null) => "*NULL".into(),
        Expr::Literal(Literal::Date(s) | Literal::Time(s) | Literal::Timestamp(s)) => {
            format!("'{s}'")
        }
        Expr::Neg(inner) => format!("-{}", dds_literal(inner)?),
        Expr::Register(_) => {
            return Err(SqlError::Unsupported {
                what: "a special register as a column default".into(),
            });
        }
        _ => {
            return Err(SqlError::Unsupported {
                what: "an expression as a column default".into(),
            });
        }
    })
}

/// The `DFT` of `WITH DEFAULT`: the type's own default.
fn system_default(t: &DataType) -> &'static str {
    match t {
        DataType::Decimal(..)
        | DataType::Numeric(..)
        | DataType::SmallInt
        | DataType::Integer
        | DataType::BigInt
        | DataType::Real
        | DataType::Double => "0",
        DataType::Date => "'0001-01-01'",
        DataType::Time => "'00.00.00'",
        DataType::Timestamp => "'0001-01-01-00.00.00.000000'",
        _ => "' '",
    }
}

/// The DDS field line(s) of a column.
fn column_lines(c: &ColumnDef) -> Result<Vec<String>> {
    if c.identity.is_some() {
        return Err(SqlError::Unsupported {
            what: "identity columns".into(),
        });
    }
    let name = sys_name(&c.name, c.system_name.as_ref())?;
    let (len, ty, dec, mut kws): (String, &str, String, Vec<String>) = match &c.type_spec.data_type
    {
        DataType::Char(n) => (n.to_string(), "A", String::new(), vec![]),
        DataType::Varchar(n) => (n.to_string(), "A", String::new(), vec!["VARLEN".into()]),
        DataType::Graphic(n) => (n.to_string(), "G", String::new(), vec![]),
        DataType::Vargraphic(n) => (n.to_string(), "G", String::new(), vec!["VARLEN".into()]),
        DataType::Decimal(p, s) => (p.to_string(), "P", s.to_string(), vec![]),
        DataType::Numeric(p, s) => (p.to_string(), "S", s.to_string(), vec![]),
        DataType::SmallInt => ("4".into(), "B", "0".into(), vec![]),
        DataType::Integer => ("9".into(), "B", "0".into(), vec![]),
        DataType::BigInt => ("18".into(), "B", "0".into(), vec![]),
        DataType::Real => ("9".into(), "F", "0".into(), vec!["FLTPCN(*SINGLE)".into()]),
        DataType::Double => ("17".into(), "F", "0".into(), vec!["FLTPCN(*DOUBLE)".into()]),
        DataType::Date => (
            String::new(),
            "L",
            String::new(),
            vec!["DATFMT(*ISO)".into()],
        ),
        DataType::Time => (
            String::new(),
            "T",
            String::new(),
            vec!["TIMFMT(*ISO)".into()],
        ),
        DataType::Timestamp => (String::new(), "Z", String::new(), vec![]),
    };
    if let Some(ccsid) = c.type_spec.ccsid {
        kws.push(format!("CCSID({ccsid})"));
    }
    if !c.not_null {
        kws.push("ALWNULL".into());
    }
    match &c.default {
        Some(ColumnDefault::Value(e)) => kws.push(format!("DFT({})", dds_literal(e)?)),
        Some(ColumnDefault::System) => {
            kws.push(format!("DFT({})", system_default(&c.type_spec.data_type)));
        }
        None => {}
    }
    if c.name.value != name.to_string() {
        kws.push(format!(
            "ALIAS({})",
            c.name.value.chars().take(30).collect::<String>()
        ));
    }
    let mut lines = Vec::new();
    let first = kws.first().cloned().unwrap_or_default();
    lines.push(dds("", &name.to_string(), &len, ty, &dec, &first));
    for k in kws.iter().skip(1) {
        lines.push(kw(k));
    }
    Ok(lines)
}

/// `CREATE TABLE`: a physical file from the generated DDS.
pub fn create_table(txn: &mut Txn<'_>, ctx: &mut SqlContext, t: &CreateTable) -> Result<()> {
    if t.as_query.is_some() {
        return Err(SqlError::Unsupported {
            what: "CREATE TABLE AS".into(),
        });
    }
    let name = sys_name(&t.name.name, t.system_name.as_ref())?;
    let library = ctx.library_for(txn, &t.name)?;
    let rcdfmt = match &t.rcdfmt {
        Some(r) => SqlContext::sys_name(r)?,
        None => {
            Name::parse(&format!("{name}R").chars().take(10).collect::<String>()).unwrap_or(name)
        }
    };
    // Keys: the primary key, from the table or a column.
    let mut keys: Vec<Name> = Vec::new();
    for c in &t.constraints {
        match c {
            Constraint::PrimaryKey { columns, .. } => {
                keys = columns
                    .iter()
                    .map(SqlContext::sys_name)
                    .collect::<Result<_>>()?;
            }
            Constraint::Unique { .. } => {
                return Err(SqlError::Unsupported {
                    what: "UNIQUE constraints; create a unique index instead".into(),
                });
            }
            Constraint::ForeignKey { .. } | Constraint::Check { .. } => {}
        }
    }
    for c in &t.columns {
        if c.primary_key {
            keys.push(sys_name(&c.name, c.system_name.as_ref())?);
        }
        if c.unique && !c.primary_key {
            return Err(SqlError::Unsupported {
                what: "UNIQUE on a column; create a unique index instead".into(),
            });
        }
    }
    let mut lines = Vec::new();
    if !keys.is_empty() {
        lines.push(kw("UNIQUE"));
    }
    lines.push(dds("R", &rcdfmt.to_string(), "", "", "", ""));
    for c in &t.columns {
        lines.extend(column_lines(c)?);
    }
    for k in &keys {
        lines.push(dds("K", &k.to_string(), "", "", "", ""));
    }
    let (source, _) = Source::from_text(&lines.join("\n"));
    let comp = compile_pf(&source, &NoRefs, &CompileOpts::default());
    let Some(def) = comp.definition else {
        return Err(SqlError::Database {
            what: format!("the table definition does not compile:\n{}", comp.listing),
            msgid: None,
        });
    };
    file::create_pf(
        txn,
        library,
        name,
        &def,
        &CreateFileOpts {
            now: ctx.clock(),
            text: format!(
                "SQL table {}",
                t.name.name.value.chars().take(40).collect::<String>()
            ),
            ..CreateFileOpts::default()
        },
    )?;
    Ok(())
}

/// `CREATE INDEX`: a keyed logical file over the table.
pub fn create_index(txn: &mut Txn<'_>, ctx: &mut SqlContext, i: &CreateIndex) -> Result<()> {
    let table = ctx.resolve_table(txn, &i.table)?;
    if table.desc.attr.kind == FileKind::Logical {
        return Err(SqlError::Unsupported {
            what: "an index over a view or another index".into(),
        });
    }
    let name = SqlContext::sys_name(&i.name.name)?;
    let library = ctx.library_for(txn, &i.name)?;
    let rcdfmt = match &i.rcdfmt {
        Some(r) => SqlContext::sys_name(r)?,
        None => table.format.format.name,
    };
    let lib_name = table.desc.library_name;
    let file_name = table.desc.name();
    let mut lines = Vec::new();
    if i.unique {
        lines.push(kw(if i.where_not_null {
            "UNIQUE(*EXCNULL)"
        } else {
            "UNIQUE"
        }));
    }
    lines.push(dds(
        "R",
        &rcdfmt.to_string(),
        "",
        "",
        "",
        &format!("PFILE({lib_name}/{file_name})"),
    ));
    if rcdfmt != table.format.format.name {
        lines.push(kw(&format!("FORMAT({lib_name}/{file_name})")));
    }
    for (c, desc) in &i.columns {
        let col = SqlContext::sys_name(c)?;
        if table.format.format.field(&col).is_none() {
            return Err(SqlError::ColumnNotFound {
                name: c.value.clone(),
            });
        }
        lines.push(dds(
            "K",
            &col.to_string(),
            "",
            "",
            "",
            if *desc { "DESCEND" } else { "" },
        ));
    }
    let mut refs = MapRefs::default();
    refs.insert(
        lib_name,
        file_name,
        table
            .desc
            .formats
            .iter()
            .map(|f| f.format.clone())
            .collect(),
    );
    let (source, _) = Source::from_text(&lines.join("\n"));
    let comp = compile_lf(&source, &refs, &CompileOpts::default());
    let Some(def) = comp.definition else {
        return Err(SqlError::Database {
            what: format!("the index definition does not compile:\n{}", comp.listing),
            msgid: None,
        });
    };
    let base = table.id;
    file::create_lf(
        txn,
        library,
        name,
        &def,
        &mut |_| Ok(base),
        &CreateFileOpts {
            now: ctx.clock(),
            text: format!(
                "SQL index {}",
                i.name.name.value.chars().take(40).collect::<String>()
            ),
            ..CreateFileOpts::default()
        },
    )?;
    Ok(())
}

/// `DROP TABLE`, `DROP INDEX`, `DROP VIEW`: the file and, for a table,
/// the indexes and views over it.
pub fn drop(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    name: &ObjectName,
    _cascade: bool,
) -> Result<()> {
    let id = ctx.resolve_file(txn, name)?;
    let desc = file::describe(txn, id)?;
    let mut dependents: Vec<_> = desc.dependents.iter().map(|d| d.lf_file).collect();
    dependents.sort();
    dependents.dedup();
    for lf in dependents {
        if file::describe(txn, lf).is_ok() {
            file::delete_file(txn, ctx.job, lf)?;
        }
    }
    file::delete_file(txn, ctx.job, id)?;
    Ok(())
}

/// A DDS literal for `COMP`, `RANGE` and `VALUES`.
fn selomit_literal(e: &Expr) -> Result<String> {
    match e {
        Expr::Literal(Literal::Str(s)) => Ok(format!("'{}'", s.replace('\'', "''"))),
        Expr::Literal(Literal::Num(n)) => Ok(n.clone()),
        Expr::Neg(inner) => Ok(format!("-{}", selomit_literal(inner)?)),
        _ => Err(SqlError::Unsupported {
            what: "a view condition that is not column against literal".into(),
        }),
    }
}

/// The `S` lines of a view's `WHERE`: `AND`ed comparisons of a column
/// with a literal, `BETWEEN` and `IN` lists.
fn select_lines(
    e: &Expr,
    out: &mut Vec<String>,
    renames: &std::collections::HashMap<Name, Name>,
) -> Result<()> {
    use crate::ast::CmpOp;
    match e {
        Expr::And(a, b) => {
            select_lines(a, out, renames)?;
            select_lines(b, out, renames)
        }
        Expr::Compare(a, op, b, None) => {
            let Expr::Column(c) = &**a else {
                return Err(SqlError::Unsupported {
                    what: "a view condition with an expression on the left".into(),
                });
            };
            let name = SqlContext::sys_name(&c.column)?;
            let name = renames.get(&name).copied().unwrap_or(name);
            let word = match op {
                CmpOp::Eq => "EQ",
                CmpOp::Ne => "NE",
                CmpOp::Lt => "LT",
                CmpOp::Le => "LE",
                CmpOp::Gt => "GT",
                CmpOp::Ge => "GE",
            };
            out.push(dds(
                "S",
                &name.to_string(),
                "",
                "",
                "",
                &format!("COMP({word} {})", selomit_literal(b)?),
            ));
            Ok(())
        }
        Expr::Between(a, lo, hi, false) => {
            let Expr::Column(c) = &**a else {
                return Err(SqlError::Unsupported {
                    what: "a view condition with an expression on the left".into(),
                });
            };
            let name = SqlContext::sys_name(&c.column)?;
            let name = renames.get(&name).copied().unwrap_or(name);
            out.push(dds(
                "S",
                &name.to_string(),
                "",
                "",
                "",
                &format!("RANGE({} {})", selomit_literal(lo)?, selomit_literal(hi)?),
            ));
            Ok(())
        }
        Expr::InList(a, list, false) => {
            let Expr::Column(c) = &**a else {
                return Err(SqlError::Unsupported {
                    what: "a view condition with an expression on the left".into(),
                });
            };
            let name = SqlContext::sys_name(&c.column)?;
            let name = renames.get(&name).copied().unwrap_or(name);
            let vals = list
                .iter()
                .map(selomit_literal)
                .collect::<Result<Vec<_>>>()?;
            out.push(dds(
                "S",
                &name.to_string(),
                "",
                "",
                "",
                &format!("VALUES({})", vals.join(" ")),
            ));
            Ok(())
        }
        _ => Err(SqlError::Unsupported {
            what: "a view condition DDS select/omit cannot express".into(),
        }),
    }
}

/// `CREATE VIEW`: a logical file when the query is a projection, with
/// renames and a simple `WHERE`, over one table.
#[allow(clippy::too_many_lines)]
pub fn create_view(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    v: &crate::ast::CreateView,
) -> Result<()> {
    use crate::ast::{QueryBody, SelectItem, TableRef};
    let QueryBody::Select(sel) = &v.query.body else {
        return Err(SqlError::Unsupported {
            what: "a view over VALUES or UNION".into(),
        });
    };
    if sel.from.len() != 1
        || !sel.group_by.is_empty()
        || sel.distinct
        || sel.having.is_some()
        || !v.query.order_by.is_empty()
    {
        return Err(SqlError::Unsupported {
            what: "a view that is not a plain selection from one table".into(),
        });
    }
    let TableRef::Table { name: tname, .. } = &sel.from[0] else {
        return Err(SqlError::Unsupported {
            what: "a view over a join or a derived table".into(),
        });
    };
    let table = ctx.resolve_table(txn, tname)?;
    let name = SqlContext::sys_name(&v.name.name)?;
    let library = ctx.library_for(txn, &v.name)?;
    let lib_name = table.desc.library_name;
    let file_name = table.desc.name();
    let mut lines = vec![dds(
        "R",
        &table.format.format.name.to_string(),
        "",
        "",
        "",
        &format!("PFILE({lib_name}/{file_name})"),
    )];
    let mut n = 0;
    let mut renames: std::collections::HashMap<Name, Name> = std::collections::HashMap::new();
    for item in &sel.items {
        match item {
            SelectItem::Star | SelectItem::TableStar(_) => {}
            SelectItem::Expr(Expr::Column(c), alias) => {
                let base = SqlContext::sys_name(&c.column)?;
                if table.format.format.field(&base).is_none() {
                    return Err(SqlError::ColumnNotFound {
                        name: c.column.value.clone(),
                    });
                }
                let out_name = match (v.columns.get(n), alias) {
                    (Some(col), _) => SqlContext::sys_name(col)?,
                    (None, Some(a)) => SqlContext::sys_name(a)?,
                    (None, None) => base,
                };
                if out_name == base {
                    lines.push(dds("", &base.to_string(), "", "", "", ""));
                } else {
                    renames.insert(base, out_name);
                    lines.push(dds(
                        "",
                        &out_name.to_string(),
                        "",
                        "",
                        "",
                        &format!("RENAME({base})"),
                    ));
                }
                n += 1;
            }
            SelectItem::Expr(..) => {
                return Err(SqlError::Unsupported {
                    what: "a view column that is an expression".into(),
                });
            }
        }
    }
    if let Some(w) = &sel.where_clause {
        // DDS tests a select/omit field only when the format holds it.
        let projected: Vec<String> = sel
            .items
            .iter()
            .filter_map(|i| match i {
                SelectItem::Expr(Expr::Column(c), _) => Some(c.column.value.clone()),
                _ => None,
            })
            .collect();
        let all = sel
            .items
            .iter()
            .any(|i| matches!(i, SelectItem::Star | SelectItem::TableStar(_)));
        if !all
            && let Some(c) = crate::ast::columns_of(w)
                .iter()
                .find(|c| !projected.contains(&c.column.value))
        {
            return Err(SqlError::Unsupported {
                what: format!(
                    "a view condition on {}, a column the view does not select",
                    c.column.value
                ),
            });
        }
        select_lines(w, &mut lines, &renames)?;
    }
    let mut refs = MapRefs::default();
    refs.insert(
        lib_name,
        file_name,
        table
            .desc
            .formats
            .iter()
            .map(|f| f.format.clone())
            .collect(),
    );
    let (source, _) = Source::from_text(&lines.join("\n"));
    let comp = compile_lf(&source, &refs, &CompileOpts::default());
    let Some(def) = comp.definition else {
        return Err(SqlError::Database {
            what: format!("the view definition does not compile:\n{}", comp.listing),
            msgid: None,
        });
    };
    let base_id = table.id;
    file::create_lf(
        txn,
        library,
        name,
        &def,
        &mut |_| Ok(base_id),
        &CreateFileOpts {
            now: ctx.clock(),
            text: format!(
                "SQL view {}",
                v.name.name.value.chars().take(41).collect::<String>()
            ),
            ..CreateFileOpts::default()
        },
    )?;
    Ok(())
}

/// `LABEL ON` and `COMMENT ON`: the object's text, or a column's
/// heading or text.
pub fn label(txn: &mut Txn<'_>, ctx: &mut SqlContext, l: &crate::ast::Label) -> Result<()> {
    use crate::ast::LabelTarget;
    let (name, column) = match &l.target {
        LabelTarget::Table(n) | LabelTarget::Index(n) | LabelTarget::Alias(n) => (n, None),
        LabelTarget::Column(n, c) => (n, Some(c)),
    };
    let id = ctx.resolve_file(txn, name)?;
    db::auth::check(txn, "file.change", id)?;
    if column.is_none() && l.columns.is_empty() {
        let text = l.text.clone().unwrap_or_default();
        store::obj::ops::change_text(
            txn,
            id,
            &text.chars().take(50).collect::<String>(),
            ctx.clock(),
        )?;
        return Ok(());
    }
    let desc = file::describe(txn, id)?;
    let mut formats = desc.formats.clone();
    let mut changes: Vec<(&crate::ast::Ident, &str, bool)> = Vec::new();
    if let Some(c) = column {
        changes.push((c, l.text.as_deref().unwrap_or(""), l.is_text));
    }
    for (c, t, is_text) in &l.columns {
        changes.push((c, t, *is_text));
    }
    for (c, text, is_text) in changes {
        let cname = SqlContext::sys_name(c)?;
        let Some(f) = formats
            .iter_mut()
            .flat_map(|fm| fm.format.fields.iter_mut())
            .find(|f| f.name == cname)
        else {
            return Err(SqlError::ColumnNotFound {
                name: c.value.clone(),
            });
        };
        if is_text {
            f.text = Some(text.chars().take(50).collect());
        } else {
            // A heading is up to three parts of twenty characters.
            let chars: Vec<char> = text.chars().collect();
            f.colhdg = chars
                .chunks(20)
                .take(3)
                .map(|ch| ch.iter().collect::<String>().trim_end().to_string())
                .collect();
        }
    }
    file::replace_formats(txn, id, &formats)?;
    Ok(())
}
