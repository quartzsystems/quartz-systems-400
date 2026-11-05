// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Data definition statements.

use super::expr::{Expr, TypeSpec};
use super::query::Query;
use super::{Ident, ObjectName};

/// A column's default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ColumnDefault {
    /// `WITH DEFAULT` or `DEFAULT` alone: the type's default.
    System,
    /// `DEFAULT value`.
    Value(Expr),
}

/// `GENERATED ... AS IDENTITY`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// `ALWAYS` (else `BY DEFAULT`).
    pub always: bool,
    /// `START WITH`.
    pub start: i64,
    /// `INCREMENT BY`.
    pub increment: i64,
    /// `CYCLE`.
    pub cycle: bool,
}

/// A column definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnDef {
    /// The SQL name.
    pub name: Ident,
    /// `FOR COLUMN` system name.
    pub system_name: Option<Ident>,
    /// The type.
    pub type_spec: TypeSpec,
    /// `NOT NULL`.
    pub not_null: bool,
    /// Default.
    pub default: Option<ColumnDefault>,
    /// Identity.
    pub identity: Option<Identity>,
    /// `PRIMARY KEY` on the column.
    pub primary_key: bool,
    /// `UNIQUE` on the column.
    pub unique: bool,
}

/// A table constraint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Constraint {
    /// `PRIMARY KEY (cols)`.
    PrimaryKey {
        /// Name.
        name: Option<Ident>,
        /// Columns.
        columns: Vec<Ident>,
    },
    /// `UNIQUE (cols)`.
    Unique {
        /// Name.
        name: Option<Ident>,
        /// Columns.
        columns: Vec<Ident>,
    },
    /// `FOREIGN KEY (cols) REFERENCES t (cols)`: parsed, not enforced.
    ForeignKey {
        /// Name.
        name: Option<Ident>,
        /// Columns.
        columns: Vec<Ident>,
        /// Parent table.
        parent: ObjectName,
        /// Parent columns.
        parent_columns: Vec<Ident>,
    },
    /// `CHECK (cond)`: parsed, not enforced.
    Check {
        /// Name.
        name: Option<Ident>,
        /// The condition.
        cond: Expr,
    },
}

/// `CREATE TABLE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateTable {
    /// The table.
    pub name: ObjectName,
    /// `FOR SYSTEM NAME`.
    pub system_name: Option<Ident>,
    /// Columns.
    pub columns: Vec<ColumnDef>,
    /// Constraints.
    pub constraints: Vec<Constraint>,
    /// `RCDFMT`.
    pub rcdfmt: Option<Ident>,
    /// `AS (query) WITH [NO] DATA`.
    pub as_query: Option<(Box<Query>, bool)>,
    /// `OR REPLACE`.
    pub replace: bool,
    /// `LIKE table`, or `(LIKE table)` (true: every column nullable).
    pub like: Option<(ObjectName, bool)>,
    /// `ON REPLACE PRESERVE ROWS` (the default) or `DELETE ROWS`.
    pub preserve_rows: bool,
    /// The column names of `CREATE TABLE t (a, b) AS (query)`.
    pub column_names: Vec<Ident>,
}

/// One `ALTER TABLE` action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AlterAction {
    /// `ADD COLUMN`.
    AddColumn(ColumnDef),
    /// `DROP COLUMN`.
    DropColumn {
        /// The column.
        name: Ident,
        /// `CASCADE`.
        cascade: bool,
    },
    /// `ALTER COLUMN ... SET DATA TYPE`.
    SetDataType(Ident, TypeSpec),
    /// `ALTER COLUMN ... SET NOT NULL` / `DROP NOT NULL`.
    SetNotNull(Ident, bool),
    /// `ALTER COLUMN ... SET DEFAULT` / `DROP DEFAULT`.
    SetDefault(Ident, Option<ColumnDefault>),
    /// `ADD constraint`.
    AddConstraint(Constraint),
    /// `DROP PRIMARY KEY`.
    DropPrimaryKey,
    /// `DROP CONSTRAINT name` / `DROP UNIQUE name`.
    DropConstraint(Ident),
}

/// `ALTER TABLE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlterTable {
    /// The table.
    pub name: ObjectName,
    /// The actions, in order.
    pub actions: Vec<AlterAction>,
}

/// `CREATE INDEX`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateIndex {
    /// The index.
    pub name: ObjectName,
    /// `UNIQUE`.
    pub unique: bool,
    /// `UNIQUE WHERE NOT NULL`.
    pub where_not_null: bool,
    /// The table.
    pub table: ObjectName,
    /// Key columns with `DESC`.
    pub columns: Vec<(Ident, bool)>,
    /// `RCDFMT`.
    pub rcdfmt: Option<Ident>,
    /// `WHERE`: the sparse condition and its text as written.
    pub sparse: Option<(Expr, String)>,
    /// `ENCODED VECTOR`.
    pub encoded_vector: bool,
    /// `INCLUDE` aggregates of an encoded vector index, as written.
    pub includes: Vec<String>,
}

/// `CREATE VIEW`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateView {
    /// The view.
    pub name: ObjectName,
    /// Column names.
    pub columns: Vec<Ident>,
    /// The query.
    pub query: Box<Query>,
    /// `WITH CHECK OPTION`.
    pub check_option: bool,
    /// `WITH LOCAL CHECK OPTION` (else `CASCADED`).
    pub local: bool,
    /// `OR REPLACE`.
    pub replace: bool,
    /// `FOR SYSTEM NAME`.
    pub system_name: Option<Ident>,
    /// The query as written.
    pub query_text: String,
    /// `RCDFMT`.
    pub rcdfmt: Option<Ident>,
}
