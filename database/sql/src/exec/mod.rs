// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Running statements: the context a job gives SQL, name resolution,
//! and `Prepared::execute`. Queries are planned in `query`, data
//! changes in `dml`, definitions in `ddl`, expressions in `eval`.

pub mod catalog;
pub mod cursor;
pub mod ddl;
pub mod dml;
pub mod eval;
pub mod func;
pub mod opnqryf;
pub mod query;
mod sort;
pub mod spill;

use db::file::{self, FileDesc, MemberEntry};
use db::format::FormatDef;
use db::odp::{self, Odp, OpenOpts, Session, SharedLocks, Usage};
use dds::FieldDef;
use qsname::{LibSpec, MemberSpec, Name};
use store::obj::ops::{self, NameContext};
use store::obj::{JobHandle, ObjectId, types};
use store::txn::Txn;

use crate::ast::{Naming, ObjectName, Statement};
use crate::error::{Result, SqlError};
use crate::parse;
use crate::value::Value;

/// A table the statement named, resolved.
#[derive(Clone, Debug)]
pub struct TableInfo {
    /// The file.
    pub id: ObjectId,
    /// Its description.
    pub desc: FileDesc,
    /// The member used.
    pub member: MemberEntry,
    /// The record format.
    pub format: FormatDef,
}

impl TableInfo {
    /// The field named, with its index.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<(usize, &FieldDef)> {
        let n = Name::parse(name).ok()?;
        self.format.format.field(&n)
    }
}

/// Rows an `ORDER BY` sorts in memory before spilling (`sort_budget`).
pub const DEFAULT_SORT_BUDGET: usize = 100_000;

/// What a job brings to SQL: naming, the library list, the schema, the
/// job handle and its database session.
pub struct SqlContext {
    /// Naming mode.
    pub naming: Naming,
    /// QSYS, the root of name resolution.
    pub qsys: ObjectId,
    /// Current library and library list.
    pub names: NameContext,
    /// `SET SCHEMA`, for unqualified names under `*SQL`.
    pub current_schema: Option<Name>,
    /// The job.
    pub job: JobHandle,
    /// The job's database session (overrides, commitment control).
    pub session: Session,
    /// The record lock table.
    pub locks: SharedLocks,
    /// The user, for `CURRENT USER`.
    pub user: String,
    /// A fixed clock, for tests; the real one when `None`.
    pub now: Option<u64>,
    /// Lines the last `EXPLAIN` produced.
    pub explain: Vec<String>,
    /// Declared cursors by name.
    pub cursors: std::collections::HashMap<String, cursor::Cursor>,
    /// Rows a sort holds in memory before it spills a run.
    pub sort_budget: usize,
    /// Where sort runs go: the job's temporary storage.
    pub spill: Box<dyn spill::SpillArea>,
    /// A column function left out a null value (SQLSTATE `01003`).
    pub nulls_eliminated: bool,
}

impl SqlContext {
    /// A context for `job` with a library list.
    #[must_use]
    pub fn new(qsys: ObjectId, job: JobHandle, names: NameContext) -> Self {
        Self::with_locks(qsys, job, names, odp::new_shared())
    }

    /// A context whose record locks live in a table other jobs share.
    #[must_use]
    pub fn with_locks(
        qsys: ObjectId,
        job: JobHandle,
        names: NameContext,
        locks: odp::SharedLocks,
    ) -> Self {
        Self {
            naming: Naming::Sys,
            qsys,
            names,
            current_schema: None,
            job,
            session: Session::new(job, locks.clone()),
            locks,
            user: "QSECOFR".into(),
            now: None,
            explain: Vec::new(),
            cursors: std::collections::HashMap::new(),
            sort_budget: DEFAULT_SORT_BUDGET,
            spill: Box::new(spill::MemSpill::default()),
            nulls_eliminated: false,
        }
    }

