// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statements as parsed. Names stay as written (regular identifiers
//! upper case); resolution to objects is the planner's job.

pub mod ddl;
pub mod expr;
pub mod query;

pub use ddl::*;
pub use expr::*;
pub use query::*;

use std::fmt;

/// `NAMING(*SYS)` or `NAMING(*SQL)`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Naming {
    /// `LIB/FILE`, the library list for unqualified names.
    #[default]
    Sys,
    /// `SCHEMA.TABLE`, the current schema for unqualified names.
    Sql,
}

/// An identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Ident {
    /// The name: upper case for a regular identifier, as written for a
    /// delimited one.
    pub value: String,
    /// Written in double quotes.
    pub delimited: bool,
}

impl Ident {
    /// A regular identifier.
    #[must_use]
    pub fn new(s: &str) -> Self {
        Self {
            value: s.to_ascii_uppercase(),
            delimited: false,
        }
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.delimited {
            write!(f, "\"{}\"", self.value.replace('"', "\"\""))
        } else {
            f.write_str(&self.value)
        }
    }
}

/// A one- or two-part object name.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ObjectName {
    /// Schema or library, when qualified.
    pub schema: Option<Ident>,
    /// The object.
    pub name: Ident,
}

impl ObjectName {
    /// An unqualified name.
    #[must_use]
    pub fn new(s: &str) -> Self {
        Self {
            schema: None,
            name: Ident::new(s),
        }
    }

    /// A qualified name.
    #[must_use]
    pub fn qualified(schema: &str, s: &str) -> Self {
        Self {
            schema: Some(Ident::new(schema)),
            name: Ident::new(s),
        }
    }
}

impl fmt::Display for ObjectName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.schema {
            Some(s) => write!(f, "{s}.{}", self.name),
            None => write!(f, "{}", self.name),
        }
    }
}

/// `WITH NC`, `UR`, `CS`, `RS`, `RR`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Isolation {
    /// No commit.
    Nc,
    /// Uncommitted read.
    Ur,
    /// Cursor stability.
    Cs,
    /// Read stability.
    Rs,
    /// Repeatable read.
    Rr,
}

/// A statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Statement {
    /// `CREATE TABLE`.
    CreateTable(CreateTable),
    /// `ALTER TABLE`.
    AlterTable(AlterTable),
    /// `DROP TABLE`.
    DropTable {
        /// The table.
        name: ObjectName,
        /// `CASCADE` (else `RESTRICT`).
        cascade: bool,
    },
    /// `CREATE INDEX`.
    CreateIndex(CreateIndex),
    /// `DROP INDEX`.
    DropIndex(ObjectName),
    /// `CREATE VIEW`.
    CreateView(CreateView),
    /// `DROP VIEW`.
    DropView(ObjectName),
    /// `CREATE ALIAS`.
    CreateAlias {
        /// The alias.
        name: ObjectName,
        /// The table.
        table: ObjectName,
        /// The member.
        member: Option<Ident>,
        /// `OR REPLACE`.
        replace: bool,
    },
    /// `DROP ALIAS`.
    DropAlias(ObjectName),
    /// `LABEL ON`.
    LabelOn(Label),
    /// `COMMENT ON`.
    CommentOn(Label),
    /// `RENAME`.
    Rename(Rename),
    /// A query.
    Select(Query),
    /// `INSERT`.
    Insert(Insert),
    /// `UPDATE`.
    Update(Update),
    /// `DELETE`.
    Delete(Delete),
    /// `MERGE`.
    Merge(Merge),
    /// `DECLARE ... CURSOR`.
    DeclareCursor(DeclareCursor),
    /// `OPEN`.
    Open {
        /// The cursor.
        cursor: Ident,
        /// `USING` host variables or values.
        using: Vec<Expr>,
    },
    /// `FETCH`.
    Fetch(Fetch),
    /// `CLOSE`.
    Close(Ident),
    /// `COMMIT`.
    Commit {
        /// `HOLD`.
        hold: bool,
    },
    /// `ROLLBACK`.
    Rollback {
        /// `HOLD`.
        hold: bool,
    },
    /// `SET SCHEMA`.
    SetSchema(SchemaTarget),
    /// `SET PATH`.
    SetPath(Vec<PathEntry>),
    /// `SET CURRENT DEGREE`: accepted, ignored.
    SetCurrentDegree,
    /// `EXPLAIN` (Quartz's): the plan of the statement as text.
    Explain(Box<Statement>),
}

/// `SET SCHEMA` argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaTarget {
    /// A name.
    Name(Ident),
    /// `USER`.
    User,
    /// `DEFAULT`.
    Default,
    /// A host variable.
    Host(String),
}

/// `SET PATH` entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathEntry {
    /// A schema.
    Name(Ident),
    /// `*LIBL`.
    Libl,
    /// `SYSTEM PATH`.
    SystemPath,
    /// `USER`.
    User,
    /// `CURRENT PATH`.
    CurrentPath,
}

/// `DECLARE CURSOR`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclareCursor {
    /// The cursor.
    pub name: Ident,
    /// `SCROLL`.
    pub scroll: bool,
    /// `WITH HOLD`.
    pub hold: bool,
    /// The query.
    pub query: Query,
}

/// `FETCH` orientation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchOrientation {
    /// `NEXT`.
    Next,
    /// `PRIOR`.
    Prior,
    /// `FIRST`.
    First,
    /// `LAST`.
    Last,
    /// `BEFORE`.
    Before,
    /// `AFTER`.
    After,
    /// `CURRENT`.
    Current,
    /// `RELATIVE n`.
    Relative(Expr),
    /// `ABSOLUTE n`.
    Absolute(Expr),
}

/// `FETCH`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetch {
    /// Orientation.
    pub orientation: FetchOrientation,
    /// The cursor.
    pub cursor: Ident,
    /// `INTO` host variables.
    pub into: Vec<Expr>,
}

/// `LABEL ON` or `COMMENT ON`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    /// What is labelled.
    pub target: LabelTarget,
    /// Per column when the statement labels columns: the column, the
    /// text, and whether it is `TEXT IS` (else a heading).
    pub columns: Vec<(Ident, String, bool)>,
    /// The text, for a table or a single column.
    pub text: Option<String>,
    /// `TEXT IS` (column heading otherwise).
    pub is_text: bool,
}

/// The object a label is on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabelTarget {
    /// A table or view.
    Table(ObjectName),
    /// A column.
    Column(ObjectName, Ident),
    /// An index.
    Index(ObjectName),
    /// An alias.
    Alias(ObjectName),
}

/// `RENAME`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rename {
    /// `TABLE` or `INDEX`.
    pub index: bool,
    /// The object.
    pub name: ObjectName,
    /// `TO`.
    pub to: Option<Ident>,
    /// `FOR SYSTEM NAME`.
    pub system_name: Option<Ident>,
}
