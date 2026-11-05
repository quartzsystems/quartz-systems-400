// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The numbers a CLI program passes and gets back, as IBM i defines
//! them (read from the system's header on pub400; the names here are
//! ours). Only the values this implementation takes are listed.

/// What every function returns.
pub mod rc {
    /// It worked.
    pub const SUCCESS: i32 = 0;
    /// It worked, with a warning in the diagnostics.
    pub const SUCCESS_WITH_INFO: i32 = 1;
    /// No (more) data.
    pub const NO_DATA: i32 = 100;
    /// It failed; the diagnostics say why.
    pub const ERROR: i32 = -1;
    /// The handle is not one.
    pub const INVALID_HANDLE: i32 = -2;
}

/// Handle types.
pub mod htype {
    /// The environment.
    pub const ENV: i16 = 1;
    /// A connection.
    pub const DBC: i16 = 2;
    /// A statement.
    pub const STMT: i16 = 3;
    /// A descriptor.
    pub const DESC: i16 = 4;
}

/// Lengths and indicators.
pub mod len {
    /// A string that ends at its first NUL.
    pub const NTS: i32 = -3;
    /// The value is null.
    pub const NULL_DATA: i32 = -1;
    /// The value comes later (`SQLPutData`).
    pub const DATA_AT_EXEC: i32 = -2;
}

/// SQL data types, which double as the C types of the buffers.
pub mod ty {
    /// The column's own type.
    pub const DEFAULT: i16 = 99;
    /// Fixed-length characters.
    pub const CHAR: i16 = 1;
    /// Zoned decimal.
    pub const NUMERIC: i16 = 2;
    /// Packed decimal.
    pub const DECIMAL: i16 = 3;
    /// 4-byte integer.
    pub const INTEGER: i16 = 4;
    /// 2-byte integer.
    pub const SMALLINT: i16 = 5;
    /// 8-byte float (`FLOAT` without a precision).
    pub const FLOAT: i16 = 6;
    /// 4-byte float.
    pub const REAL: i16 = 7;
    /// 8-byte float.
    pub const DOUBLE: i16 = 8;
    /// The datetime class of `SQLDescribeCol`.
    pub const DATETIME: i16 = 9;
    /// Varying-length characters: a 2-byte length, then the data.
    pub const VARCHAR: i16 = 12;
    /// Binary large object.
    pub const BLOB: i16 = 13;
    /// Character large object.
    pub const CLOB: i16 = 14;
    /// Double-byte large object.
    pub const DBCLOB: i16 = 15;
    /// Boolean.
    pub const BOOLEAN: i16 = 16;
    /// UTF-16 characters.
    pub const WCHAR: i16 = 17;
    /// Varying UTF-16 characters.
    pub const WVARCHAR: i16 = 18;
    /// 8-byte integer.
    pub const BIGINT: i16 = 19;
    /// Graphic characters.
    pub const GRAPHIC: i16 = 95;
    /// Varying graphic characters.
    pub const VARGRAPHIC: i16 = 96;
    /// Fixed-length bytes.
    pub const BINARY: i16 = -2;
    /// Varying-length bytes.
    pub const VARBINARY: i16 = -3;
    /// A date.
    pub const DATE: i16 = 91;
    /// A time.
    pub const TIME: i16 = 92;
    /// A timestamp.
    pub const TIMESTAMP: i16 = 93;
    /// Decimal floating point.
    pub const DECFLOAT: i16 = -360;
    /// XML.
    pub const XML: i16 = -370;
}

/// `SQLDescribeCol`'s and `SQLDescribeParam`'s nullability.
pub mod null {
    /// Never null.
    pub const NO_NULLS: i16 = 0;
    /// May be null.
    pub const NULLABLE: i16 = 1;
}

/// `SQLFreeStmt` options.
pub mod free {
    /// Close the cursor.
    pub const CLOSE: i16 = 0;
    /// Free the handle.
    pub const DROP: i16 = 1;
    /// Forget the bound columns.
    pub const UNBIND: i16 = 2;
    /// Forget the bound parameters.
    pub const RESET_PARAMS: i16 = 3;
}

