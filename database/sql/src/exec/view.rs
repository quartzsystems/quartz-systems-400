// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL views. A view the select/omit rules of a logical file can say is
//! a logical file, as before. Any other is a `*FILE` without members
//! that keeps its query text in a `VIEW` entry: a query naming it runs
//! that query as a derived table in its place (`expand`).

use db::file::{self, CreateFileOpts, FileKind, MemberOpt};
use db::sqlinf::body::{CheckOption, Dependency, ViewDef};
use db::sqlinf::{self, Kind, tag};
use qsname::Name;
use store::obj::{ObjectId, ops, types};
use store::txn::Txn;

use super::{SqlContext, names};
use crate::ast::{CreateView, Ident, Naming, ObjectName, Query, QueryBody, TableRef};
use crate::error::{Result, SqlError};

/// `CREATE [OR REPLACE] VIEW`.
pub fn create(txn: &mut Txn<'_>, ctx: &mut SqlContext, v: &CreateView) -> Result<()> {
    let library = ctx.library_for(txn, &v.name)?;
    if v.replace
        && let Ok(old) = ctx.resolve_file(txn, &v.name)
    {
        if !is_view(txn, old) {
            return Err(SqlError::Exists { name: v.name.name.value.clone(), kind: "*FILE" });
        }
        if !dependents(txn, ctx, old)?.is_empty() {
            // BLOCKED(rel): IBM keeps views over a replaced view when
            // they still fit; that waits on view revalidation.
            return Err(SqlError::Unsupported { what: "CREATE OR REPLACE VIEW of a view other views depend on".into() });
        }
        file::delete_file(txn, ctx.job, old)?;
    }
    if v.rcdfmt.is_some() {
        return Err(SqlError::Unsupported { what: "RCDFMT on CREATE VIEW".into() });
    }
    let (name, sql) = names::new_object(txn, library, &v.name.name, v.system_name.as_ref())?;
    let deps = dependencies(txn, ctx, &v.query)?;
    // Only a view in `QTEMP` may read what is in it (`SQL7008`).
    if Some(library) != ctx.qtemp_library(txn) && deps.iter().any(|d| super::qtemp::is_qtemp(d.library)) {
        return Err(SqlError::Coded { code: -7008, state: "55019", what: format!("{} reads a table in QTEMP; only a view in QTEMP may", v.name.name.value) });
    }
    // A simple view stays a logical file (RPG opens it); any other runs
    // its query.
    let over_physical = match &v.query.body {
        QueryBody::Select(s) => match s.from.as_slice() {
            [TableRef::Table { name: t, .. }] => ctx
                .resolve_file(txn, t)
                .ok()
                .filter(|&id| query_view(txn, id).is_none())
                .and_then(|id| file::describe(txn, id).ok())
                .is_some_and(|d| d.attr.kind == FileKind::Physical && !d.members.is_empty()),
            _ => false,
        },
        _ => false,
    };
    let lf = if over_physical { super::ddl::create_view(txn, ctx, v, library, name) } else { Err(SqlError::Unsupported { what: String::new() }) };
    let (id, columns, logical, typed) = match lf {
        Ok(id) => {
            let d = file::describe(txn, id)?;
            let fields: Vec<Name> = d.formats.first().map(|f| f.format.fields.iter().map(|f| f.name).collect()).unwrap_or_default();
            let cols = fields.iter().map(ToString::to_string).collect();
            // The columns of a distinct type keep it from the table.
            let mut typed = Vec::new();
            if let QueryBody::Select(s) = &v.query.body
                && let [TableRef::Table { name: t, .. }] = s.from.as_slice()
                && let Ok(base) = ctx.resolve_file(txn, t)
            {
                for c in super::udt::column::of(txn, base)? {
                    if fields.contains(&c.field) {
                        typed.push((c.field, super::udt::column::qualified(&c)));
                    }
                }
            }
            (id, cols, true, typed)
        }
        Err(SqlError::Unsupported { .. }) => {
            let (id, cols, typed) = shell(txn, ctx, v, library, name)?;
            (id, cols, false, typed)
        }
        Err(e) => return Err(e),
    };
    let info = names::record(txn, library, id, Kind::View, sql.as_deref(), ctx.clock())?;
    let mut info = info;
    let def = ViewDef {
        check: match (v.check_option, v.local) {
            (false, _) => CheckOption::None,
            (true, false) => CheckOption::Cascaded,
            (true, true) => CheckOption::Local,
        },
        read_only: !logical,
        sys_naming: ctx.naming == Naming::Sys,
        // IBM keeps the text with one blank after it.
        query: format!("{} ", v.query_text),
        columns,
    };
    if v.check_option && !logical {
        // BLOCKED(rel): a view that is not a logical file is read only, so
        // its check option could never act; IBM's updatable forms of it
        // (expressions over one table) wait on updates through them.
        file::delete_file(txn, ctx.job, id)?;
        return Err(SqlError::Unsupported { what: "WITH CHECK OPTION on a view that is not a plain selection from one table".into() });
    }
    info.set(tag::VIEW, def.encode());
    info.remove(tag::DEPENDENCY);
    for d in deps {
        info.push(tag::DEPENDENCY, d.encode());
    }
    sqlinf::save(txn, id, &info, ctx.clock())?;
    super::udt::column::record(txn, id, &typed, Kind::View, ctx.clock())?;
    Ok(())
}

