// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `CREATE TABLE`, `CREATE INDEX` and `DROP`: the statement becomes DDS,
//! the DDS compiler checks it, the database creates the file.

use db::file::{self, CreateFileOpts, FileKind, MemberOpt};
use db::util::Hold;
use store::obj::LockState;
use dds::{CompileOpts, MapRefs, NoRefs, Source, compile_lf, compile_pf};
use qsname::Name;
use store::txn::Txn;

use super::{SqlContext, TableInfo};
use crate::ast::{
    ColumnDef, ColumnDefault, Constraint, CreateIndex, CreateTable, DataType, Expr, Ident, Literal,
    ObjectName,
};
use crate::error::{Result, SqlError};

/// One DDS line: positional columns 6, 17, 19, 30-34, 35, 36-37 and 45.
pub(crate) fn dds(name_type: &str, name: &str, len: &str, ty: &str, dec: &str, kw: &str) -> String {
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
pub(crate) fn kw(k: &str) -> String {
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
        Expr::Literal(Literal::Bool(b)) => format!("'{}'", if *b { "TRUE" } else { "FALSE" }),
        Expr::Literal(Literal::Bin(b) | Literal::Hex(b)) => format!("X'{}'", crate::value::hex(b)),
        Expr::Neg(inner) => format!("-{}", dds_literal(inner)?),
        Expr::Register(_) => {
            return Err(SqlError::Unsupported {
                what: "a special register where DDS wants a literal".into(),
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
        | DataType::Double
        | DataType::DecFloat(_) => "0",
        DataType::Date => "'0001-01-01'",
        DataType::Time => "'00.00.00'",
        DataType::Timestamp(_) => "'0001-01-01-00.00.00.000000'",
        DataType::Boolean => "'FALSE'",
        DataType::Binary(_) | DataType::Varbinary(_) | DataType::Blob(_) | DataType::Clob(_) | DataType::Dbclob(_) => {
            "''"
        }
        _ => "' '",
    }
}

/// The text an expression default is kept as: a special register.
fn register_text(r: crate::ast::Register) -> Result<String> {
    use crate::ast::Register;
    Ok(match r {
        Register::Date => "CURRENT DATE",
        Register::Time => "CURRENT TIME",
        Register::Timestamp(6) => "CURRENT TIMESTAMP",
        Register::Timestamp(p) => return Ok(format!("CURRENT TIMESTAMP({p})")),
        Register::User => "USER",
        Register::Server => "CURRENT SERVER",
        Register::Schema => "CURRENT SCHEMA",
        _ => {
            return Err(SqlError::Unsupported {
                what: "this special register as a column default".into(),
            });
        }
    }
    .into())
}

/// What DDS cannot say about a column, set on the compiled field: the
/// identity definition and an expression default.
fn finish_field(f: &mut dds::FieldDef, c: &ColumnDef) -> Result<()> {
    // The SQL name when it is not the system name, up to 128 characters
    // (`column_lines` refused a longer one).
    if c.name.value != f.name.to_string() {
        f.alias = Some(c.name.value.clone());
    }
    if let Some(id) = &c.identity {
        if !matches!(
            c.type_spec.data_type,
            DataType::SmallInt | DataType::Integer | DataType::BigInt | DataType::Decimal(_, 0)
        ) {
            return Err(SqlError::Unsupported {
                what: "an identity column that is not an integer or DECIMAL(n, 0)".into(),
            });
        }
        f.identity = Some(dds::IdentityDef {
            always: id.always,
            start: id.start,
            increment: id.increment,
            cycle: id.cycle,
        });
    }
    if let Some(ColumnDefault::Value(Expr::Register(r))) = &c.default {
        f.default = Some(dds::DefaultValue::Expr(register_text(*r)?));
    }
    // SQL's integers hold their bytes' whole range.
    f.int_range = matches!(c.type_spec.data_type, DataType::SmallInt | DataType::Integer | DataType::BigInt);
    if let DataType::Timestamp(p) = c.type_spec.data_type {
        f.length = dds::DataType::timestamp_length(p);
        f.byte_len = f.length;
    }
    if let Some(t) = sql_only_type(&c.type_spec.data_type) {
        f.data_type = t;
        f.length = match c.type_spec.data_type {
            DataType::DecFloat(n) => u32::from(n),
            DataType::Binary(n)
            | DataType::Varbinary(n)
            | DataType::Blob(n)
            | DataType::Clob(n)
            | DataType::Dbclob(n) => n,
            _ => 1,
        };
        f.ccsid = match t {
            dds::DataType::Bytes | dds::DataType::Blob => Some(65535),
            dds::DataType::Dbclob => Some(1200),
            _ => c.type_spec.ccsid,
        };
        if let Some(d) = c.default.as_ref().filter(|d| !matches!(d, ColumnDefault::Value(Expr::Register(_)))) {
            f.default = match d {
                ColumnDefault::Value(Expr::Literal(Literal::Bin(b) | Literal::Hex(b))) => {
                    Some(dds::DefaultValue::Hex(b.clone()))
                }
                d => default_def(d, &c.type_spec.data_type)?,
            };
        }
    }
    Ok(())
}

/// A LOB cannot be a key.
fn no_lob_keys(rf: &dds::RecordFormat, keys: impl Iterator<Item = usize>) -> Result<()> {
    for k in keys {
        if let Some(f) = rf.fields.get(k).filter(|f| f.data_type.is_lob()) {
            // IBM: SQL0350, a LOB cannot be a key.
            return Err(SqlError::Coded { code: -350, state: "42962", what: format!("the LOB column {} cannot be a key", f.name) });
        }
    }
    Ok(())
}

/// The field type of an SQL type DDS has no letter for.
fn sql_only_type(t: &DataType) -> Option<dds::DataType> {
    Some(match t {
        DataType::DecFloat(_) => dds::DataType::DecFloat,
        DataType::Binary(_) | DataType::Varbinary(_) => dds::DataType::Bytes,
        DataType::Boolean => dds::DataType::Boolean,
        DataType::Blob(_) => dds::DataType::Blob,
        DataType::Clob(_) => dds::DataType::Clob,
        DataType::Dbclob(_) => dds::DataType::Dbclob,
        _ => return None,
    })
}

/// The bytes the stand-in character field has, and whether it is `VARLEN`.
fn sql_only_shape(t: &DataType) -> (u32, bool) {
    match t {
        DataType::DecFloat(16) => (8, false),
        DataType::DecFloat(_) => (16, false),
        DataType::Binary(n) => (*n, false),
        DataType::Varbinary(n) => (*n, true),
        DataType::Blob(_) | DataType::Clob(_) | DataType::Dbclob(_) => (dds::LOB_REF_LEN, false),
        _ => (1, false),
    }
}

/// The DDS field line(s) of a column.
fn column_lines(c: &ColumnDef) -> Result<Vec<String>> {
    column_lines_as(c, sys_name(&c.name, c.system_name.as_ref())?)
}

/// The DDS field line(s) of a column whose system name is `name`.
pub(crate) fn column_lines_as(c: &ColumnDef, name: Name) -> Result<Vec<String>> {
    if c.name.value.chars().count() > db::format::sections::ALIAS_MAX {
        return Err(SqlError::NameTooLong { name: c.name.value.clone(), max: db::format::sections::ALIAS_MAX });
    }
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
        DataType::Timestamp(_) => (String::new(), "Z", String::new(), vec![]),
        // DDS has no word for these: a character field of the bytes
        // stands in, and `finish_field` gives it its type.
        t => {
            let (n, varlen) = sql_only_shape(t);
            (n.to_string(), "A", String::new(), if varlen { vec!["VARLEN".into()] } else { vec![] })
        }
    };
    let sql_only = sql_only_type(&c.type_spec.data_type).is_some();
    // FOR BIT DATA is CCSID 65535.
    let ccsid = c.type_spec.ccsid.or(c.type_spec.bit_data.then_some(65535));
    if let Some(ccsid) = ccsid.filter(|_| !sql_only) {
        kws.push(format!("CCSID({ccsid})"));
    }
    if !c.not_null {
        kws.push("ALWNULL".into());
    }
    match &c.default {
        // A register is evaluated at insert; the field carries it apart,
        // as it does the default of a type DDS does not have.
        Some(ColumnDefault::Value(Expr::Register(_))) | None => {}
        Some(_) if sql_only => {}
        Some(ColumnDefault::Value(e)) => kws.push(format!("DFT({})", dds_literal(e)?)),
        Some(ColumnDefault::System) => {
            kws.push(format!("DFT({})", system_default(&c.type_spec.data_type)));
        }
    }
    // The SQL name is the field's ALIAS, set by `finish_field`: DDS's
    // keyword has no room for 128 characters.
    let mut lines = Vec::new();
    let first = kws.first().cloned().unwrap_or_default();
    lines.push(dds("", &name.to_string(), &len, ty, &dec, &first));
    for k in kws.iter().skip(1) {
        lines.push(kw(k));
    }
    Ok(lines)
}

/// `CREATE TABLE` of columns: a physical file from the generated DDS,
/// its SQL names kept. `AS`, `LIKE` and `OR REPLACE` are `table`'s.
pub fn create_table_def(txn: &mut Txn<'_>, ctx: &mut SqlContext, t: &CreateTable) -> Result<store::obj::ObjectId> {
    let library = ctx.library_for(txn, &t.name)?;
    let (name, sql) = super::names::new_object(txn, library, &t.name.name, t.system_name.as_ref())?;
    let col_names = super::table::column_names(&t.columns)?;
    let col_sys = |i: &Ident| -> Result<Name> {
        t.columns
            .iter()
            .position(|c| c.name.value == i.value)
            .map_or_else(|| SqlContext::sys_name(i), |p| Ok(col_names[p]))
    };
    let rcdfmt = match &t.rcdfmt {
        Some(r) => SqlContext::sys_name(r)?,
        // IBM names the format after the table.
        None => super::names::format_name(name),
    };
    // Keys: the primary key, from the table or a column; UNIQUE
    // constraints become unique indexes afterwards.
    let mut keys: Vec<Name> = Vec::new();
    let mut uniques: Vec<(Option<Ident>, Vec<Ident>)> = Vec::new();
    for c in &t.constraints {
        match c {
            Constraint::PrimaryKey { columns, .. } => {
                keys = columns
                    .iter()
                    .map(col_sys)
                    .collect::<Result<_>>()?;
            }
            Constraint::Unique { name, columns } => {
                uniques.push((name.clone(), columns.clone()));
            }
            Constraint::ForeignKey { .. } | Constraint::Check { .. } => {}
        }
    }
    for (c, n) in t.columns.iter().zip(&col_names) {
        if c.primary_key {
            keys.push(*n);
        }
        if c.unique && !c.primary_key {
            uniques.push((None, vec![c.name.clone()]));
        }
    }
    let mut lines = Vec::new();
    if !keys.is_empty() {
        lines.push(kw("UNIQUE"));
    }
    lines.push(dds("R", &rcdfmt.to_string(), "", "", "", ""));
    for (c, n) in t.columns.iter().zip(&col_names) {
        lines.extend(column_lines_as(c, *n)?);
    }
    for k in &keys {
        lines.push(dds("K", &k.to_string(), "", "", "", ""));
    }
    let mut def = compile_table(&lines)?;
    for (f, c) in def.format.fields.iter_mut().zip(&t.columns) {
        finish_field(f, c)?;
    }
    def.format.lay_out();
    no_lob_keys(&def.format, def.format.keys.iter().map(|k| k.field))?;
    let id = file::create_pf(
        txn,
        library,
        name,
        &def,
        &CreateFileOpts {
            now: ctx.clock(),
            // IBM leaves an SQL table's text blank; its kind is in its
            // `*SQLINF`.
            text: String::new(),
            // An SQL table has no size limit: SIZE(*NOMAX).
            size: file::Size::NOMAX,
            ..CreateFileOpts::default()
        },
    )?;
    finish_table(txn, ctx, library, id, sql.as_deref(), &t.name.name.value)?;
    let mut anon = 0;
    for (cname, columns) in uniques {
        if cname.is_none() {
            anon += 1;
        }
        add_unique(txn, ctx, &t.name, &name, anon, cname.as_ref(), &columns)?;
    }
    Ok(id)
}

/// The journal a schema keeps for its tables.
const SCHEMA_JOURNAL: &str = "QSQJRN";
/// Its first receiver.
const SCHEMA_RECEIVER: &str = "QSQJRN0001";

/// `CREATE SCHEMA`: a library with the journal `QSQJRN` and its first
/// receiver, to which tables made in it by SQL are journaled, both
/// images.
pub fn create_schema(txn: &mut Txn<'_>, ctx: &mut SqlContext, name: &Ident) -> Result<()> {
    let lib_name = SqlContext::sys_name(name)?;
    let now = ctx.clock();
    let lib = store::obj::ops::create(
        txn,
        &store::obj::CreateParams {
            library: ctx.qsys,
            name: lib_name,
            type_code: store::obj::types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: String::new(),
            system: String::new(),
            now,
            by_system: false,
        },
    )?;
    let fixed = |n: &str| Name::parse(n).expect("a valid name");
    let rcv = jrn::rcv::crtjrnrcv(
        txn,
        lib,
        fixed(SCHEMA_RECEIVER),
        &jrn::rcv::CrtJrnRcvOpts {
            now,
            ..jrn::rcv::CrtJrnRcvOpts::default()
        },
    )
    .map_err(db::DbError::from)?;
    jrn::journal::crtjrn(
        txn,
        lib,
        fixed(SCHEMA_JOURNAL),
        rcv,
        &jrn::journal::CrtJrnOpts {
            now,
            ..jrn::journal::CrtJrnOpts::default()
        },
    )
    .map_err(db::DbError::from)?;
    Ok(())
}

/// What every new SQL table gets: its SQL name kept, journaling in a
/// schema, else the `SQL7905` warning (none in QTEMP).
pub(crate) fn finish_table(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    library: store::obj::ObjectId,
    id: store::obj::ObjectId,
    sql: Option<&str>,
    shown: &str,
) -> Result<()> {
    super::names::record(txn, library, id, db::sqlinf::Kind::Table, sql, ctx.clock())?;
    if !journal_in_schema(txn, ctx, library, id)? {
        let lib = store::obj::ops::read_header(txn, library)?.1.name.to_string();
        if lib != "QTEMP" {
            ctx.warning = Some(crate::sqlca::Warning::not_journaled(shown, &lib));
        }
    }
    Ok(())
}

/// A table made in a library with `QSQJRN` (a schema) is journaled to it;
/// false when the library has none.
fn journal_in_schema(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    library: store::obj::ObjectId,
    file: store::obj::ObjectId,
) -> Result<bool> {
    let lib = store::obj::library::Library::load(txn, library)?;
    let Some(journal) = lib.lookup(txn, jrn::JRN, Name::parse(SCHEMA_JOURNAL).expect("valid"))? else {
        return Ok(false);
    };
    db::journal::strjrnpf(txn, ctx.job, file, journal, true, false, ctx.clock())?;
    Ok(true)
}

/// A `UNIQUE` constraint: a unique index over the columns, its file
/// named after the constraint (or `Q<table>U<n>`) and its text recording
/// the constraint for the catalog.
pub fn add_unique(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    table: &ObjectName,
    sys_table: &Name,
    n: usize,
    cname: Option<&Ident>,
    columns: &[Ident],
) -> Result<()> {
    let cname = cname.map_or_else(|| format!("Q_{sys_table}_{n}"), |c| c.value.clone());
    let index_name = match cname.len() {
        0..=10 if Name::parse(&cname).is_ok() => cname.clone(),
        _ => {
            let stem: String = sys_table.to_string().chars().take(6).collect();
            format!("{stem}U{n:03}")
        }
    };
    let i = CreateIndex {
        name: ObjectName {
            schema: table.schema.clone(),
            name: Ident::new(&index_name),
        },
        unique: true,
        where_not_null: false,
        table: table.clone(),
        columns: columns.iter().map(|c| (c.clone(), false)).collect(),
        rcdfmt: None,
        sparse: None,
        encoded_vector: false,
        includes: Vec::new(),
    };
    create_index_with_text(txn, ctx, &i, &format!("{UNIQUE_TEXT}{cname}"))
}

/// What the text of a unique constraint's index starts with.
pub const UNIQUE_TEXT: &str = "SQL unique constraint ";

/// The physical file definition the DDS lines make.
fn compile_table(lines: &[String]) -> Result<dds::PfDefinition> {
    let (source, _) = Source::from_text(&lines.join("\n"));
    let comp = compile_pf(&source, &NoRefs, &CompileOpts::default());
    comp.definition.ok_or_else(|| SqlError::Database {
        what: format!("the table definition does not compile:\n{}", comp.listing),
        msgid: None,
    })
}

/// A column's field, compiled on its own.
fn field_of(c: &ColumnDef) -> Result<dds::FieldDef> {
    let mut lines = vec![dds("R", "ALTERED", "", "", "", "")];
    lines.extend(column_lines(c)?);
    let def = compile_table(&lines)?;
    let mut f = def
        .format
        .fields
        .into_iter()
        .next()
        .ok_or_else(|| SqlError::Database {
            what: "the column compiled to no field".into(),
            msgid: None,
        })?;
    finish_field(&mut f, c)?;
    Ok(f)
}

/// A column default as DDS keeps it.
fn default_def(d: &ColumnDefault, t: &DataType) -> Result<Option<dds::DefaultValue>> {
    use dds::DefaultValue as D;
    Ok(Some(match d {
        ColumnDefault::System => {
            let text = system_default(t);
            match text.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
                Some(inner) => D::Quoted(inner.to_string()),
                None => D::Number(text.to_string()),
            }
        }
        ColumnDefault::Value(Expr::Register(r)) => D::Expr(register_text(*r)?),
        ColumnDefault::Value(Expr::Literal(Literal::Null)) => D::Null,
        ColumnDefault::Value(e) => {
            let text = dds_literal(e)?;
            match text.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')) {
                Some(inner) => D::Quoted(inner.replace("''", "'")),
                None => D::Number(text),
            }
        }
    }))
}