/// `SQLEndTran` and `SQLTransact` completion types.
pub mod tran {
    /// Commit.
    pub const COMMIT: i16 = 0;
    /// Roll back.
    pub const ROLLBACK: i16 = 1;
    /// Commit, cursors kept.
    pub const COMMIT_HOLD: i16 = 2;
    /// Roll back, cursors kept.
    pub const ROLLBACK_HOLD: i16 = 3;
}

/// Environment attributes.
pub mod env_attr {
    /// Character output ends with a NUL.
    pub const OUTPUT_NTS: i32 = 10001;
    /// System naming for new connections.
    pub const SYS_NAMING: i32 = 10002;
    /// The default library for new connections.
    pub const DEFAULT_LIB: i32 = 10003;
    /// Server mode: each connection's statements run in a job of its own.
    pub const SERVER_MODE: i32 = 10004;
    /// How many times `SQLAllocEnv` gave the handle out.
    pub const ENVHNDL_COUNTER: i32 = 10009;
}

/// Connection attributes.
pub mod conn_attr {
    /// The commitment control level (`SQL_TXN_*`).
    pub const COMMIT: i32 = 0;
    /// Read only or read and write.
    pub const ACCESS_MODE: i32 = 10002;
    /// Commit after each statement.
    pub const AUTOCOMMIT: i32 = 10003;
    /// System naming.
    pub const DBC_SYS_NAMING: i32 = 10004;
    /// The default library for unqualified names.
    pub const DBC_DEFAULT_LIB: i32 = 10005;
}

/// Statement attributes.
pub mod stmt_attr {
    /// `FOR FETCH ONLY`.
    pub const FOR_FETCH_ONLY: i32 = 10014;
    /// A scrollable cursor.
    pub const CURSOR_SCROLLABLE: i32 = 10015;
    /// Rows a fetch returns (only 1).
    pub const ROWSET_SIZE: i32 = 10016;
    /// `WITH HOLD`.
    pub const CURSOR_HOLD: i32 = 10017;
    /// The cursor's sensitivity.
    pub const CURSOR_SENSITIVITY: i32 = 10051;
    /// The cursor's type.
    pub const CURSOR_TYPE: i32 = 10050;
}

/// The commitment control levels.
pub mod txn {
    /// No commitment control.
    pub const NO_COMMIT: i32 = 1;
    /// `*CHG`.
    pub const READ_UNCOMMITTED: i32 = 2;
    /// `*CS`.
    pub const READ_COMMITTED: i32 = 3;
    /// `*ALL`.
    pub const REPEATABLE_READ: i32 = 4;
    /// `*RR`.
    pub const SERIALIZABLE: i32 = 5;
}

/// `SQLFetchScroll` orientations.
pub mod fetch {
    /// The next row.
    pub const NEXT: i16 = 1;
    /// The first row.
    pub const FIRST: i16 = 2;
    /// The last row.
    pub const LAST: i16 = 3;
    /// The row before.
    pub const PRIOR: i16 = 4;
    /// Row `n`.
    pub const ABSOLUTE: i16 = 5;
    /// `n` rows on.
    pub const RELATIVE: i16 = 6;
}

/// `SQLBindParameter`'s parameter types.
pub mod param {
    /// An input.
    pub const INPUT: i16 = 1;
    /// An output.
    pub const OUTPUT: i16 = 2;
    /// Both.
    pub const INPUT_OUTPUT: i16 = 3;
}

/// `SQLGetDiagField` identifiers.
pub mod diag {
    /// The function's return code.
    pub const RETURNCODE: i16 = 1;
    /// How many records.
    pub const NUMBER: i16 = 2;
    /// The rows the statement changed.
    pub const ROW_COUNT: i16 = 3;
    /// A record's SQLSTATE.
    pub const SQLSTATE: i16 = 4;
    /// A record's native error (the SQLCODE).
    pub const NATIVE: i16 = 5;
    /// A record's message text.
    pub const MESSAGE_TEXT: i16 = 6;
}

/// The native error of a condition the CLI itself detects.
pub const CLI_NATIVE: i32 = -99999;

/// `SQL_TRUE`.
pub const TRUE: i32 = 1;
