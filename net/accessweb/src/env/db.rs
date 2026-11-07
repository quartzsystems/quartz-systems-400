// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database: the catalog, statements run in a connection of a
//! page's own, and the saved requests and history (handoff steps 7-12).
//! Everything runs as the signed-on user: the running system answers
//! through QAP's `Sql` service in the `ACCESSWEB` job, swapped to the
//! user's profile; the fixture from its sample tables.

use qsweb::msg::Message;

/// What a catalog entry is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    /// An SQL table.
    #[default]
    Table,
    /// An SQL view.
    View,
    /// An alias.
    Alias,
    /// A physical file not made by SQL.
    Physical,
    /// A logical file not made by SQL.
    Logical,
}

impl Kind {
    /// Every kind, in the Objects select's order.
    pub const ALL: [Kind; 5] = [Kind::Table, Kind::View, Kind::Alias, Kind::Physical, Kind::Logical];

    /// `Table`, `Logical file`.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Table => "Table",
            Kind::View => "View",
            Kind::Alias => "Alias",
            Kind::Physical => "Physical file",
            Kind::Logical => "Logical file",
        }
    }

    /// The Objects select's choice (`Tables`).
    pub fn plural(self) -> &'static str {
        match self {
            Kind::Table => "Tables",
            Kind::View => "Views",
            Kind::Alias => "Aliases",
            Kind::Physical => "Physical files",
            Kind::Logical => "Logical files",
        }
    }
}

/// A table, view, alias or file of a schema.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// The SQL name.
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// Its label or text.
    pub text: String,
    /// Rows, as the catalog's statistics say.
    pub rows: u64,
    /// Last changed, seconds since 1970; 0 when the catalog has none.
    pub changed: u64,
}

/// A column of a table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Column {
    /// The SQL name.
    pub name: String,
    /// `CHAR`, `VARCHAR`, `DECIMAL`, `DATE`, ...
    pub typ: String,
    /// Length, or precision for decimals.
    pub length: u32,
    /// The scale of a decimal.
    pub scale: Option<u32>,
    /// Nulls allowed.
    pub nullable: bool,
    /// The default as SQL writes it (`'ACTIVE'`, `CURRENT TIMESTAMP`);
    /// empty for none.
    pub default: String,
    /// Part of the primary key (or the first unique key).
    pub key: bool,
    /// The values a check constraint or a referenced table allows, when
    /// there are few; empty otherwise.
    pub choices: Vec<String>,
}

impl Column {
    /// `CHAR(10)`, `DECIMAL(11,2)`, `DATE`.
    pub fn type_label(&self) -> String {
        match (self.typ.as_str(), self.scale) {
            ("DECIMAL" | "NUMERIC", Some(s)) => format!("{}({},{s})", self.typ, self.length),
            ("CHAR" | "VARCHAR" | "GRAPHIC" | "VARGRAPHIC" | "BINARY" | "VARBINARY" | "CLOB" | "BLOB" | "DBCLOB", _) => {
                format!("{}({})", self.typ, self.length)
            }
            _ => self.typ.clone(),
        }
    }

    /// A number type: its values are right-aligned.
    pub fn numeric(&self) -> bool {
        is_numeric(&self.typ)
    }

    /// NOT NULL without a default: a value must be given.
    pub fn required(&self) -> bool {
        !self.nullable && self.default.is_empty()
    }

    /// A value the system makes (`CURRENT TIMESTAMP`, an identity): the
    /// form does not ask for it.
    pub fn generated(&self) -> bool {
        let d = self.default.to_ascii_uppercase();
        d.starts_with("CURRENT") || d.starts_with("GENERATED") || d == "IDENTITY"
    }
}

/// Whether SQL type `typ` is a number.
pub fn is_numeric(typ: &str) -> bool {
    matches!(
        typ,
        "SMALLINT" | "INTEGER" | "INT" | "BIGINT" | "DECIMAL" | "NUMERIC" | "DECFLOAT" | "REAL" | "DOUBLE" | "FLOAT"
    )
}

/// One value of a result or a parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Val {
    /// NULL.
    Null,
    /// A number, exactly as the system writes it.
    Num(String),
    /// Text (character, date, time, timestamp).
    Text(String),
    /// Binary data (and character data of CCSID 65535).
    Bin(Vec<u8>),
}

impl Val {
    /// The value as text: NULL empty, binary as hex.
    pub fn text(&self) -> String {
        match self {
            Val::Null => String::new(),
            Val::Num(s) | Val::Text(s) => s.clone(),
            Val::Bin(b) => b.iter().fold(String::new(), |mut s, x| {
                use std::fmt::Write as _;
                let _ = write!(s, "{x:02X}");
                s
            }),
        }
    }

    /// Whether it is NULL.
    pub fn is_null(&self) -> bool {
        matches!(self, Val::Null)
    }
}

/// A commit level, as Run SQL offers them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Commit {
    /// No commitment control.
    #[default]
    None,
    /// `*CHG`.
    ReadUncommitted,
    /// `*CS`.
    ReadCommitted,
    /// `*ALL`.
    RepeatableRead,
    /// `*RR`.
    Serializable,
}

impl Commit {
    /// Every level, in the select's order.
    pub const ALL: [Commit; 5] = [
        Commit::None,
        Commit::ReadUncommitted,
        Commit::ReadCommitted,
        Commit::RepeatableRead,
        Commit::Serializable,
    ];

