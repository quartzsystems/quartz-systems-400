// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SQL types as a compiled program holds them: the IR storage type and
//! the descriptor word the runtime's host-variable calls take (kind in
//! the top byte, length or digits in bits 8 to 55, decimals in the low
//! byte; `rpg/qrpgrt/src/value.rs`).

use std::fmt::Write as _;

use crate::ast::{DataType, TypeSpec};
use crate::error::{Result, SqlError};

/// A type's storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostType {
    /// The IR type.
    pub qir: String,
    /// The descriptor word.
    pub desc: i64,
    /// A date's or time's format word (1 `*ISO`); 0 otherwise.
    pub extra: i64,
    /// Bytes of storage.
    pub size: u32,
}

fn word(kind: i64, len: i64, dec: i64) -> i64 {
    (kind << 56) | (len << 8) | dec
}

/// The runtime's mark of an `SQLTYPE` (LOB) host variable's extra word
/// (`rpg/qrpgrt/src/sql.rs`).
const SQLTYPE: i64 = 1 << 40;

/// The type a routine's statements read a variable of `t` as, when its
/// storage is not that type's: `CAST(? AS type)` around its marker.
#[must_use]
pub fn marker_type(t: &TypeSpec) -> Option<String> {
    matches!(t.data_type, DataType::DecFloat(_) | DataType::Boolean).then(|| type_text(t))
}

/// The longest character variable a routine holds.
pub const MAX_CHAR: u32 = 16_773_104;

/// The storage of an SQL type.
///
/// # Errors
///
/// `SQL0084` for the types a compiled routine does not hold yet.
pub fn host_type(t: &TypeSpec) -> Result<HostType> {
    let unsupported = |what: &str| SqlError::Unsupported {
        what: format!("SQL PL variables and parameters of type {what}"),
    };
    if t.bit_data {
        return Err(unsupported("FOR BIT DATA"));
    }
    let graphic = matches!(
        t.data_type,
        DataType::Graphic(_) | DataType::Vargraphic(_) | DataType::Dbclob(_)
    );
    if t.ccsid
        .is_some_and(|c| c != 37 && !(graphic && matches!(c, 1200 | 13488)))
    {
        return Err(unsupported("a CCSID other than 37"));
    }
    let h = |qir: String, desc: i64, extra: i64, size: u32| HostType {
        qir,
        desc,
        extra,
        size,
    };
    Ok(match &t.data_type {
        DataType::SmallInt => h("i16".into(), word(6, 5, 0), 0, 2),
        DataType::Integer => h("i32".into(), word(6, 10, 0), 0, 4),
        DataType::BigInt => h("i64".into(), word(6, 20, 0), 0, 8),
        DataType::Decimal(p, s) => h(
            format!("packed({p},{s})"),
            word(3, (*p).into(), (*s).into()),
            0,
            u32::from(*p) / 2 + 1,
        ),
        DataType::Numeric(p, s) => h(
            format!("zoned({p},{s})"),
            word(4, (*p).into(), (*s).into()),
            0,
            u32::from(*p),
        ),
        DataType::Real => h("f32".into(), word(12, 4, 0), 0, 4),
        DataType::Double => h("f64".into(), word(12, 8, 0), 0, 8),
        DataType::Char(n) => h(format!("char({n})"), word(1, (*n).into(), 0), 0, *n),
        DataType::Varchar(n) | DataType::Clob(n)
            if *n <= 65_535 && matches!(t.data_type, DataType::Varchar(_)) =>
        {
            h(format!("varchar({n})"), word(2, (*n).into(), 0), 0, n + 2)
        }
        DataType::Varchar(n) | DataType::Clob(n) if *n <= MAX_CHAR => {
            h(format!("varchar({n}:4)"), word(2, (*n).into(), 4), 0, n + 4)
        }
        DataType::Varchar(_) | DataType::Clob(_) => {
            return Err(unsupported("a character string longer than 16773104"));
        }
        DataType::Date => h("date(*ISO)".into(), word(9, 10, 0), 1, 10),
        DataType::Time => h("time(*ISO)".into(), word(10, 8, 0), 1, 8),
        DataType::Timestamp(p) => {
            let n = if *p == 0 { 19 } else { 20 + u32::from(*p) };
            h(format!("timestamp({p})"), word(11, n.into(), 0), 0, n)
        }
        // Double-byte and binary strings as the runtime's LOB host
        // variables: a 4-byte length, then UTF-16 or the bytes.
        DataType::Graphic(n) | DataType::Vargraphic(n) | DataType::Dbclob(n)
            if *n <= MAX_CHAR / 2 =>
        {
            let size = 4 + 2 * n;
            h(
                format!("bytes({size})"),
                word(1, size.into(), 0),
                SQLTYPE | (3 << 4) | 1,
                size,
            )
        }
        DataType::Binary(n) | DataType::Varbinary(n) | DataType::Blob(n) if *n <= MAX_CHAR => {
            let size = 4 + n;
            h(
                format!("bytes({size})"),
                word(1, size.into(), 0),
                SQLTYPE | (2 << 4) | 1,
                size,
            )
        }
        DataType::Graphic(_)
        | DataType::Vargraphic(_)
        | DataType::Dbclob(_)
        | DataType::Binary(_)
        | DataType::Varbinary(_)
        | DataType::Blob(_) => {
            return Err(unsupported("a string longer than 16773104 bytes"));
        }
        // DECFLOAT and BOOLEAN as their text; the statements read them
        // through a CAST to the type (`marker_type`).
        DataType::DecFloat(_) => h("varchar(45)".into(), word(2, 45, 0), 0, 47),
        DataType::Boolean => h("char(1)".into(), word(1, 1, 0), 0, 1),
    })
}

