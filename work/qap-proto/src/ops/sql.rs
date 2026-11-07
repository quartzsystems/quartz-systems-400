// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Sql` (version 4): statements, cursors, commitment control and the
//! catalog. `Commit` and `Rollback` have no bodies. Flags are `bool`s:
//! one byte, 0 or 1, any other value does not decode.

use ipc::{wire_enum, wire_struct};

/// The commitment control level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CommitLevel {
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

wire_enum!(CommitLevel {
    0 => None,
    1 => ReadUncommitted,
    2 => ReadCommitted,
    3 => RepeatableRead,
    4 => Serializable,
});

/// How names are qualified.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Naming {
    /// `schema.table`.
    #[default]
    Sql,
    /// `library/file`.
    System,
}

wire_enum!(Naming {
    0 => Sql,
    1 => System,
});

/// A value in a row or a parameter set.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum SqlValue {
    /// The null value.
    #[default]
    Null,
    /// Integer types.
    Integer {
        /// The value.
        value: i64,
    },
    /// `DECIMAL`, `NUMERIC` and `DECFLOAT`: exact text, `.` as the
    /// point; `DECFLOAT` may use E notation, `Infinity`, `-Infinity`,
    /// `NaN` or `sNaN`.
    Decimal {
        /// The text.
        text: String,
    },
    /// `REAL` and `DOUBLE`: the shortest text that reads back as the
    /// same value.
    Float {
        /// The text.
        text: String,
    },
    /// Character and graphic types, as UTF-8.
    Text {
        /// The text.
        text: String,
    },
    /// Binary types, and character and graphic types with CCSID 65535.
    Binary {
        /// The bytes as stored.
        data: Vec<u8>,
    },
    /// A date.
    Date {
        /// Midnight UTC of the date, milliseconds since 1970-01-01.
        ms: i64,
    },
    /// A time.
    Time {
        /// Milliseconds since midnight.
        ms: i64,
    },
    /// A timestamp, as if UTC.
    Timestamp {
        /// Milliseconds since 1970-01-01.
        ms: i64,
        /// Picoseconds past `ms`, 0 to 999 999 999: `TIMESTAMP(12)`.
        picos: u32,
    },
    /// A Boolean.
    Boolean {
        /// The value.
        value: bool,
    },
}

wire_enum!(SqlValue {
    0 => Null,
    1 => Integer { value },
    2 => Decimal { text },
    3 => Float { text },
    4 => Text { text },
    5 => Binary { data },
    6 => Date { ms },
    7 => Time { ms },
    8 => Timestamp { ms, picos },
    9 => Boolean { value },
});

/// The largest `picos` of a timestamp.
pub const MAX_PICOS: u32 = 999_999_999;

/// A result column or a parameter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Column {
    /// Its name.
    pub name: String,
    /// Its label (column heading).
    pub label: String,
    /// The SQL type (`DECIMAL`, `TIMESTAMP`).
    pub sql_type: String,
    /// Length or precision.
    pub length: u32,
    /// Scale; a timestamp's fractional digits.
    pub scale: u16,
    /// Whether it may be null.
    pub nullable: bool,
    /// Its CCSID; 65535 for binary character data, 0 for non-character.
    pub ccsid: u16,
}

wire_struct!(Column {
    name,
    label,
    sql_type,
    length,
    scale,
    nullable,
    ccsid
});

/// The body of an `SqlError` reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SqlFailure {
    /// SQLSTATE, five characters.
    pub sqlstate: String,
    /// SQLCODE.
    pub sqlcode: i32,
    /// The failing parameter row of an `Execute`, 0-based.
    pub row: Option<u32>,
}

wire_struct!(SqlFailure {
    sqlstate,
    sqlcode,
    row
});

/// What a catalog entry is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TableKind {
    /// An SQL table.
    #[default]
    Table,
    /// An SQL view.
    View,
    /// An alias.
    Alias,
    /// A physical file not made by SQL.
    PhysicalFile,
    /// A logical file not made by SQL.
    LogicalFile,
}