/// A view's columns of a distinct type: field, `SCHEMA.NAME`.
type Typed = Vec<(Name, String)>;

/// Whether a file is an SQL view: its kind, or for one made before SQL
/// kept kinds, its text.
pub fn is_view(txn: &mut Txn<'_>, id: ObjectId) -> bool {
    match names::kind_of(txn, id) {
        Some(k) => k == Kind::View,
        None => file::describe(txn, id).is_ok_and(|d| {
            d.attr.kind == FileKind::Logical && ccsid::decode_trimmed(&d.header.text).starts_with("SQL view")
        }),
    }
}

/// The view's `VIEW` entry and dependencies, `None` for any other file.
pub fn definition(txn: &mut Txn<'_>, id: ObjectId) -> Option<(ViewDef, Vec<Dependency>)> {
    let info = sqlinf::load(txn, id).ok().flatten().filter(|i| i.kind == Kind::View)?;
    let def = ViewDef::decode(info.first(tag::VIEW)?).ok()?;
    let deps = info.all(tag::DEPENDENCY).filter_map(|b| Dependency::decode(b).ok()).collect();
    Some((def, deps))
}

/// The view that runs its query, by file: `None` for a logical-file view
/// and every other file.
fn query_view(txn: &mut Txn<'_>, id: ObjectId) -> Option<(ViewDef, Vec<Dependency>)> {
    let (_, h) = ops::read_header(txn, id).ok()?;
    if h.sql.is_none() {
        return None;
    }
    let d = definition(txn, id)?;
    (!file::describe(txn, id).ok()?.attr.kind.eq(&FileKind::Logical)).then_some(d)
}

