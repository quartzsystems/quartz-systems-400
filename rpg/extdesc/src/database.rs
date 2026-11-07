// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Database files: every record format with its fields (alias, null
//! capability, default), its keys and its level identifier.

use std::fmt::Write as _;

use dds::DataType;
use rpgc::files::{Dev, ExtField, ExtFile, ExtFormat};
use rpgc::types::{DatFmt, TimFmt, Ty};

/// A database file as the compiler sees it.
#[must_use]
pub fn database_desc(desc: &db::file::FileDesc) -> ExtFile {
    ExtFile {
        lib: desc.library_name.to_string(),
        dev: Dev::Disk,
        formats: desc.formats.iter().map(format_of).collect(),
        indara: false,
    }
}

/// One record format as the compiler sees it.
#[must_use]
pub fn format_of(f: &db::FormatDef) -> ExtFormat {
    let r = &f.format;
    ExtFormat {
        name: r.name.to_string(),
        level_id: f.level_id.clone(),
        len: r.record_len,
        fields: r
            .fields
            .iter()
            .map(|d| ExtField {
                name: d.name.to_string(),
                ty: field_type(d),
                offset: d.offset,
                usage: match d.usage {
                    Some(dds::Usage::Input) => 'I',
                    Some(dds::Usage::Output) => 'O',
                    Some(dds::Usage::Neither) => 'N',
                    _ => 'B',
                },
                alias: d.alias.clone().unwrap_or_default(),
                nullable: d.allow_null,
                default: d.default.as_ref().and_then(dft_literal),
            })
            .collect(),
        keys: r.keys.iter().map(|k| k.name.to_string()).collect(),
        dsp: None,
    }
}

/// A field's `DFT` value as an RPG literal.
fn dft_literal(v: &dds::DefaultValue) -> Option<String> {
    match v {
        dds::DefaultValue::Quoted(q) => Some(format!("'{}'", q.replace('\'', "''"))),
        dds::DefaultValue::Number(n) => Some(n.clone()),
        dds::DefaultValue::Hex(h) => Some(
            h.iter().fold("X'".to_string(), |mut s, b| {
                let _ = write!(s, "{b:02X}");
                s
            }) + "'",
        ),
        dds::DefaultValue::Null | dds::DefaultValue::Expr(_) => None,
    }
}

/// A database field's RPG type.
#[must_use]
pub fn field_type(d: &dds::FieldDef) -> Ty {
    let digits = u8::try_from(d.length).unwrap_or(63);
    match d.data_type {
        DataType::Char | DataType::Hex | DataType::DbcsOpen => {
            if d.varlen.is_some() {
                Ty::VarChar(d.length, 2)
            } else {
                Ty::Char(d.length)
            }
        }
        DataType::Graphic => Ty::Graph(d.length),
        // SQL-only types: their record bytes.
        DataType::DecFloat
        | DataType::Bytes
        | DataType::Boolean
        | DataType::Blob
        | DataType::Clob
        | DataType::Dbclob => {
            if d.varlen.is_some() {
                Ty::VarChar(d.length, 2)
            } else {
                Ty::Char(d.data_len())
            }
        }
        DataType::Zoned => Ty::Zoned(digits, d.decimals),
        DataType::Packed => Ty::Packed(digits, d.decimals),
        DataType::Binary => Ty::Bin(digits, d.decimals),
        DataType::Float => Ty::Float(if d.byte_len == 4 { 4 } else { 8 }),
        DataType::Date => {
            let f = match d.date_fmt.unwrap_or_default() {
                dds::DateFmt::Iso => DatFmt::Iso,
                dds::DateFmt::Usa => DatFmt::Usa,
                dds::DateFmt::Eur => DatFmt::Eur,
                dds::DateFmt::Jis => DatFmt::Jis,
                dds::DateFmt::Mdy => DatFmt::Mdy,
                dds::DateFmt::Dmy => DatFmt::Dmy,
                dds::DateFmt::Ymd => DatFmt::Ymd,
                dds::DateFmt::Jul => DatFmt::Jul,
            };
            Ty::Date(f, d.date_sep.unwrap_or(f.sep()))
        }
        DataType::Time => {
            let f = match d.time_fmt.unwrap_or_default() {
                dds::TimeFmt::Hms => TimFmt::Hms,
                dds::TimeFmt::Iso => TimFmt::Iso,
                dds::TimeFmt::Usa => TimFmt::Usa,
                dds::TimeFmt::Eur => TimFmt::Eur,
                dds::TimeFmt::Jis => TimFmt::Jis,
            };
            Ty::Time(f, d.time_sep.unwrap_or(f.sep()))
        }
        DataType::Timestamp => Ty::Timestamp(6),
    }
}