    /// The clock.
    #[must_use]
    pub fn clock(&self) -> u64 {
        self.now.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs())
        })
    }

    /// An identifier as a system name.
    pub fn sys_name(ident: &crate::ast::Ident) -> Result<Name> {
        let v = &ident.value;
        if v.len() > 10 {
            return Err(SqlError::Unsupported {
                what: format!("long name {v} without a system name (FOR SYSTEM NAME / FOR COLUMN)"),
            });
        }
        Name::parse(v).map_err(|_| SqlError::ObjectNotFound {
            name: v.clone(),
            kind: "name",
        })
    }

    /// Where an unqualified name is looked up or created.
    fn default_library(&self) -> LibSpec {
        match (self.naming, self.current_schema) {
            (Naming::Sql, Some(s)) => LibSpec::Name(s),
            _ => LibSpec::Libl,
        }
    }

    /// The library an object name means.
    pub fn lib_spec(&self, name: &ObjectName) -> Result<LibSpec> {
        Ok(match &name.schema {
            Some(s) => LibSpec::Name(Self::sys_name(s)?),
            None => self.default_library(),
        })
    }

    /// The library to create an object in.
    pub fn library_for(&self, txn: &mut Txn<'_>, name: &ObjectName) -> Result<ObjectId> {
        match self.lib_spec(name)? {
            LibSpec::Name(n) => ops::find_library(txn, self.qsys, n)
                .map(|f| f.id)
                .map_err(|_| SqlError::ObjectNotFound {
                    name: n.to_string(),
                    kind: "schema",
                }),
            LibSpec::Curlib | LibSpec::Libl => self
                .names
                .curlib
                .or_else(|| self.names.libl.first().copied())
                .ok_or_else(|| SqlError::ObjectNotFound {
                    name: "*CURLIB".into(),
                    kind: "schema",
                }),
        }
    }

    /// Resolve a table, view or index name to a file.
    pub fn resolve_file(&self, txn: &mut Txn<'_>, name: &ObjectName) -> Result<ObjectId> {
        let lib = self.lib_spec(name)?;
        let n = Self::sys_name(&name.name)?;
        ops::resolve(txn, self.qsys, &self.names, lib, n, types::FILE)
            .map(|f| f.id)
            .map_err(|_| SqlError::ObjectNotFound {
                name: name.to_string(),
                kind: "table",
            })
    }

    /// Resolve a table to its first member.
    pub fn resolve_table(&self, txn: &mut Txn<'_>, name: &ObjectName) -> Result<TableInfo> {
        let id = self.resolve_file(txn, name)?;
        let desc = file::describe(txn, id).map_err(|_| SqlError::ObjectNotFound {
            name: name.to_string(),
            kind: "table",
        })?;
        let member = *desc
            .member(MemberSpec::First)
            .ok_or_else(|| SqlError::ObjectNotFound {
                name: format!("{name} (no member)"),
                kind: "table",
            })?;
        let format = desc
            .formats
            .first()
            .cloned()
            .ok_or_else(|| SqlError::ObjectNotFound {
                name: name.to_string(),
                kind: "table",
            })?;
        Ok(TableInfo {
            id,
            desc,
            member,
            format,
        })
    }

    /// Open a table for a data change, under commitment control when the
    /// job has it.
    pub fn open(&mut self, txn: &mut Txn<'_>, t: &TableInfo, usage: Usage) -> Result<Odp> {
        let cmt = self.session.commit_def().cloned();
        let opts = OpenOpts {
            member: MemberSpec::Name(t.member.name),
            usage,
            keyed: false,
            commit: cmt.is_some(),
            ..OpenOpts::input(self.job)
        };
        let mut o = Odp::open(txn, self.locks.clone(), t.id, opts)?;
        if let Some(c) = cmt {
            o.set_commit(c);
        }
        Ok(o)
    }
}

/// A column of a result set, as the SQLDA would describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnDesc {
    /// Name.
    pub name: String,
    /// SQL type, e.g. `DECIMAL(7,0)`.
    pub data_type: String,
    /// Nullable.
    pub nullable: bool,
}

/// Rows with their columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResultSet {
    /// Columns.
    pub columns: Vec<ColumnDesc>,
    /// Rows.
    pub rows: Vec<Vec<Value>>,
}

impl ResultSet {
    /// A simple text table.
    #[must_use]
    pub fn render(&self) -> String {
        let mut widths: Vec<usize> = self
            .columns
            .iter()
            .map(|c| c.name.chars().count())
            .collect();
        let texts: Vec<Vec<String>> = self
            .rows
            .iter()
            .map(|r| {
                r.iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let s = if v.is_null() {
                            "-".to_string()
                        } else {
                            v.to_string()
                        };
                        widths[i] = widths[i].max(s.chars().count());
                        s
                    })
                    .collect()
            })
            .collect();
        let line = |cells: &[String]| -> String {
            cells
                .iter()
                .enumerate()
                .map(|(i, c)| format!("{c:<w$}", w = widths[i]))
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_string()
        };
        let mut out = line(
            &self
                .columns
                .iter()
                .map(|c| c.name.clone())
                .collect::<Vec<_>>(),
        );
        out.push('\n');
        for t in &texts {
            out.push_str(&line(t));
            out.push('\n');
        }
        out
    }
}