/// The `*FILE` of a view that runs its query: a physical file without
/// members whose fields are the view's columns.
fn shell(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    v: &CreateView,
    library: ObjectId,
    name: Name,
) -> Result<(ObjectId, Vec<String>, Typed)> {
    let (cols, _) = super::query::columns_and_rows_planned(txn, ctx, &[], &v.query)?;
    // A union's column has a name only where every side names it alike.
    if v.columns.is_empty() && !union_names_agree(&v.query.body) {
        return Err(SqlError::ColumnListRequired);
    }
    if !v.columns.is_empty() && v.columns.len() != cols.len() {
        return Err(SqlError::ValueCount { what: format!("{} column names for {} columns", v.columns.len(), cols.len()) });
    }
    let mut sql_names = Vec::with_capacity(cols.len());
    let mut fields = Vec::with_capacity(cols.len());
    for (i, c) in cols.iter().enumerate() {
        let sql = match v.columns.get(i) {
            Some(n) => n.value.clone(),
            None if c.name.chars().all(|ch| ch.is_ascii_digit()) => return Err(SqlError::ColumnListRequired),
            None => match &c.field {
                Some(f) if f.name.to_string() == c.name => f.alias.clone().unwrap_or_else(|| c.name.clone()),
                _ => c.name.clone(),
            },
        };
        if sql_names.contains(&sql) {
            return Err(SqlError::ColumnListRequired);
        }
        // BLOCKED(rel): an expression column has no type until the
        // executor types expressions (SQL-3, SQL-4); the catalog shows it
        // as VARCHAR(256) meanwhile. Its values are right.
        let mut f = c.field.clone().unwrap_or_else(|| {
            let mut f = dds::FieldDef::new(Name::parse("COL").expect("valid"), dds::DataType::Char, 256, 0);
            f.varlen = Some(dds::VarLen { allocated: None });
            f
        });
        f.allow_null = c.nullable;
        f.identity = None;
        f.usage = None;
        f.reference = None;
        f.line = 0;
        sql_names.push(sql);
        fields.push(f);
    }
    let sys: Vec<Name> = {
        let mut taken: Vec<Name> = Vec::new();
        sql_names
            .iter()
            .map(|s| {
                let n = Name::parse(s).ok().filter(|n| n.to_string() == *s && !taken.contains(n)).unwrap_or_else(|| names::generate_column(s, &|n| taken.contains(n)));
                taken.push(n);
                n
            })
            .collect()
    };
    for ((f, s), n) in fields.iter_mut().zip(&sql_names).zip(&sys) {
        f.name = *n;
        f.alias = (n.to_string() != *s).then(|| s.clone());
    }
    let typed = super::udt::column::typed(&fields);
    let mut rf = dds::RecordFormat::new(names::format_name(name));
    rf.fields = fields;
    rf.lay_out();
    let def = dds::PfDefinition { format: rf, unique: None, dup_order: None, reference: None, refaccpth: None, altseq: None, ccsid: None, text: None };
    let id = file::create_pf(
        txn,
        library,
        name,
        &def,
        &CreateFileOpts { now: ctx.clock(), text: String::new(), member: MemberOpt::None, ..CreateFileOpts::default() },
    )?;
    Ok((id, sql_names, typed))
}

/// Every table, view and alias the query reads in its `FROM` lists, by
/// library and system name.
fn dependencies(txn: &mut Txn<'_>, ctx: &SqlContext, q: &Query) -> Result<Vec<Dependency>> {
    let mut refs = Vec::new();
    from_names(&q.body, &mut refs);
    let mut out: Vec<Dependency> = Vec::new();
    for r in refs {
        let Ok(id) = ctx.resolve_file(txn, &r) else {
            // A catalog view: nothing on the system to depend on.
            continue;
        };
        let (_, h) = ops::read_header(txn, id)?;
        let (_, lh) = ops::read_header(txn, h.library)?;
        let d = Dependency { type_code: types::FILE, library: lh.name, name: h.name };
        if !out.contains(&d) {
            out.push(d);
        }
    }
    Ok(out)
}

/// The table names of a query's `FROM` trees, derived tables included.
fn from_names(b: &QueryBody, out: &mut Vec<ObjectName>) {
    fn walk(t: &TableRef, out: &mut Vec<ObjectName>) {
        match t {
            TableRef::Table { name, .. } => out.push(name.clone()),
            TableRef::Subquery { query, .. } => from_names(&query.body, out),
            TableRef::Join { left, right, .. } | TableRef::JoinUsing { left, right, .. } => {
                walk(left, out);
                walk(right, out);
            }
            TableRef::Lateral(inner) => walk(inner, out),
            TableRef::Recursive { cte, .. } => cte.seed.iter().chain(&cte.step).for_each(|b| from_names(b, out)),
            TableRef::Function { .. } | TableRef::DataChange { .. } => {}
        }
    }
    match b {
        QueryBody::Select(s) => s.from.iter().for_each(|t| walk(t, out)),
        QueryBody::Union { left, right, .. } => {
            from_names(left, out);
            from_names(right, out);
        }
        QueryBody::Nested(q) => from_names(&q.body, out),
        QueryBody::Values(_) => {}
    }
}

/// The query with every view that runs its query, named in its `FROM`
/// lists, put in as a derived table; `None` when it names none.
pub fn expand(txn: &mut Txn<'_>, ctx: &SqlContext, q: &Query) -> Result<Option<Query>> {
    let mut out = q.clone();
    if expand_body(txn, ctx, &mut out.body)? { Ok(Some(out)) } else { Ok(None) }
}

