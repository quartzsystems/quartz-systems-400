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

/// `GENERATED ... AS IDENTITY` (and an identity column's or sequence's
/// options): values as written; `None` for the default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Identity {
    /// `ALWAYS` (else `BY DEFAULT`).
    pub always: bool,
    /// `START WITH`.
    pub start: Option<i128>,
    /// `INCREMENT BY`.
    pub increment: Option<i128>,
    /// `MINVALUE n`; `Some(None)` for `NO MINVALUE`.
    pub min: Option<Option<i128>>,
    /// `MAXVALUE n`; `Some(None)` for `NO MAXVALUE`.
    pub max: Option<Option<i128>>,
    /// `CYCLE` / `NO CYCLE`.
    pub cycle: Option<bool>,
    /// `CACHE n`; `Some(0)` for `NO CACHE`.
    pub cache: Option<u32>,
    /// `ORDER` / `NO ORDER`.
    pub order: Option<bool>,
    /// `RESTART [WITH n]` (alter only): `Some(None)` restarts at `START WITH`.
    pub restart: Option<Option<i128>>,
}

/// What a generated column holds, besides an identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Generated {
    /// `FOR EACH ROW ON UPDATE AS ROW CHANGE TIMESTAMP`.
    RowChangeTimestamp {
        /// `ALWAYS` (else `BY DEFAULT`).
        always: bool,
    },
    /// `AS (expression)`.
    Expr(Box<Expr>),
    /// An auditing column (7.3): `AS (DATA CHANGE OPERATION)`,
    /// `AS (SESSION_USER)` and the other special registers IBM allows.
    Audit {
        /// `ALWAYS` (else `BY DEFAULT`).
        always: bool,
        /// What it records, as IBM writes it.
        what: String,
    },
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
    /// `GENERATED ... AS (...)` and row change timestamps.
    pub generated: Option<Generated>,
    /// `IMPLICITLY HIDDEN`.
    pub hidden: bool,
    /// The column's own constraints (`PRIMARY KEY`, `UNIQUE`, `CHECK`,
    /// `REFERENCES`), as table constraints over this column.
    pub constraints: Vec<Constraint>,
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
    /// `FOREIGN KEY (cols) REFERENCES t (cols) ON DELETE ... ON UPDATE ...`.
    ForeignKey {
        /// Name.
        name: Option<Ident>,
        /// Columns.
        columns: Vec<Ident>,
        /// Parent table.
        parent: ObjectName,
        /// Parent columns; empty for its primary key.
        parent_columns: Vec<Ident>,
        /// `ON DELETE`.
        on_delete: RefRule,
        /// `ON UPDATE`: `NO ACTION` or `RESTRICT`.
        on_update: RefRule,
    },
    /// `CHECK (cond)`.
    Check {
        /// Name.
        name: Option<Ident>,
        /// The condition.
        cond: Expr,
        /// The condition as the catalogs show it: its tokens with a blank
        /// between each and after the last.
        text: String,
    },
}

impl Constraint {
    /// The constraint's name, when given.
    #[must_use]
    pub fn name(&self) -> Option<&Ident> {
        match self {
            Self::PrimaryKey { name, .. }
            | Self::Unique { name, .. }
            | Self::ForeignKey { name, .. }
            | Self::Check { name, .. } => name.as_ref(),
        }
    }
}

/// A referential constraint's delete or update rule.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RefRule {
    /// `NO ACTION`.
    #[default]
    NoAction,
    /// `RESTRICT`.
    Restrict,
    /// `CASCADE`.
    Cascade,
    /// `SET NULL`.
    SetNull,
    /// `SET DEFAULT`.
    SetDefault,
}

/// Which constraint `ALTER TABLE ... DROP` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropKind {
    /// `DROP CONSTRAINT`.
    Any,
    /// `DROP UNIQUE`.
    Unique,
    /// `DROP FOREIGN KEY`.
    ForeignKey,
    /// `DROP CHECK`.
    Check,
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
#[allow(clippy::large_enum_variant)] // one per action, briefly
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
    /// `ALTER COLUMN ... SET GENERATED ...`, `SET INCREMENT BY ...`,
    /// `RESTART [WITH]`: the identity options changed.
    AlterIdentity {
        /// The column.
        column: Ident,
        /// `SET GENERATED ALWAYS` (true) or `BY DEFAULT` (false).
        always: Option<bool>,
        /// The options given; `always` there is unused.
        options: Identity,
    },
    /// `ALTER COLUMN ... DROP IDENTITY`.
    DropIdentity(Ident),
    /// `ALTER COLUMN ... DROP GENERATED` (a row change timestamp or
    /// generated expression column becomes an ordinary one).
    DropGenerated(Ident),
    /// `ADD constraint`.
    AddConstraint(Constraint),
    /// `DROP PRIMARY KEY [CASCADE | RESTRICT]`.
    DropPrimaryKey {
        /// `CASCADE` (the default): dependent foreign keys go too.
        cascade: bool,
    },
    /// `DROP CONSTRAINT | UNIQUE | FOREIGN KEY | CHECK name [CASCADE | RESTRICT]`.
    DropConstraint {
        /// The name.
        name: Ident,
        /// What the statement said it is.
        kind: DropKind,
        /// `CASCADE` (the default).
        cascade: bool,
    },
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

/// `CREATE SEQUENCE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateSequence {
    /// The sequence.
    pub name: ObjectName,
    /// `FOR SYSTEM NAME`.
    pub system_name: Option<Ident>,
    /// `AS data-type`; `INTEGER` when not given.
    pub data_type: Option<TypeSpec>,
    /// The options.
    pub options: Identity,
    /// `OR REPLACE`.
    pub replace: bool,
}

/// `CREATE VARIABLE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateVariable {
    /// The variable.
    pub name: ObjectName,
    /// Its type.
    pub data_type: TypeSpec,
    /// `DEFAULT` or `CONSTANT`: the expression and its text.
    pub default: Option<(Expr, String)>,
    /// `CONSTANT`.
    pub constant: bool,
    /// `OR REPLACE`.
    pub replace: bool,
}

/// `CREATE TYPE ... AS` a source type: a distinct type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateType {
    /// The type.
    pub name: ObjectName,
    /// The source type; the element type of an array type.
    pub source: TypeSpec,
    /// `ARRAY[n]`: the largest cardinality (`ARRAY[]` the most there is).
    pub array: Option<u64>,
}

/// `DECLARE GLOBAL TEMPORARY TABLE`: a table in the job's `QTEMP`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct DeclareTemp {
    /// The table: its columns, `LIKE` or `AS (query)`.
    pub table: CreateTable,
    /// `ON COMMIT DELETE ROWS` (the default) rather than `PRESERVE`.
    pub delete_rows: bool,
    /// `NOT LOGGED`: changes outside commitment control.
    pub not_logged: bool,
    /// `NOT LOGGED ON ROLLBACK PRESERVE ROWS` rather than `DELETE`.
    pub rollback_preserve: bool,
    /// `WITH REPLACE`.
    pub replace: bool,
}