    /// `Read committed`.
    pub fn label(self) -> &'static str {
        match self {
            Commit::None => "None",
            Commit::ReadUncommitted => "Read uncommitted",
            Commit::ReadCommitted => "Read committed",
            Commit::RepeatableRead => "Repeatable read",
            Commit::Serializable => "Serializable",
        }
    }

    /// The level of a label.
    pub fn of(label: &str) -> Option<Commit> {
        Self::ALL.into_iter().find(|c| c.label() == label)
    }
}

/// A connection's options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// The default schema; empty for none.
    pub schema: String,
    /// System naming (`library/file`) rather than SQL (`schema.table`).
    pub system_naming: bool,
    /// The commit level.
    pub commit: Commit,
}

/// A result column.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResultColumn {
    /// The SQL name.
    pub name: String,
    /// The SQL type.
    pub typ: String,
}

impl ResultColumn {
    /// A number column.
    pub fn numeric(&self) -> bool {
        is_numeric(&self.typ)
    }
}

/// What one statement did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// A query's columns; empty for any other statement.
    pub columns: Vec<ResultColumn>,
    /// The rows fetched.
    pub rows: Vec<Vec<Val>>,
    /// A query's cursor, while rows are left to fetch.
    pub cursor: Option<u32>,
    /// Rows changed (not a query).
    pub changed: i64,
    /// The statement's message: its completion, warning or error.
    pub message: Option<Message>,
    /// SQLSTATE.
    pub sqlstate: String,
    /// SQLCODE.
    pub sqlcode: i32,
    /// The statement failed.
    pub failed: bool,
    /// The parameter row that failed, from 0.
    pub row: Option<usize>,
    /// Milliseconds it took.
    pub ms: u64,
}

impl Outcome {
    /// Whether it was a query.
    pub fn query(&self) -> bool {
        !self.columns.is_empty()
    }
}

/// A saved request's type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReqType {
    /// An SQL script.
    #[default]
    Sql,
    /// A copy to a table (an upload `.qdx`).
    CopyTo,
    /// A copy from a table (a download `.qdx`).
    CopyFrom,
}

impl ReqType {
    /// `SQL`, `Copy data to table`.
    pub fn label(self) -> &'static str {
        match self {
            ReqType::Sql => "SQL",
            ReqType::CopyTo => "Copy data to table",
            ReqType::CopyFrom => "Copy data from table",
        }
    }

    /// The key kept in the store (`sql`, `to`, `from`).
    pub fn key(self) -> &'static str {
        match self {
            ReqType::Sql => "sql",
            ReqType::CopyTo => "to",
            ReqType::CopyFrom => "from",
        }
    }

    /// The type of a store key.
    pub fn of_key(k: &str) -> Option<ReqType> {
        [ReqType::Sql, ReqType::CopyTo, ReqType::CopyFrom].into_iter().find(|t| t.key() == k)
    }
}

/// A saved request.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Request {
    /// Its number, unique for its owner.
    pub id: u32,
    /// The owner.
    pub owner: String,
    /// Its name.
    pub name: String,
    /// SQL, copy to or copy from.
    pub typ: ReqType,
    /// An SQL request's script, or a copy request's `.qdx` text.
    pub body: String,
    /// The output format (`HTML`, `CSV`, `XLSX`).
    pub format: String,
    /// Last run, seconds since 1970; 0 for never.
    pub last_run: u64,
    /// The group or `*PUBLIC` it is shared with; empty for none.
    pub shared: String,
}

/// The database, as the signed-on user.
pub trait Db: Send + Sync {
    /// The schemas whose names match `pattern` (SQL `LIKE`, upper
    /// case), in order.
    fn schemas(&self, user: &str, pattern: &str) -> Vec<String>;
    /// The tables, views, aliases and files of `schema`, in the
    /// catalog's order.
    fn tables(&self, user: &str, schema: &str) -> Result<Vec<Entry>, Message>;
    /// A table's columns, in order.
    fn columns(&self, user: &str, schema: &str, table: &str) -> Result<Vec<Column>, Message>;
    /// A connection of a page's own (at most 16 a session, `QAD7003`).
    fn open(&self, user: &str) -> Result<u32, Message>;
    /// End a connection: changes not committed roll back.
    fn close(&self, user: &str, conn: u32);
    /// Set a connection's options (refused while changes are pending).
    fn options(&self, user: &str, conn: u32, opts: &Options) -> Result<(), Message>;
    /// Run one statement, once for each parameter row (once without
    /// parameters for none); a query keeps its first `max_rows` rows and
    /// its cursor while there are more.
    fn run(&self, user: &str, conn: u32, sql: &str, params: &[Vec<Val>], max_rows: usize) -> Outcome;
    /// The next `max_rows` rows of an open cursor; the cursor is gone
    /// when there are no more.
    fn fetch(&self, user: &str, conn: u32, cursor: u32, max_rows: usize) -> Outcome;
    /// Commit (or roll back) the connection's changes.
    fn end_unit(&self, user: &str, conn: u32, commit: bool) -> Outcome;
    /// Ask the statement running in the connection to end.
    fn cancel(&self, user: &str, conn: u32);
    /// The requests `user` owns, and those shared with one of `groups`
    /// or `*PUBLIC`.
    fn requests(&self, user: &str, groups: &[String]) -> Vec<Request>;
    /// Keep a request (a new one with id 0); its id.
    fn save_request(&self, user: &str, r: &Request) -> Result<u32, Message>;
    /// Remove one of `user`'s requests.
    fn delete_request(&self, user: &str, id: u32) -> Result<(), Message>;
    /// The statements `user` ran, newest first (at most 100).
    fn history(&self, user: &str) -> Vec<String>;
    /// Remember statements `user` ran.
    fn remember(&self, user: &str, statements: &[String]);
}