wire_enum!(TableKind {
    0 => Table,
    1 => View,
    2 => Alias,
    3 => PhysicalFile,
    4 => LogicalFile,
});

impl TableKind {
    /// Every kind, in value order.
    pub const ALL: [TableKind; 5] = [
        TableKind::Table,
        TableKind::View,
        TableKind::Alias,
        TableKind::PhysicalFile,
        TableKind::LogicalFile,
    ];
}

/// `SetOptions`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetOptions {
    /// The commitment control level.
    pub commit: CommitLevel,
    /// The naming.
    pub naming: Naming,
    /// The schema unqualified names are in; none for the default.
    pub default_schema: Option<String>,
}

wire_struct!(SetOptions {
    commit,
    naming,
    default_schema
});

/// `Prepare`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prepare {
    /// The statement.
    pub sql: String,
}

wire_struct!(Prepare { sql });

/// `Prepare`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Prepared {
    /// The statement, for `Execute` and `Release`.
    pub statement: u32,
    /// A query's result columns; empty when not a query.
    pub columns: Vec<Column>,
    /// The parameter markers.
    pub parameters: Vec<Column>,
}

wire_struct!(Prepared {
    statement,
    columns,
    parameters
});

/// `Execute`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Execute {
    /// The prepared statement.
    pub statement: u32,
    /// One parameter set per row; empty to run once without parameters.
    pub rows: Vec<Vec<SqlValue>>,
}

wire_struct!(Execute { statement, rows });

/// `Execute`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Executed {
    /// Rows changed.
    pub updated: i64,
    /// A query's open cursor.
    pub cursor: Option<u32>,
}

wire_struct!(Executed { updated, cursor });

/// `Fetch`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fetch {
    /// The cursor.
    pub cursor: u32,
    /// At most this many rows; 0 for no limit.
    pub max_rows: u32,
}

wire_struct!(Fetch { cursor, max_rows });

/// `Fetch`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fetched {
    /// The rows.
    pub rows: Vec<Vec<SqlValue>>,
    /// The cursor has no more rows.
    pub end: bool,
}

wire_struct!(Fetched { rows, end });

/// `CloseCursor`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CursorRef {
    /// The cursor.
    pub cursor: u32,
}

wire_struct!(CursorRef { cursor });

/// `Release`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatementRef {
    /// The prepared statement.
    pub statement: u32,
}

wire_struct!(StatementRef { statement });

/// `ListSchemas`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListSchemas {
    /// An SQL `LIKE` pattern.
    pub pattern: String,
    /// The first entry, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListSchemas {
    pattern,
    start,
    max
});

/// A schema.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schema {
    /// Its name.
    pub name: String,
    /// Its description.
    pub text: String,
}

wire_struct!(Schema { name, text });

/// `ListSchemas`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Schemas {
    /// Schemas matching.
    pub total: u32,
    /// Those returned.
    pub schemas: Vec<Schema>,
}

wire_struct!(Schemas { total, schemas });

/// `ListTables`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListTables {
    /// The schema.
    pub schema: String,
    /// An SQL `LIKE` pattern.
    pub pattern: String,
    /// The kinds wanted; empty for all.
    pub kinds: Vec<TableKind>,
    /// The first entry, 0-based.
    pub start: u32,
    /// At most this many; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListTables {
    schema,
    pattern,
    kinds,
    start,
    max
});

/// A table, view, alias or file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    /// Its schema.
    pub schema: String,
    /// Its name.
    pub name: String,
    /// What it is.
    pub kind: TableKind,
    /// Its description.
    pub text: String,
}

wire_struct!(Table {
    schema,
    name,
    kind,
    text
});

/// `ListTables`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tables {
    /// Entries matching.
    pub total: u32,
    /// Those returned.
    pub tables: Vec<Table>,
}

wire_struct!(Tables { total, tables });

/// `ListColumns`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListColumns {
    /// The schema.
    pub schema: String,
    /// The table.
    pub table: String,
}

wire_struct!(ListColumns { schema, table });

/// `ListColumns`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Columns {
    /// The columns, in order.
    pub columns: Vec<Column>,
}

wire_struct!(Columns { columns });