/// What a statement produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// A result set.
    Rows(ResultSet),
    /// Rows changed.
    Count(u64),
    /// Nothing to show.
    Done,
}

/// A parsed statement ready to run.
#[derive(Clone, Debug)]
pub struct Prepared {
    /// The statement.
    pub statement: Statement,
    /// Naming mode it was parsed with.
    pub naming: Naming,
}

/// Parse a statement.
pub fn prepare(text: &str, naming: Naming) -> Result<Prepared> {
    Ok(Prepared {
        statement: parse::parse(text, naming)?,
        naming,
    })
}

impl Prepared {
    /// Run the statement with `params` for the `?` markers.
    pub fn execute(
        &self,
        txn: &mut Txn<'_>,
        ctx: &mut SqlContext,
        params: &[Value],
    ) -> Result<Outcome> {
        ctx.naming = self.naming;
        match &self.statement {
            Statement::Select(q) => {
                let rs = query::run_query(txn, ctx, params, q, &eval::Env::default())?;
                Ok(Outcome::Rows(rs))
            }
            Statement::Insert(i) => dml::insert(txn, ctx, params, i).map(Outcome::Count),
            Statement::Update(u) => dml::update(txn, ctx, params, u).map(Outcome::Count),
            Statement::Delete(d) => dml::delete(txn, ctx, params, d).map(Outcome::Count),
            Statement::CreateTable(t) => ddl::create_table(txn, ctx, t).map(|()| Outcome::Done),
            Statement::CreateIndex(i) => ddl::create_index(txn, ctx, i).map(|()| Outcome::Done),
            Statement::AlterTable(a) => ddl::alter_table(txn, ctx, a).map(|()| Outcome::Done),
            Statement::DropTable { name, cascade } => {
                ddl::drop(txn, ctx, name, *cascade).map(|()| Outcome::Done)
            }
            Statement::DropIndex(name) | Statement::DropView(name) => {
                ddl::drop(txn, ctx, name, false).map(|()| Outcome::Done)
            }
            Statement::Commit { hold } => {
                if ctx.session.commit_def().is_some() {
                    ctx.session.commit(txn, ctx.now)?;
                }
                if !hold {
                    cursor::end_of_unit(ctx);
                }
                Ok(Outcome::Done)
            }
            Statement::Rollback { hold } => {
                if ctx.session.commit_def().is_some() {
                    ctx.session.rollbck(txn, ctx.now)?;
                }
                if !hold {
                    cursor::end_of_unit(ctx);
                }
                Ok(Outcome::Done)
            }
            Statement::DeclareCursor(d) => Ok(cursor::declare(ctx, d)),
            Statement::Open { cursor: c, using } => {
                let mut values = Vec::with_capacity(using.len());
                let mut ev = eval::Evaluator { ctx, params };
                for u in using {
                    values.push(ev.eval(txn, u, &eval::Env::default())?);
                }
                let p: Vec<Value> = if values.is_empty() {
                    params.to_vec()
                } else {
                    values
                };
                cursor::open(txn, ctx, &c.value, &p)
            }
            Statement::Fetch(f) => cursor::fetch(ctx, f, params),
            Statement::Close(c) => cursor::close(ctx, &c.value),
            Statement::CreateView(v) => ddl::create_view(txn, ctx, v).map(|()| Outcome::Done),
            Statement::LabelOn(l) | Statement::CommentOn(l) => {
                ddl::label(txn, ctx, l).map(|()| Outcome::Done)
            }
            Statement::Merge(m) => dml::merge(txn, ctx, params, m).map(Outcome::Count),
            Statement::SetSchema(target) => {
                ctx.current_schema = match target {
                    crate::ast::SchemaTarget::Name(n) => Some(SqlContext::sys_name(n)?),
                    crate::ast::SchemaTarget::User => Name::parse(&ctx.user).ok(),
                    crate::ast::SchemaTarget::Default => None,
                    crate::ast::SchemaTarget::Host(_) => {
                        return Err(SqlError::Unsupported {
                            what: "SET SCHEMA from a host variable".into(),
                        });
                    }
                };
                Ok(Outcome::Done)
            }
            Statement::SetPath(_) | Statement::SetCurrentDegree => Ok(Outcome::Done),
            Statement::Explain(inner) => {
                let Statement::Select(q) = &**inner else {
                    return Err(SqlError::Unsupported {
                        what: "EXPLAIN of a statement that is not a query".into(),
                    });
                };
                ctx.explain.clear();
                query::explain(txn, ctx, params, q)?;
                let lines = std::mem::take(&mut ctx.explain);
                Ok(Outcome::Rows(ResultSet {
                    columns: vec![ColumnDesc {
                        name: "PLAN".into(),
                        data_type: "VARCHAR(132)".into(),
                        nullable: false,
                    }],
                    rows: lines.into_iter().map(|l| vec![Value::Str(l)]).collect(),
                }))
            }
            other => Err(SqlError::Unsupported {
                what: format!("running {}", statement_name(other)),
            }),
        }
    }
}

