// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A statement's columns and markers as the CLI describes them, from
//! the database's `DESCRIBE` rows (SQLDA types). The oracle's sizes:
//! an `INTEGER` is 4, a `SMALLINT` 2, a `BIGINT` 8, a decimal its
//! precision with its scale, a date 10; a marker is always nullable.

use sql::Value;

use crate::consts::ty;

/// A result column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColDesc {
    /// Its name.
    pub name: String,
    /// The CLI's SQL type.
    pub sql_type: i16,
    /// Its size: bytes, or a decimal's precision.
    pub size: i32,
    /// A decimal's scale.
    pub scale: i16,
    /// May be null.
    pub nullable: bool,
    /// The CCSID of character data.
    pub ccsid: u16,
}

/// A parameter marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamDesc {
    /// The CLI's SQL type.
    pub sql_type: i16,
    /// Its size.
    pub size: i32,
    /// Its scale.
    pub scale: i16,
    /// `IN` (1), `OUT` (2) or `INOUT` (3) for a `CALL`'s markers; 0 otherwise.
    pub mode: i16,
}

fn int_of(v: Option<&Value>) -> i64 {
    v.and_then(|v| v.as_i64().ok()).unwrap_or(0)
}

/// The CLI type, size and scale of an SQLDA type and length.
#[must_use]
pub fn of_sqlda(sqltype: i64, sqllen: i64, ccsid: u16) -> (i16, i32, i16) {
    let base = sqltype & !1;
    let len = i32::try_from(sqllen).unwrap_or(i32::MAX);
    let wide = matches!(ccsid, 1200 | 13488);
    let dec = || {
        (
            i32::try_from(sqllen >> 8).unwrap_or(0),
            i16::try_from(sqllen & 0xFF).unwrap_or(0),
        )
    };
    match base {
        384 => (ty::DATE, 10, 0),
        388 => (ty::TIME, 8, 0),
        392 => (ty::TIMESTAMP, len, 0),
        448 | 456 => (ty::VARCHAR, len, 0),
        452 => (ty::CHAR, len, 0),
        464 if wide => (ty::WVARCHAR, len, 0),
        464 => (ty::VARGRAPHIC, len, 0),
        468 if wide => (ty::WCHAR, len, 0),
        468 => (ty::GRAPHIC, len, 0),
        480 if len == 4 => (ty::REAL, 4, 0),
        480 => (ty::DOUBLE, 8, 0),
        484 => {
            let (p, s) = dec();
            (ty::DECIMAL, p, s)
        }
        488 => {
            let (p, s) = dec();
            (ty::NUMERIC, p, s)
        }
        492 => (ty::BIGINT, 8, 0),
        496 => (ty::INTEGER, 4, 0),
        500 => (ty::SMALLINT, 2, 0),
        404 => (ty::BLOB, len, 0),
        408 => (ty::CLOB, len, 0),
        412 => (ty::DBCLOB, len, 0),
        908 => (ty::VARBINARY, len, 0),
        912 => (ty::BINARY, len, 0),
        996 => (ty::DECFLOAT, if len == 8 { 16 } else { 34 }, 0),
        2436 => (ty::BOOLEAN, 1, 0),
        _ => (ty::VARCHAR, len, 0),
    }
}

/// The result columns of a `DESCRIBE` reply's rows.
#[must_use]
pub fn columns(rows: &[Vec<Value>]) -> Vec<ColDesc> {
    rows.iter()
        .map(|r| {
            let sqltype = int_of(r.first());
            let ccsid = u16::try_from(int_of(r.get(3))).unwrap_or(0);
            let (sql_type, size, scale) = of_sqlda(sqltype, int_of(r.get(1)), ccsid);
            ColDesc {
                name: r.get(2).map(Value::as_text).unwrap_or_default(),
                sql_type,
                size,
                scale,
                nullable: sqltype & 1 == 1,
                ccsid,
            }
        })
        .collect()
}

/// The markers of a `DESCRIBE INPUT` reply's rows.
#[must_use]
pub fn params(rows: &[Vec<Value>]) -> Vec<ParamDesc> {
    rows.iter()
        .map(|r| {
            let ccsid = u16::try_from(int_of(r.get(3))).unwrap_or(0);
            let (sql_type, size, scale) = of_sqlda(int_of(r.first()), int_of(r.get(1)), ccsid);
            ParamDesc {
                sql_type,
                size,
                scale,
                mode: i16::try_from(int_of(r.get(9))).unwrap_or(0),
            }
        })
        .collect()
}

/// The C type a column of `sql_type` is given as for `SQL_C_DEFAULT`.
#[must_use]
pub fn default_ctype(sql_type: i16) -> i16 {
    match sql_type {
        ty::DATE | ty::TIME | ty::TIMESTAMP | ty::CLOB | ty::DECFLOAT => ty::CHAR,
        ty::BLOB => ty::BINARY,
        t => t,
    }
}
