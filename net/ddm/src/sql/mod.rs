// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL values, column types and descriptions that cross the wire,
//! independent of any one database engine.

use qdecimal::Decimal;

/// A column's type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// `SMALLINT`.
    SmallInt,
    /// `INTEGER`.
    Integer,
    /// `BIGINT`.
    BigInt,
    /// `DECIMAL` (packed).
    Decimal,
    /// `NUMERIC` (zoned).
    Numeric,
    /// `DOUBLE`.
    Double,
    /// `CHAR`.
    Char,
    /// `VARCHAR`.
    VarChar,
    /// `DATE` (ISO characters).
    Date,
    /// `TIME` (ISO characters).
    Time,
    /// `TIMESTAMP` (ISO characters).
    Timestamp,
}

/// A column's type with its attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColType {
    /// The type.
    pub kind: Kind,
    /// Characters for strings; bytes for the rest.
    pub length: u32,
    /// Digits for decimals.
    pub precision: u8,
    /// Fraction digits.
    pub scale: u8,
    /// Whether it may be null.
    pub nullable: bool,
    /// The CCSID of character data (0 for numbers).
    pub ccsid: u16,
}

impl ColType {
    /// A type with its natural length.
    pub fn new(kind: Kind, length: u32, precision: u8, scale: u8, nullable: bool) -> Self {
        let length = match kind {
            Kind::SmallInt => 2,
            Kind::Integer => 4,
            Kind::BigInt | Kind::Double | Kind::Time => 8,
            Kind::Decimal => u32::from(precision) / 2 + 1,
            Kind::Numeric => u32::from(precision),
            Kind::Date => 10,
            Kind::Timestamp => 26,
            Kind::Char | Kind::VarChar => length,
        };
        let ccsid = match kind {
            Kind::Char | Kind::VarChar | Kind::Date | Kind::Time | Kind::Timestamp => 37,
            _ => 0,
        };
        Self {
            kind,
            length,
            precision,
            scale,
            nullable,
            ccsid,
        }
    }

    /// The SQL text of the type (`DECIMAL(7,2)`).
    pub fn sql_name(&self) -> String {
        match self.kind {
            Kind::SmallInt => "SMALLINT".into(),
            Kind::Integer => "INTEGER".into(),
            Kind::BigInt => "BIGINT".into(),
            Kind::Decimal => format!("DECIMAL({},{})", self.precision, self.scale),
            Kind::Numeric => format!("NUMERIC({},{})", self.precision, self.scale),
            Kind::Double => "DOUBLE".into(),
            Kind::Char => format!("CHAR({})", self.length),
            Kind::VarChar => format!("VARCHAR({})", self.length),
            Kind::Date => "DATE".into(),
            Kind::Time => "TIME".into(),
            Kind::Timestamp => "TIMESTAMP".into(),
        }
    }

    /// Read SQL type text (`CHAR(10)`, `DECIMAL(7, 2)`, `INTEGER`).
    pub fn parse(text: &str, nullable: bool) -> Option<Self> {
        let t = text.trim().to_ascii_uppercase();
        let (name, args) = match t.find('(') {
            Some(i) => (t[..i].trim().to_string(), t[i + 1..].trim_end_matches(')').to_string()),
            None => (t.clone(), String::new()),
        };
        let nums: Vec<u32> = args
            .split(',')
            .filter_map(|a| a.trim().parse().ok())
            .collect();
        let a = |i: usize, d: u32| nums.get(i).copied().unwrap_or(d);
        let small = |v: u32| u8::try_from(v).unwrap_or(u8::MAX);
        Some(match name.as_str() {
            "SMALLINT" => Self::new(Kind::SmallInt, 0, 5, 0, nullable),
            "INTEGER" | "INT" => Self::new(Kind::Integer, 0, 10, 0, nullable),
            "BIGINT" => Self::new(Kind::BigInt, 0, 19, 0, nullable),
            "DECIMAL" | "DEC" => Self::new(Kind::Decimal, 0, small(a(0, 5)), small(a(1, 0)), nullable),
            "NUMERIC" => Self::new(Kind::Numeric, 0, small(a(0, 5)), small(a(1, 0)), nullable),
            "DOUBLE" | "FLOAT" => Self::new(Kind::Double, 0, 53, 0, nullable),
            "CHAR" | "CHARACTER" => Self::new(Kind::Char, a(0, 1), 0, 0, nullable),
            "VARCHAR" => Self::new(Kind::VarChar, a(0, 1), 0, 0, nullable),
            "DATE" => Self::new(Kind::Date, 0, 0, 0, nullable),
            "TIME" => Self::new(Kind::Time, 0, 0, 0, nullable),
            "TIMESTAMP" => Self::new(Kind::Timestamp, 0, 0, 0, nullable),
            _ => return None,
        })
    }
}

/// A described column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Column {
    /// Its name.
    pub name: String,
    /// Its type.
    pub ty: ColType,
    /// Its label, when it has one.
    pub label: Option<String>,
}

/// A value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// Null.
    Null,
    /// Any integer type.
    Int(i64),
    /// A decimal or numeric.
    Dec(Decimal),
    /// A double.
    Double(f64),
    /// Characters (and dates, times and timestamps in ISO form).
    Text(String),
}

impl Value {
    /// The value as display text (`-` for null).
    pub fn render(&self) -> String {
        match self {
            Self::Null => "-".into(),
            Self::Int(i) => i.to_string(),
            Self::Dec(d) => d.to_string(),
            Self::Double(f) => f.to_string(),
            Self::Text(t) => t.clone(),
        }
    }
}

/// The SQL communication area that crosses the wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sqlca {
    /// `SQLCODE`.
    pub sqlcode: i32,
    /// `SQLSTATE`.
    pub sqlstate: String,
    /// `SQLERRPROC`: product and module.
    pub sqlerrproc: String,
    /// `SQLERRD1` to `SQLERRD6`.
    pub sqlerrd: [i32; 6],
    /// `SQLWARN0` to `SQLWARNA`.
    pub sqlwarn: [u8; 11],
    /// `SQLRDBNAME`.
    pub rdbname: String,
    /// `SQLERRMC`: the message tokens.
    pub sqlerrmc: String,
}

impl Default for Sqlca {
    fn default() -> Self {
        Self {
            sqlcode: 0,
            sqlstate: "00000".into(),
            sqlerrproc: "QSQ400".into(),
            sqlerrd: [0; 6],
            sqlwarn: [b' '; 11],
            rdbname: String::new(),
            sqlerrmc: String::new(),
        }
    }
}

impl Sqlca {
    /// An error.
    pub fn error(sqlcode: i32, sqlstate: &str, tokens: &str) -> Self {
        Self {
            sqlcode,
            sqlstate: sqlstate.into(),
            sqlerrmc: tokens.into(),
            ..Self::default()
        }
    }

    /// `+100`.
    pub fn no_data() -> Self {
        Self {
            sqlcode: 100,
            sqlstate: "02000".into(),
            ..Self::default()
        }
    }

    /// Success with a row count in `SQLERRD3`.
    pub fn rows(n: u64) -> Self {
        let mut c = Self::default();
        c.sqlerrd[2] = i32::try_from(n).unwrap_or(i32::MAX);
        c
    }

    /// Whether it reports an error.
    pub fn is_error(&self) -> bool {
        self.sqlcode < 0
    }
}