fn expand_body(txn: &mut Txn<'_>, ctx: &SqlContext, b: &mut QueryBody) -> Result<bool> {
    match b {
        QueryBody::Select(s) => {
            let mut any = false;
            for t in &mut s.from {
                any |= expand_ref(txn, ctx, t)?;
            }
            Ok(any)
        }
        QueryBody::Union { left, right, .. } => Ok(expand_body(txn, ctx, left)? | expand_body(txn, ctx, right)?),
        QueryBody::Nested(q) => expand_body(txn, ctx, &mut q.body),
        QueryBody::Values(_) => Ok(false),
    }
}

fn expand_ref(txn: &mut Txn<'_>, ctx: &SqlContext, t: &mut TableRef) -> Result<bool> {
    match t {
        TableRef::Table { name, alias, columns } => {
            let Ok(id) = ctx.resolve_file(txn, name) else {
                return Ok(false);
            };
            let Some((def, deps)) = query_view(txn, id) else {
                return Ok(false);
            };
            let naming = if def.sys_naming { Naming::Sys } else { Naming::Sql };
            let crate::ast::Statement::Select(mut query) = crate::parse::parse(&def.query, naming)? else {
                return Err(SqlError::Database { what: format!("view {name} keeps a query that is not one"), msgid: None });
            };
            qualify(txn, ctx, &mut query.body, &deps);
            let cols = if columns.is_empty() { def.columns.iter().map(|c| Ident { value: c.clone(), delimited: true }).collect() } else { columns.clone() };
            let alias = alias.clone().unwrap_or_else(|| name.name.clone());
            *t = TableRef::Subquery { query: Box::new(query), alias, columns: cols };
            Ok(true)
        }
        TableRef::Join { left, right, .. } | TableRef::JoinUsing { left, right, .. } => {
            Ok(expand_ref(txn, ctx, left)? | expand_ref(txn, ctx, right)?)
        }
        TableRef::Lateral(inner) => expand_ref(txn, ctx, inner),
        TableRef::Recursive { cte, .. } => {
            let mut any = false;
            for b in cte.seed.iter_mut().chain(cte.step.iter_mut()) {
                any |= expand_body(txn, ctx, b)?;
            }
            Ok(any)
        }
        TableRef::Subquery { .. } | TableRef::Function { .. } | TableRef::DataChange { .. } => Ok(false),
    }
}

/// An identifier for a system name.
pub fn ident_of(n: Name) -> Ident {
    let s = n.to_string();
    match s.strip_prefix('"').and_then(|x| x.strip_suffix('"')) {
        Some(inner) => Ident { value: inner.to_string(), delimited: true },
        None => Ident { value: s, delimited: false },
    }
}

/// Unqualified names in a view's `FROM` lists get the library they had
/// when the view was made: the dependency of that SQL or system name.
fn qualify(txn: &mut Txn<'_>, ctx: &SqlContext, b: &mut QueryBody, deps: &[Dependency]) {
    fn walk(txn: &mut Txn<'_>, ctx: &SqlContext, t: &mut TableRef, deps: &[Dependency]) {
        match t {
            TableRef::Table { name, .. } if name.schema.is_none() => {
                for d in deps {
                    let Some(found) = super::qtemp::find_library(txn, ctx, d.library) else { continue };
                    let Ok(lib) = store::obj::library::Library::load(txn, found) else { continue };
                    let Ok(Some(id)) = lib.lookup(txn, d.type_code, d.name) else { continue };
                    let sql = names::sql_name(txn, id);
                    if sql == name.name.value || d.name.to_string() == name.name.value {
                        name.schema = Some(ident_of(d.library));
                        name.name = ident_of(d.name);
                        break;
                    }
                }
            }
            TableRef::Subquery { query, .. } => qualify(txn, ctx, &mut query.body, deps),
            TableRef::Join { left, right, .. } | TableRef::JoinUsing { left, right, .. } => {
                walk(txn, ctx, left, deps);
                walk(txn, ctx, right, deps);
            }
            TableRef::Lateral(inner) => walk(txn, ctx, inner, deps),
            TableRef::Recursive { cte, .. } => {
                for b in cte.seed.iter_mut().chain(cte.step.iter_mut()) {
                    qualify(txn, ctx, b, deps);
                }
            }
            TableRef::Table { .. } | TableRef::Function { .. } | TableRef::DataChange { .. } => {}
        }
    }
    match b {
        QueryBody::Select(s) => {
            for t in &mut s.from {
                walk(txn, ctx, t, deps);
            }
        }
        QueryBody::Union { left, right, .. } => {
            qualify(txn, ctx, left, deps);
            qualify(txn, ctx, right, deps);
        }
        QueryBody::Nested(q) => qualify(txn, ctx, &mut q.body, deps),
        QueryBody::Values(_) => {}
    }
}

