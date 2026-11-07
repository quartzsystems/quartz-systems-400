// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database: schemas and their objects as the catalog views
//! (`QSYS2`) describe them, and one SQL statement run as the user, as
//! the interactive SQL session runs it.

use qsweb::msg::Message;

/// How a statement runs: the default schema, naming and isolation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlOptions {
    /// The default schema (`HLINV`).
    pub schema: String,
    /// `*SQL` or `*SYS`.
    pub naming: String,
    /// `*NONE`, `*CHG`, `*CS` or `*ALL`.
    pub isolation: String,
}

impl Default for SqlOptions {
    fn default() -> Self {
        Self {
            schema: String::new(),
            naming: "*SQL".into(),
            isolation: "*NONE".into(),
        }
    }
}

/// What one statement left.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SqlOutcome {
    /// A query: it has a result table.
    pub query: bool,
    /// The result's column names.
    pub columns: Vec<String>,
    /// The result's rows, `None` for a null.
    pub rows: Vec<Vec<Option<String>>>,
    /// More rows than asked for were left out.
    pub truncated: bool,
    /// Milliseconds the statement took.
    pub elapsed_ms: u64,
    /// Its messages: a completion, a warning or the error that stopped it.
    pub messages: Vec<Message>,
}

impl SqlOutcome {
    /// Whether an error stopped the statement.
    pub fn failed(&self) -> bool {
        self.messages
            .iter()
            .any(|m| m.kind == qsweb::msg::Kind::Error)
    }
}

/// A schema (`QSYS2.SYSSCHEMAS`, `DSPLIB`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Schema {
    /// The name.
    pub name: String,
    /// The library's text.
    pub text: String,
    /// The owner.
    pub owner: String,
    /// The journal its tables are journaled to (`HLINV/QSQJRN`), blank
    /// for none.
    pub journal: String,
    /// Bytes.
    pub size: u64,
    /// Created, seconds since 1970.
    pub created: u64,
}

/// The kinds of objects a schema lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjKind {
    /// Tables (physical files).
    Tables,
    /// Views (logical files without keys).
    Views,
    /// Indexes.
    Indexes,
    /// Procedures and functions.
    Procedures,
}

impl ObjKind {
    /// Every kind, in the tree's order.
    pub const ALL: [ObjKind; 4] = [
        ObjKind::Tables,
        ObjKind::Views,
        ObjKind::Indexes,
        ObjKind::Procedures,
    ];

    /// The tree's label.
    pub fn label(self) -> &'static str {
        match self {
            ObjKind::Tables => "Tables",
            ObjKind::Views => "Views",
            ObjKind::Indexes => "Indexes",
            ObjKind::Procedures => "Procedures",
        }
    }

    /// The `?show=` value.
    pub fn key(self) -> &'static str {
        match self {
            ObjKind::Tables => "tables",
            ObjKind::Views => "views",
            ObjKind::Indexes => "indexes",
            ObjKind::Procedures => "procedures",
        }
    }

    /// The kind of a `?show=` value.
    pub fn of(key: &str) -> Option<ObjKind> {
        Self::ALL.into_iter().find(|k| k.key() == key)
    }
}

/// One object of a schema (`QSYS2.SYSTABLES`, `SYSTABLESTAT`,
/// `SYSINDEXES`, `SYSROUTINES`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DbObject {
    /// The SQL name.
    pub name: String,
    /// Its text.
    pub text: String,
    /// Rows (tables), 0 otherwise.
    pub rows: u64,
    /// Bytes.
    pub size: u64,
    /// Last changed, seconds since 1970.
    pub changed: u64,
    /// Journaled.
    pub journaled: bool,
    /// The table a view or index is over, blank for others.
    pub over: String,
}