/// `ALTER TABLE`: columns added, dropped or changed and keys changed in
/// the record format, the data converted as `CHGPF SRCFILE` converts
/// it; `UNIQUE` constraints added or dropped as unique indexes. A table
/// with indexes or views over it refuses a format change: drop them
/// first.
#[allow(clippy::too_many_lines)]
pub fn alter_table(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    stmt: &crate::ast::AlterTable,
) -> Result<()> {
    use crate::ast::AlterAction;
    let table = ctx.resolve_table(txn, &stmt.name)?;
    if table.desc.attr.kind != FileKind::Physical {
        return Err(SqlError::Unsupported {
            what: "ALTER TABLE on a view or an index".into(),
        });
    }
    let mut rf = table.format.format.clone();
    let mut unique = table.format.unique;
    let mut touched = false;
    let mut indexes: Vec<(Option<Ident>, Vec<Ident>)> = Vec::new();
    let mut drops: Vec<Ident> = Vec::new();
    let find = |rf: &dds::RecordFormat, name: &Ident| -> Result<usize> {
        let n = SqlContext::sys_name(name)?;
        rf.field(&n)
            .map(|(i, _)| i)
            .ok_or_else(|| SqlError::ColumnNotFound {
                name: name.value.clone(),
            })
    };
    for act in &stmt.actions {
        match act {
            AlterAction::AddColumn(c) => {
                let f = field_of(c)?;
                if rf.field(&f.name).is_some() {
                    return Err(SqlError::Database {
                        what: format!("column {} already exists", f.name),
                        msgid: Some("CPF3204".into()),
                    });
                }
                rf.fields.push(f);
                touched = true;
            }
            AlterAction::DropColumn { name, .. } => {
                let i = find(&rf, name)?;
                if rf.keys.iter().any(|k| k.field == i) {
                    return Err(SqlError::Unsupported {
                        what: "dropping a key column; drop the primary key first".into(),
                    });
                }
                rf.fields.remove(i);
                for k in &mut rf.keys {
                    if k.field > i {
                        k.field -= 1;
                    }
                }
                touched = true;
            }
            AlterAction::SetDataType(name, ts) => {
                let i = find(&rf, name)?;
                let old = rf.fields[i].clone();
                let c = ColumnDef {
                    name: name.clone(),
                    system_name: None,
                    type_spec: ts.clone(),
                    not_null: !old.allow_null,
                    default: None,
                    identity: None,
                    primary_key: false,
                    unique: false,
                };
                let mut f = field_of(&c)?;
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
                rf.fields[i].allow_null = !yes;
                touched = true;
            }
            AlterAction::SetDefault(name, d) => {
                let i = find(&rf, name)?;
                let data_type = rf.fields[i].data_type;
                rf.fields[i].default = match d {
                    Some(d) => default_def(d, &data_type_of(&rf.fields[i], data_type))?,
                    None => None,
                };
                touched = true;
            }
            AlterAction::AddConstraint(Constraint::PrimaryKey { columns, .. }) => {
                rf.keys.clear();
                for c in columns {
                    let i = find(&rf, c)?;
                    let f = &rf.fields[i];
                    rf.keys.push(dds::KeyField {
                        name: f.name,
                        field: i,
                        descend: false,
                        transform: if f.data_type.is_numeric() {
                            dds::KeyTransform::Signed
                        } else {
                            dds::KeyTransform::Unsigned
                        },
                        noaltseq: false,
                        line: 0,
                    });
                }
                unique = Some(dds::NullKeys::Include);
                touched = true;
            }
            AlterAction::AddConstraint(Constraint::Unique { name, columns }) => {
                indexes.push((name.clone(), columns.clone()));
            }
            AlterAction::AddConstraint(_) => {}
            AlterAction::DropPrimaryKey => {
                rf.keys.clear();
                unique = None;
                touched = true;
            }
            AlterAction::DropConstraint(name) => drops.push(name.clone()),
        }
    }
    if touched {
        rf.lay_out();
        let def = dds::PfDefinition {
            format: rf,
            unique,
            dup_order: table.format.dup_order,
            reference: None,
            refaccpth: None,
            altseq: None,
            ccsid: None,
            text: None,
        };
        // The records are read and converted taking turns, the table
        // held `*EXCL`; they are written in one step.
        let job = ctx.job;
        db::chg::chgpf_srcfile_paused(txn, job, table.id, &def, &mut |t| ctx.pause(t))?;
    }
    for name in &drops {
        drop_constraint(txn, ctx, &table, name)?;
    }
    let sys = table.desc.name();
    let mut anon = existing_unique(txn, &table);
    for (cname, columns) in indexes {
        if cname.is_none() {
            anon += 1;
        }
        add_unique(txn, ctx, &stmt.name, &sys, anon, cname.as_ref(), &columns)?;
    }
    Ok(())
}