/// The views that depend on `id` (by their `DEPENDENCY` entries), in
/// every library.
pub fn dependents(txn: &mut Txn<'_>, ctx: &SqlContext, id: ObjectId) -> Result<Vec<ObjectId>> {
    let (_, h) = ops::read_header(txn, id)?;
    let (_, lh) = ops::read_header(txn, h.library)?;
    let me = (lh.name, h.name);
    let mut out = Vec::new();
    let mut libs: Vec<ObjectId> = ops::list(txn, ctx.qsys, Some(types::LIB))?.into_iter().map(|(_, l)| l).collect();
    libs.push(ctx.qsys);
    libs.extend(ctx.qtemp_library(txn));
    for lib in libs {
        let Ok(files) = ops::list(txn, lib, Some(types::FILE)) else { continue };
        for (_, f) in files {
            if f == id {
                continue;
            }
            if let Some((_, deps)) = definition(txn, f)
                && deps.iter().any(|d| (d.library, d.name) == me)
            {
                out.push(f);
            }
        }
    }
    Ok(out)
}

/// The view a change through `t` must satisfy (`WITH CHECK OPTION`),
/// with its logical format's select/omit rules.
pub fn check_option(txn: &mut Txn<'_>, t: &super::TableInfo) -> Option<String> {
    if t.desc.attr.kind != FileKind::Logical {
        return None;
    }
    let (def, _) = definition(txn, t.id)?;
    (def.check != CheckOption::None).then(|| names::sql_name(txn, t.id))
}

/// The object a dependency names, when it is still there.
pub fn dependency_object(txn: &mut Txn<'_>, ctx: &SqlContext, d: &Dependency) -> Option<ObjectId> {
    let found = super::qtemp::find_library(txn, ctx, d.library)?;
    let lib = store::obj::library::Library::load(txn, found).ok()?;
    lib.lookup(txn, d.type_code, d.name).ok().flatten()
}

/// The names a query body's select list gives its columns, `None` at a
/// position it cannot tell (`*`) or for the whole body (`VALUES`).
fn item_names(b: &QueryBody) -> Option<Vec<Option<String>>> {
    use crate::ast::{Expr, SelectItem};
    match b {
        QueryBody::Select(s) => Some(
            s.items
                .iter()
                .map(|i| match i {
                    SelectItem::Expr(_, Some(a)) => Some(a.value.clone()),
                    SelectItem::Expr(Expr::Column(c), None) => Some(c.column.value.clone()),
                    SelectItem::Expr(..) => Some(String::new()),
                    SelectItem::Star | SelectItem::TableStar(_) => None,
                })
                .collect(),
        ),
        QueryBody::Nested(q) => item_names(&q.body),
        QueryBody::Union { left, .. } => item_names(left),
        QueryBody::Values(_) => None,
    }
}

/// Whether the sides of every union name their columns alike.
fn union_names_agree(b: &QueryBody) -> bool {
    match b {
        QueryBody::Union { left, right, .. } => {
            if !union_names_agree(left) || !union_names_agree(right) {
                return false;
            }
            match (item_names(left), item_names(right)) {
                (Some(l), Some(r)) => l.iter().zip(&r).all(|(a, b)| match (a, b) {
                    (Some(a), Some(b)) => a == b && !a.is_empty(),
                    _ => true,
                }),
                _ => true,
            }
        }
        QueryBody::Nested(q) => union_names_agree(&q.body),
        _ => true,
    }
}