/// A column (`QSYS2.SYSCOLUMNS`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Column {
    /// The SQL name.
    pub name: String,
    /// The system (field) name.
    pub system_name: String,
    /// `CHAR`, `VARCHAR`, `DECIMAL`, ...
    pub typ: String,
    /// The length (digits for numbers, bytes otherwise).
    pub length: u32,
    /// The scale of a decimal, `None` for others.
    pub scale: Option<u32>,
    /// Nulls allowed.
    pub nullable: bool,
    /// The default as SQL shows it (`'ACTIVE'`, `NULL`), blank for none.
    pub default: String,
    /// Its text.
    pub text: String,
}

/// A constraint (`QSYS2.SYSCST`, `SYSKEYCST`, `SYSREFCST`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Constraint {
    /// The name.
    pub name: String,
    /// `PRIMARY KEY`, `UNIQUE`, `FOREIGN KEY` or `CHECK`.
    pub kind: String,
    /// Its columns, or a check's condition.
    pub columns: String,
    /// The table a foreign key refers to, blank otherwise.
    pub references: String,
}

/// An index over a table (`QSYS2.SYSINDEXES`, `SYSKEYS`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Index {
    /// The name.
    pub name: String,
    /// Its schema.
    pub schema: String,
    /// Its key columns.
    pub columns: String,
    /// Unique keys.
    pub unique: bool,
    /// Its text.
    pub text: String,
}

/// A trigger (`QSYS2.SYSTRIGGERS`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trigger {
    /// The name.
    pub name: String,
    /// `INSERT`, `UPDATE`, `DELETE`.
    pub event: String,
    /// `BEFORE` or `AFTER`.
    pub time: String,
    /// The program or SQL body that runs.
    pub program: String,
    /// Enabled.
    pub enabled: bool,
}

/// One table: its catalog entry, columns, constraints, indexes,
/// triggers and file attributes (`DSPFD`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableDesc {
    /// The schema.
    pub schema: String,
    /// The catalog entry.
    pub table: DbObject,
    /// The system (file) name.
    pub system_name: String,
    /// The owner.
    pub owner: String,
    /// Created, seconds since 1970.
    pub created: u64,
    /// The record format.
    pub format: String,
    /// Deleted rows not yet reused.
    pub deleted: u64,
    /// The journal (`HLINV/QSQJRN`), blank for none.
    pub journal: String,
    /// The columns, in order.
    pub columns: Vec<Column>,
    /// Constraints.
    pub constraints: Vec<Constraint>,
    /// Indexes over the table.
    pub indexes: Vec<Index>,
    /// Triggers.
    pub triggers: Vec<Trigger>,
}

impl TableDesc {
    /// The primary key's columns, as written.
    pub fn primary_key(&self) -> Option<&str> {
        self.constraints
            .iter()
            .find(|c| c.kind == "PRIMARY KEY")
            .map(|c| c.columns.as_str())
    }
}

/// What the database pages need, as the user.
pub trait Db: Send + Sync {
    /// The schemas the user may see, by name.
    fn schemas(&self, user: &str) -> Vec<Schema>;
    /// The schemas Run SQL offers, the default first.
    fn sql_schemas(&self, user: &str) -> Vec<String>;
    /// A schema's objects of one kind.
    fn objects(&self, user: &str, schema: &str, kind: ObjKind) -> Vec<DbObject>;
    /// One table.
    fn table(&self, user: &str, schema: &str, table: &str) -> Option<TableDesc>;
    /// The SQL that creates a table (`QSQGNDDL`).
    fn ddl(&self, user: &str, schema: &str, table: &str) -> Result<String, Message>;
    /// Run one statement as the user; a query keeps at most `max_rows`
    /// rows (`None`: all).
    fn run_sql(
        &self,
        user: &str,
        statement: &str,
        opts: &SqlOptions,
        max_rows: Option<usize>,
    ) -> SqlOutcome;
    /// How the statement would run: the access plan's lines.
    fn explain(
        &self,
        user: &str,
        statement: &str,
        opts: &SqlOptions,
    ) -> Result<Vec<String>, Message>;
}