/// The type as SQL writes it, the form the catalogs and the `PARM`
/// entries keep.
#[must_use]
pub fn type_text(t: &TypeSpec) -> String {
    let base = match &t.data_type {
        DataType::Char(n) => format!("CHARACTER({n})"),
        DataType::Varchar(n) => format!("VARCHAR({n})"),
        DataType::Graphic(n) => format!("GRAPHIC({n})"),
        DataType::Vargraphic(n) => format!("VARGRAPHIC({n})"),
        DataType::Decimal(p, s) => format!("DECIMAL({p}, {s})"),
        DataType::Numeric(p, s) => format!("NUMERIC({p}, {s})"),
        DataType::SmallInt => "SMALLINT".into(),
        DataType::Integer => "INTEGER".into(),
        DataType::BigInt => "BIGINT".into(),
        DataType::Real => "REAL".into(),
        DataType::Double => "DOUBLE".into(),
        DataType::Date => "DATE".into(),
        DataType::Time => "TIME".into(),
        DataType::Timestamp(p) => format!("TIMESTAMP({p})"),
        DataType::DecFloat(p) => format!("DECFLOAT({p})"),
        DataType::Binary(n) => format!("BINARY({n})"),
        DataType::Varbinary(n) => format!("VARBINARY({n})"),
        DataType::Boolean => "BOOLEAN".into(),
        DataType::Blob(n) => format!("BLOB({n})"),
        DataType::Clob(n) => format!("CLOB({n})"),
        DataType::Dbclob(n) => format!("DBCLOB({n})"),
    };
    let mut s = base;
    if let Some(c) = t.ccsid {
        let _ = write!(s, " CCSID {c}");
    }
    if t.bit_data {
        s.push_str(" FOR BIT DATA");
    }
    s
}

/// The catalog's name, length and scale of a type (`SYSPARMS`
/// `DATA_TYPE`, `CHARACTER_MAXIMUM_LENGTH`/`NUMERIC_PRECISION`,
/// `NUMERIC_SCALE`): name, length or precision, scale, whether numeric.
#[must_use]
pub fn catalog_type(t: &DataType) -> (&'static str, i64, Option<i64>, bool) {
    match t {
        DataType::Char(n) => ("CHARACTER", (*n).into(), None, false),
        DataType::Varchar(n) => ("VARCHAR", (*n).into(), None, false),
        DataType::Graphic(n) => ("GRAPHIC", (*n).into(), None, false),
        DataType::Vargraphic(n) => ("VARGRAPHIC", (*n).into(), None, false),
        DataType::Decimal(p, s) => ("DECIMAL", (*p).into(), Some((*s).into()), true),
        DataType::Numeric(p, s) => ("NUMERIC", (*p).into(), Some((*s).into()), true),
        DataType::SmallInt => ("SMALLINT", 5, Some(0), true),
        DataType::Integer => ("INTEGER", 10, Some(0), true),
        DataType::BigInt => ("BIGINT", 19, Some(0), true),
        DataType::Real => ("REAL", 24, None, true),
        DataType::Double => ("DOUBLE", 53, None, true),
        DataType::Date => ("DATE", 10, None, false),
        DataType::Time => ("TIME", 8, None, false),
        DataType::Timestamp(p) => (
            "TIMESTAMP",
            if *p == 0 { 19 } else { 20 + i64::from(*p) },
            None,
            false,
        ),
        DataType::DecFloat(p) => ("DECFLOAT", (*p).into(), None, true),
        DataType::Binary(n) => ("BINARY", (*n).into(), None, false),
        DataType::Varbinary(n) => ("VARBINARY", (*n).into(), None, false),
        DataType::Boolean => ("BOOLEAN", 1, None, false),
        DataType::Blob(n) => ("BLOB", (*n).into(), None, false),
        DataType::Clob(n) => ("CLOB", (*n).into(), None, false),
        DataType::Dbclob(n) => ("DBCLOB", (*n).into(), None, false),
    }
}

/// Parse a type kept as text.
///
/// # Errors
///
/// The text is not a type.
pub fn parse_type(text: &str) -> Result<TypeSpec> {
    let mut p = crate::parse::Parser::new(text, crate::ast::Naming::Sql)?;
    p.type_spec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn types_round_trip_as_text() {
        for t in [
            "INTEGER",
            "DECIMAL(9, 2)",
            "CHARACTER(10)",
            "VARCHAR(100)",
            "TIMESTAMP(6)",
            "NUMERIC(5, 0)",
        ] {
            assert_eq!(type_text(&parse_type(t).unwrap()), t);
        }
        let h = host_type(&parse_type("DECIMAL(9, 2)").unwrap()).unwrap();
        assert_eq!((h.qir.as_str(), h.size), ("packed(9,2)", 5));
        assert_eq!(
            host_type(&parse_type("BLOB(1K)").unwrap()).unwrap().size,
            1028
        );
        assert_eq!(
            host_type(&parse_type("GRAPHIC(3)").unwrap()).unwrap().qir,
            "bytes(10)"
        );
        assert_eq!(
            marker_type(&parse_type("DECFLOAT(16)").unwrap()).as_deref(),
            Some("DECFLOAT(16)")
        );
    }
}