impl Prepared {
    /// Run and report through an SQLCA, as a program would see it.
    pub fn run(
        &self,
        txn: &mut Txn<'_>,
        ctx: &mut SqlContext,
        params: &[Value],
    ) -> (crate::sqlca::Sqlca, Option<Outcome>) {
        match self.execute(txn, ctx, params) {
            Ok(o) => (crate::sqlca::Sqlca::from_outcome(&o), Some(o)),
            Err(e) => (crate::sqlca::Sqlca::from_error(&e), None),
        }
    }
}

/// The statement's name, for messages.
#[must_use]
pub fn statement_name(s: &Statement) -> &'static str {
    match s {
        Statement::CreateTable(_) => "CREATE TABLE",
        Statement::AlterTable(_) => "ALTER TABLE",
        Statement::DropTable { .. } => "DROP TABLE",
        Statement::CreateIndex(_) => "CREATE INDEX",
        Statement::DropIndex(_) => "DROP INDEX",
        Statement::CreateView(_) => "CREATE VIEW",
        Statement::DropView(_) => "DROP VIEW",
        Statement::CreateAlias { .. } => "CREATE ALIAS",
        Statement::DropAlias(_) => "DROP ALIAS",
        Statement::LabelOn(_) => "LABEL ON",
        Statement::CommentOn(_) => "COMMENT ON",
        Statement::Rename(_) => "RENAME",
        Statement::Select(_) => "SELECT",
        Statement::Insert(_) => "INSERT",
        Statement::Update(_) => "UPDATE",
        Statement::Delete(_) => "DELETE",
        Statement::Merge(_) => "MERGE",
        Statement::DeclareCursor(_) => "DECLARE CURSOR",
        Statement::Open { .. } => "OPEN",
        Statement::Fetch(_) => "FETCH",
        Statement::Close(_) => "CLOSE",
        Statement::Commit { .. } => "COMMIT",
        Statement::Rollback { .. } => "ROLLBACK",
        Statement::SetSchema(_) => "SET SCHEMA",
        Statement::SetPath(_) => "SET PATH",
        Statement::SetCurrentDegree => "SET CURRENT DEGREE",
        Statement::Explain(_) => "EXPLAIN",
    }
}

/// A store failure is a system error to SQL.
impl From<store::StoreError> for SqlError {
    fn from(e: store::StoreError) -> Self {
        Self::Database {
            what: e.to_string(),
            msgid: e.msgid().map(str::to_string),
        }
    }
}

/// `DbError` into `SqlError` with the SQLCODE the condition has.
impl From<db::DbError> for SqlError {
    fn from(e: db::DbError) -> Self {
        use db::DbError as D;
        match &e {
            D::DuplicateKey { .. } => Self::DuplicateKey {
                what: e.to_string(),
            },
            D::RecordLocked { .. } | D::FileLocked { .. } => Self::Locked {
                what: e.to_string(),
            },
            D::CommitNeedsJournal { .. } => Self::NotJournaled {
                what: e.to_string(),
            },
            D::DataMapping { .. } => Self::Conversion {
                what: e.to_string(),
            },
            D::NotAFile { .. } | D::MemberNotFound { .. } => Self::ObjectNotFound {
                name: e.to_string(),
                kind: "table",
            },
            _ => Self::Database {
                what: e.to_string(),
                msgid: e.msgid().map(str::to_string),
            },
        }
    }
}
