// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The DRDA data types of FD:OCA descriptors (odd codes are the
//! nullable forms) and the SQLDA's `SQLTYPE` codes (odd codes are
//! nullable), with the mapping from the column types.

use crate::sql::{ColType, Kind};

/// DRDA data types (not null; add one for the nullable form).
pub mod drda {
    consts! {
        /// Four-byte integer.
        INTEGER = 0x02,
        /// Two-byte integer.
        SMALL = 0x04,
        /// Eight-byte IEEE float.
        FLOAT8 = 0x0A,
        /// Four-byte IEEE float.
        FLOAT4 = 0x0C,
        /// Packed decimal.
        DECIMAL = 0x0E,
        /// Zoned decimal.
        ZDECIMAL = 0x10,
        /// Eight-byte integer.
        INTEGER8 = 0x16,
        /// Date (characters).
        DATE = 0x20,
        /// Time (characters).
        TIME = 0x22,
        /// Timestamp (characters).
        TIMESTAMP = 0x24,
        /// Fixed bytes.
        FIXBYTE = 0x26,
        /// Varying bytes.
        VARBYTE = 0x28,
        /// Fixed single-byte characters.
        CHAR = 0x30,
        /// Varying single-byte characters.
        VARCHAR = 0x32,
        /// Long varying single-byte characters.
        LONG = 0x34,
        /// Fixed mixed characters.
        MIX = 0x3C,
        /// Varying mixed characters.
        VARMIX = 0x3E,
        /// Long varying mixed characters.
        LONGMIX = 0x40,
    }
}

/// `SQLTYPE` codes (not null; add one for nullable).
pub mod sqltype {
    consts! {
        /// `DATE`.
        DATE = 384,
        /// `TIME`.
        TIME = 388,
        /// `TIMESTAMP`.
        TIMESTAMP = 392,
        /// `VARCHAR`.
        VARCHAR = 448,
        /// `CHAR`.
        CHAR = 452,
        /// `LONG VARCHAR`.
        LONGVARCHAR = 456,
        /// `FLOAT`/`DOUBLE`.
        FLOAT = 480,
        /// `DECIMAL`.
        DECIMAL = 484,
        /// `NUMERIC`.
        NUMERIC = 488,
        /// `BIGINT`.
        BIGINT = 492,
        /// `INTEGER`.
        INTEGER = 496,
        /// `SMALLINT`.
        SMALLINT = 500,
    }
}

/// The DRDA type and descriptor length of a column type.
pub fn drda_of(t: &ColType) -> (u8, u16) {
    let (code, len) = match t.kind {
        Kind::SmallInt => (drda::SMALL, 2),
        Kind::Integer => (drda::INTEGER, 4),
        Kind::BigInt => (drda::INTEGER8, 8),
        Kind::Double => (drda::FLOAT8, 8),
        Kind::Decimal => (drda::DECIMAL, (u16::from(t.precision) << 8) | u16::from(t.scale)),
        Kind::Numeric => (drda::ZDECIMAL, (u16::from(t.precision) << 8) | u16::from(t.scale)),
        Kind::Char => (drda::CHAR, u16::try_from(t.length).unwrap_or(u16::MAX)),
        Kind::VarChar => (drda::VARCHAR, u16::try_from(t.length).unwrap_or(u16::MAX)),
        Kind::Date => (drda::DATE, 10),
        Kind::Time => (drda::TIME, 8),
        Kind::Timestamp => (drda::TIMESTAMP, 26),
    };
    (u8::try_from(code).unwrap_or(0) | u8::from(t.nullable), len)
}

/// The column type of a DRDA type and descriptor length.
pub fn col_of(code: u8, len: u16) -> Option<ColType> {
    let nullable = code & 1 == 1;
    let base = u16::from(code & !1);
    let hi = u8::try_from(len >> 8).unwrap_or(0);
    let lo = u8::try_from(len & 0xFF).unwrap_or(0);
    let kind = match base {
        drda::SMALL => Kind::SmallInt,
        drda::INTEGER => Kind::Integer,
        drda::INTEGER8 => Kind::BigInt,
        drda::FLOAT8 => Kind::Double,
        drda::DECIMAL => Kind::Decimal,
        drda::ZDECIMAL => Kind::Numeric,
        drda::CHAR | drda::MIX | drda::FIXBYTE => Kind::Char,
        drda::VARCHAR | drda::VARMIX | drda::LONG | drda::LONGMIX | drda::VARBYTE => Kind::VarChar,
        drda::DATE => Kind::Date,
        drda::TIME => Kind::Time,
        drda::TIMESTAMP => Kind::Timestamp,
        _ => return None,
    };
    let (p, s) = if matches!(kind, Kind::Decimal | Kind::Numeric) {
        (hi, lo)
    } else {
        (0, 0)
    };
    let mut t = ColType::new(kind, u32::from(len), p, s, nullable);
    if matches!(base, drda::FIXBYTE | drda::VARBYTE) {
        t.ccsid = 65535;
    }
    Some(t)
}

/// The `SQLTYPE` of a column type.
pub fn sqltype_of(t: &ColType) -> i16 {
    let c = match t.kind {
        Kind::SmallInt => sqltype::SMALLINT,
        Kind::Integer => sqltype::INTEGER,
        Kind::BigInt => sqltype::BIGINT,
        Kind::Double => sqltype::FLOAT,
        Kind::Decimal => sqltype::DECIMAL,
        Kind::Numeric => sqltype::NUMERIC,
        Kind::Char => sqltype::CHAR,
        Kind::VarChar => sqltype::VARCHAR,
        Kind::Date => sqltype::DATE,
        Kind::Time => sqltype::TIME,
        Kind::Timestamp => sqltype::TIMESTAMP,
    };
    i16::try_from(c + u16::from(t.nullable)).unwrap_or(0)
}

/// The column type of an `SQLTYPE` with the SQLDA's precision, scale,
/// length and CCSID.
pub fn col_of_sqltype(code: i16, precision: i16, scale: i16, length: i64, ccsid: u16) -> Option<ColType> {
    let nullable = code & 1 == 1;
    let kind = match u16::try_from(code & !1).ok()? {
        sqltype::SMALLINT => Kind::SmallInt,
        sqltype::INTEGER => Kind::Integer,
        sqltype::BIGINT => Kind::BigInt,
        sqltype::FLOAT => Kind::Double,
        sqltype::DECIMAL => Kind::Decimal,
        sqltype::NUMERIC => Kind::Numeric,
        sqltype::CHAR => Kind::Char,
        sqltype::VARCHAR | sqltype::LONGVARCHAR => Kind::VarChar,
        sqltype::DATE => Kind::Date,
        sqltype::TIME => Kind::Time,
        sqltype::TIMESTAMP => Kind::Timestamp,
        _ => return None,
    };
    let p = u8::try_from(precision).unwrap_or(0);
    let s = u8::try_from(scale).unwrap_or(0);
    let mut t = ColType::new(kind, u32::try_from(length).unwrap_or(0), p, s, nullable);
    if matches!(kind, Kind::Char | Kind::VarChar) {
        t.ccsid = ccsid;
    }
    Some(t)
}