/// The SQL data type a field's DDS type stands for, for its default.
fn data_type_of(f: &dds::FieldDef, t: dds::DataType) -> DataType {
    match t {
        dds::DataType::Date => DataType::Date,
        dds::DataType::Time => DataType::Time,
        dds::DataType::Timestamp => DataType::Timestamp(dds::DataType::timestamp_precision(f.length)),
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

/// How many unnamed unique constraints the table has, from the texts
/// of the indexes over it.
fn existing_unique(txn: &mut Txn<'_>, t: &TableInfo) -> usize {
    let mut n = 0;
    let stem = format!("{UNIQUE_TEXT}Q_{}_", t.desc.name());
    for d in &t.desc.dependents {
        if let Ok(lf) = file::describe(txn, d.lf_file)
            && ccsid::decode_trimmed(&lf.header.text).starts_with(&stem)
        {
            n += 1;
        }
    }
    n
}

/// `DROP CONSTRAINT name`: the unique index recorded for it goes.
fn drop_constraint(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    t: &TableInfo,
    name: &Ident,
) -> Result<()> {
    let want = format!("{UNIQUE_TEXT}{}", name.value);
    let mut ids: Vec<_> = t.desc.dependents.iter().map(|d| d.lf_file).collect();
    ids.sort();
    ids.dedup();
    for lf in ids {
        if let Ok(d) = file::describe(txn, lf)
            && ccsid::decode_trimmed(&d.header.text) == want
        {
            file::delete_file(txn, ctx.job, lf)?;
            return Ok(());
        }
    }
    Err(SqlError::ObjectNotFound {
        kind: "constraint",
        name: name.value.clone(),
    })
}

/// `CREATE INDEX`: a keyed logical file over the table.
pub fn create_index(txn: &mut Txn<'_>, ctx: &mut SqlContext, i: &CreateIndex) -> Result<()> {
    let text = format!(
        "SQL index {}",
        i.name.name.value.chars().take(40).collect::<String>()
    );
    create_index_with_text(txn, ctx, i, &text)
}

/// `CREATE INDEX` with the file's text given.
fn create_index_with_text(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    i: &CreateIndex,
    text: &str,
) -> Result<()> {
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
        if let Some((_, f)) = table.format.format.field(&col).filter(|(_, f)| f.data_type.is_lob()) {
            return Err(SqlError::Unsupported { what: format!("the LOB column {} as a key", f.name) });
        }
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
    build_index(txn, ctx, &table, library, name, &def, text).map(|_| ())
}

/// The index file over `table`, its path built taking turns: the table
/// held `*SHRNUP` throughout (read, never changed, by others), the new
/// file `*EXCL` from its creation, and the file gone again when the
/// build fails or is ended.
pub(crate) fn build_index(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    table: &TableInfo,
    library: store::obj::ObjectId,
    name: Name,
    def: &dds::LfDefinition,
    text: &str,
) -> Result<store::obj::ObjectId> {
    let base = table.id;
    let job = ctx.job;
    let now = ctx.clock();
    let text: String = text.chars().take(50).collect();
    let what = table.desc.name().to_string();
    let base_hold = Hold::take(
        txn,
        job,
        &[(base, LockState::SharedNoUpdate), (table.member.object, LockState::SharedNoUpdate)],
        &what,
    )?;
    let opts = CreateFileOpts {
        now,
        text: text.clone(),
        member: MemberOpt::None,
        ..CreateFileOpts::default()
    };
    let lf = match file::create_lf(txn, library, name, def, &mut |_| Ok(base), &opts) {
        Ok(id) => id,
        Err(e) => {
            base_hold.release(txn);
            return Err(e.into());
        }
    };
    let built = Hold::take(txn, job, &[(lf, LockState::Exclusive)], &name.to_string()).and_then(|hold| {
        let r = file::add_lf_member_paused(txn, lf, name, &text, None, now, &mut |t| ctx.pause(t));
        hold.release(txn);
        r
    });
    if built.is_err() {
        // Nothing of it stays, the parts it committed included.
        let _ = file::delete_file(txn, job, lf);
    }
    base_hold.release(txn);
    built?;
    Ok(lf)
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
pub(crate) fn select_lines(
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
    library: store::obj::ObjectId,
    name: Name,
) -> Result<store::obj::ObjectId> {
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
    Ok(file::create_lf(
        txn,
        library,
        name,
        &def,
        &mut |_| Ok(base_id),
        // IBM leaves an SQL view's text blank; its kind is in its
        // `*SQLINF`.
        &CreateFileOpts { now: ctx.clock(), text: String::new(), ..CreateFileOpts::default() },
    )?)
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
        // The column by its system name or its SQL name.
        let Some(f) = formats
            .iter_mut()
            .flat_map(|fm| fm.format.fields.iter_mut())
            .find(|f| f.name.to_string() == c.value || f.alias.as_deref() == Some(c.value.as_str()))
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
