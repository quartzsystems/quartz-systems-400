// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The codes the `ROUTINE` entry keeps and the words the catalogs show.

use crate::ast::{DataType, Language, ParmMode, ParmStyle, SqlAccess, TypeSpec};

/// The stored language code.
#[must_use]
pub fn language_code(l: Language) -> u8 {
    match l {
        Language::Sql => 1,
        Language::Rpgle => 2,
        Language::Rpg => 3,
        Language::Cl => 4,
        Language::C => 5,
        Language::Cobol => 6,
        Language::Cobolle => 7,
        Language::Java => 8,
        Language::Pli => 9,
    }
}

/// The catalog's `EXTERNAL_LANGUAGE`.
#[must_use]
pub fn language_word(code: u8) -> &'static str {
    match code {
        2 => "RPGLE",
        3 => "RPG",
        4 => "CL",
        5 => "C",
        6 => "COBOL",
        7 => "COBOLLE",
        8 => "JAVA",
        9 => "PLI",
        _ => "SQL",
    }
}

/// The stored parameter style code.
#[must_use]
pub fn style_code(s: ParmStyle) -> u8 {
    match s {
        ParmStyle::Sql => 1,
        ParmStyle::General => 2,
        ParmStyle::GeneralNulls => 3,
        ParmStyle::Db2General => 4,
        ParmStyle::Java => 5,
        ParmStyle::Db2Sql => 6,
    }
}

/// The catalog's `PARAMETER_STYLE` (7 characters at most, as IBM's).
#[must_use]
pub fn style_word(code: u8) -> &'static str {
    match code {
        1 => "SQL",
        2 => "GENERAL",
        3 => "GENERAL WITH NULLS",
        4 => "DB2GNRL",
        5 => "JAVA",
        6 => "DB2SQL",
        _ => "",
    }
}

/// The stored access code.
#[must_use]
pub fn access_code(a: SqlAccess) -> u8 {
    match a {
        SqlAccess::NoSql => 1,
        SqlAccess::Contains => 2,
        SqlAccess::Reads => 3,
        SqlAccess::Modifies => 4,
    }
}

/// The catalog's `SQL_DATA_ACCESS`.
#[must_use]
pub fn access_word(code: u8) -> &'static str {
    match code {
        1 => "NONE",
        2 => "CONTAINS",
        3 => "READS",
        _ => "MODIFIES",
    }
}

/// The stored mode.
#[must_use]
pub fn mode_code(m: ParmMode) -> u8 {
    match m {
        ParmMode::In => 1,
        ParmMode::Out => 2,
        ParmMode::InOut => 3,
    }
}

/// The mode of a stored code.
#[must_use]
pub fn mode_of(code: u8) -> ParmMode {
    match code {
        2 => ParmMode::Out,
        3 => ParmMode::InOut,
        _ => ParmMode::In,
    }
}

/// The catalog's `DATA_TYPE` of a parameter (`SYSPARMS`).
#[must_use]
pub fn parm_type_word(t: &DataType) -> &'static str {
    match t {
        DataType::Char(_) => "CHARACTER",
        DataType::Varchar(_) => "CHARACTER VARYING",
        DataType::Graphic(_) => "GRAPHIC",
        DataType::Vargraphic(_) => "GRAPHIC VARYING",
        DataType::Decimal(..) => "DECIMAL",
        DataType::Numeric(..) => "NUMERIC",
        DataType::SmallInt => "SMALLINT",
        DataType::Integer => "INTEGER",
        DataType::BigInt => "BIGINT",
        DataType::Real => "REAL",
        DataType::Double => "DOUBLE PRECISION",
        DataType::Date => "DATE",
        DataType::Time => "TIME",
        DataType::Timestamp(_) => "TIMESTAMP",
        DataType::DecFloat(_) => "DECFLOAT",
        DataType::Binary(_) => "BINARY",
        DataType::Varbinary(_) => "BINARY VARYING",
        DataType::Boolean => "BOOLEAN",
        DataType::Blob(_) => "BINARY LARGE OBJECT",
        DataType::Clob(_) => "CHARACTER LARGE OBJECT",
        DataType::Dbclob(_) => "DOUBLE-BYTE CHARACTER LARGE OBJECT",
    }
}

/// The SQLTYPE of a type, without the null bit: `PARM_SIGNATURE` is
/// these, two bytes each.
#[must_use]
pub fn sqltype(t: &DataType) -> u16 {
    match t {
        DataType::Char(_) => 452,
        DataType::Varchar(_) => 448,
        DataType::Graphic(_) => 468,
        DataType::Vargraphic(_) => 464,
        DataType::Decimal(..) => 484,
        DataType::Numeric(..) => 488,
        DataType::SmallInt => 500,
        DataType::Integer => 496,
        DataType::BigInt => 492,
        DataType::Real | DataType::Double => 480,
        DataType::Date => 384,
        DataType::Time => 388,
        DataType::Timestamp(_) => 392,
        DataType::DecFloat(_) => 996,
        DataType::Binary(_) => 912,
        DataType::Varbinary(_) => 908,
        DataType::Boolean => 2436,
        DataType::Blob(_) => 404,
        DataType::Clob(_) => 408,
        DataType::Dbclob(_) => 412,
    }
}

/// The type of a described column (SQLTYPE and SQLLEN).
#[must_use]
pub fn type_of_sqltype(base: i16, len: i32) -> Option<TypeSpec> {
    let n = u32::try_from(len).unwrap_or(0);
    let dt = match base {
        452 => DataType::Char(n.max(1)),
        448 | 456 => DataType::Varchar(n.max(1)),
        484 => DataType::Decimal(u8::try_from(len >> 8).ok()?, u8::try_from(len & 0xFF).ok()?),
        488 => DataType::Numeric(u8::try_from(len >> 8).ok()?, u8::try_from(len & 0xFF).ok()?),
        500 => DataType::SmallInt,
        496 => DataType::Integer,
        492 => DataType::BigInt,
        480 if len == 4 => DataType::Real,
        480 => DataType::Double,
        384 => DataType::Date,
        388 => DataType::Time,
        392 => DataType::Timestamp(6),
        408 => DataType::Clob(n),
        _ => return None,
    };
    Some(TypeSpec { data_type: dt, ccsid: None, bit_data: false, udt: None })
}
