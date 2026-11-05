// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE [UNIQUE [WHERE NOT NULL]] [ENCODED VECTOR] INDEX`: a keyed
//! logical file over the table, its SQL name and attributes in its
//! `*SQLINF`. A sparse index's `WHERE` is the file's select/omit rules,
//! so it holds only the rows the condition keeps. An encoded vector
//! index is an ordinary index here; its `INCLUDE` aggregates are kept
//! for the catalog and used by nothing.

use db::file::FileKind;
use db::sqlinf::body::{IndexDef, IndexKey};
use db::sqlinf::{self, Kind, tag};
use dds::{CompileOpts, MapRefs, Source, compile_lf};
use store::obj::ObjectId;
use store::txn::Txn;

use super::ddl::{dds, kw, select_lines};
use super::{SqlContext, names};
use crate::ast::CreateIndex;
use crate::error::{Result, SqlError};

/// `CREATE INDEX`.
pub fn create(txn: &mut Txn<'_>, ctx: &mut SqlContext, i: &CreateIndex) -> Result<()> {
    let table = ctx.resolve_table(txn, &i.table)?;
    if table.desc.attr.kind == FileKind::Logical {
        return Err(SqlError::Unsupported { what: "an index over a view or another index".into() });
    }
    let library = ctx.library_for(txn, &i.name)?;
    let (name, sql) = names::new_object(txn, library, &i.name.name, None)?;
    let rcdfmt = match &i.rcdfmt {
        Some(r) => SqlContext::sys_name(r)?,
        None => table.format.format.name,
    };
    let lib_name = table.desc.library_name;
    let file_name = table.desc.name();
    let mut lines = Vec::new();
    if i.unique {
        lines.push(kw(if i.where_not_null { "UNIQUE(*EXCNULL)" } else { "UNIQUE" }));
    }
    lines.push(dds("R", &rcdfmt.to_string(), "", "", "", &format!("PFILE({lib_name}/{file_name})")));
    if rcdfmt != table.format.format.name {
        lines.push(kw(&format!("FORMAT({lib_name}/{file_name})")));
    }
    let mut keys = Vec::with_capacity(i.columns.len());
    for (c, desc) in &i.columns {
        let Some((_, f)) = table.field(&c.value) else {
            return Err(SqlError::ColumnNotFound { name: c.value.clone() });
        };
        if f.data_type.is_lob() {
            return Err(SqlError::Unsupported { what: format!("the LOB column {} as a key", f.name) });
        }
        lines.push(dds("K", &f.name.to_string(), "", "", "", if *desc { "DESCEND" } else { "" }));
        keys.push(IndexKey { expression: false, descend: *desc, text: c.value.clone() });
    }
    if let Some((cond, _)) = &i.sparse {
        // The condition's columns by their system names, as DDS has them.
        let renames = std::collections::HashMap::new();
        let cond = system_columns(cond, &table);
        select_lines(&cond, &mut lines, &renames).map_err(|_| SqlError::Unsupported {
            what: "a sparse index condition that is not comparisons of columns with constants".into(),
        })?;
    }
    let mut refs = MapRefs::default();
    refs.insert(lib_name, file_name, table.desc.formats.iter().map(|f| f.format.clone()).collect());
    let (source, _) = Source::from_text(&lines.join("\n"));
    let comp = compile_lf(&source, &refs, &CompileOpts::default());
    let Some(def) = comp.definition else {
        return Err(SqlError::Database { what: format!("the index definition does not compile:\n{}", comp.listing), msgid: None });
    };
    let id = super::ddl::build_index(txn, ctx, &table, library, name, &def, "")?;
    let mut info = names::record(txn, library, id, Kind::Index, sql.as_deref(), ctx.clock())?;
    let idx = IndexDef {
        where_not_null: i.where_not_null,
        encoded_vector: i.encoded_vector,
        // IBM keeps the condition with one blank after it.
        where_text: i.sparse.as_ref().map(|(_, t)| format!("{t} ")).unwrap_or_default(),
        keys,
        includes: i.includes.clone(),
    };
    info.set(tag::INDEX, idx.encode());
    sqlinf::save(txn, id, &info, ctx.clock())?;
    Ok(())
}

/// A condition with its column names as the table's system names.
fn system_columns(e: &crate::ast::Expr, table: &super::TableInfo) -> crate::ast::Expr {
    use crate::ast::Expr;
    let col = |c: &crate::ast::ColumnRef| -> Expr {
        let mut c = c.clone();
        if let Some((_, f)) = table.field(&c.column.value) {
            c.column = crate::ast::Ident::new(&f.name.to_string());
        }
        Expr::Column(c)
    };
    match e {
        Expr::Column(c) => col(c),
        Expr::And(a, b) => Expr::And(Box::new(system_columns(a, table)), Box::new(system_columns(b, table))),
        Expr::Compare(a, op, b, q) => Expr::Compare(Box::new(system_columns(a, table)), *op, Box::new(system_columns(b, table)), *q),
        Expr::Between(a, lo, hi, not) => Expr::Between(Box::new(system_columns(a, table)), lo.clone(), hi.clone(), *not),
        Expr::InList(a, l, not) => Expr::InList(Box::new(system_columns(a, table)), l.clone(), *not),
        other => other.clone(),
    }
}

/// An index's SQL attributes, `None` for a file that has none.
pub fn definition(txn: &mut Txn<'_>, id: ObjectId) -> Option<IndexDef> {
    let info = sqlinf::load(txn, id).ok().flatten()?;
    IndexDef::decode(info.first(tag::INDEX)?).ok()
}
