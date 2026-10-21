// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPFFD`: every field of every record format.

use std::fmt::Write as _;

use dds::{DataType, DefaultValue, FieldDef};
use qsname::Name;
use store::obj::ObjectId;
use store::txn::Txn;

use super::{kv, library_name};
use crate::error::Result;
use crate::file;
use crate::format::FormatDef;

/// One field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldInfo {
    /// Name.
    pub name: Name,
    /// Type letter.
    pub data_type: char,
    /// Digits or characters.
    pub length: u32,
    /// Decimal positions.
    pub decimals: u8,
    /// Bytes in the buffer.
    pub byte_len: u32,
    /// 1-based buffer position.
    pub position: u32,
    /// `ALWNULL`.
    pub allow_null: bool,
    /// `VARLEN`, with the allocated length.
    pub varlen: Option<Option<u32>>,
    /// Key position (1-based) and `DESCEND`, when a key field.
    pub key: Option<(usize, bool)>,
    /// `TEXT`.
    pub text: Option<String>,
    /// `COLHDG` lines.
    pub colhdg: Vec<String>,
    /// `ALIAS`.
    pub alias: Option<String>,
    /// `DFT` as DDS spells it.
    pub default: Option<String>,
    /// `DATFMT`/`TIMFMT` with separator.
    pub format: Option<String>,
    /// `EDTCDE` or `EDTWRD`.
    pub edit: Option<String>,
}

/// One format's fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormatFields {
    /// Name.
    pub name: Name,
    /// Level identifier.
    pub level_id: String,
    /// Record length.
    pub record_len: u32,
    /// Text.
    pub text: Option<String>,
    /// Fields in buffer order.
    pub fields: Vec<FieldInfo>,
}

/// The file's fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldDescription {
    /// File.
    pub file: Name,
    /// Library.
    pub library: String,
    /// Formats.
    pub formats: Vec<FormatFields>,
}

fn field_info(fmt: &FormatDef, i: usize, f: &FieldDef) -> FieldInfo {
    let key = fmt
        .format
        .keys
        .iter()
        .position(|k| k.field == i)
        .map(|p| (p + 1, fmt.format.keys[p].descend));
    let default = f.default.as_ref().map(|d| match d {
        DefaultValue::Null => "*NULL".to_string(),
        DefaultValue::Quoted(q) => format!("'{}'", q.replace('\'', "''")),
        DefaultValue::Hex(h) => {
            let mut s = String::from("X'");
            for b in h {
                let _ = write!(s, "{b:02X}");
            }
            s.push('\'');
            s
        }
        DefaultValue::Number(n) => n.clone(),
    });
    let format = match f.data_type {
        DataType::Date => Some(format!(
            "{} {}",
            f.date_fmt.unwrap_or_default().name(),
            f.date_sep
                .filter(|_| f.date_fmt.unwrap_or_default().fixed_separator().is_none())
                .map_or(String::new(), |c| format!("DATSEP('{c}')"))
        )),
        DataType::Time => Some(format!(
            "{} {}",
            f.time_fmt.unwrap_or_default().name(),
            f.time_sep
                .filter(|_| f.time_fmt.unwrap_or_default().fixed_separator().is_none())
                .map_or(String::new(), |c| format!("TIMSEP('{c}')"))
        )),
        _ => None,
    }
    .map(|s| s.trim_end().to_string());
    let edit = f
        .edit_code
        .as_ref()
        .map(|e| {
            format!(
                "EDTCDE({}{})",
                e.code,
                e.modifier
                    .as_deref()
                    .map_or(String::new(), |m| format!(" {m}"))
            )
        })
        .or_else(|| f.edit_word.as_ref().map(|w| format!("EDTWRD('{w}')")));
    FieldInfo {
        name: f.name,
        data_type: f.data_type.letter(),
        length: f.length,
        decimals: f.decimals,
        byte_len: f.byte_len,
        position: f.offset + 1,
        allow_null: f.allow_null,
        varlen: f.varlen.map(|v| v.allocated),
        key,
        text: f.text.clone(),
        colhdg: f.colhdg.clone(),
        alias: f.alias.clone(),
        default,
        format,
        edit,
    }
}

/// Describe the fields.
pub fn dspffd(txn: &mut Txn<'_>, file: ObjectId) -> Result<FieldDescription> {
    let desc = file::describe(txn, file)?;
    let formats = desc
        .formats
        .iter()
        .map(|fmt| FormatFields {
            name: fmt.format.name,
            level_id: fmt.level_id.clone(),
            record_len: fmt.format.record_len,
            text: fmt.format.text.clone(),
            fields: fmt
                .format
                .fields
                .iter()
                .enumerate()
                .map(|(i, f)| field_info(fmt, i, f))
                .collect(),
        })
        .collect();
    Ok(FieldDescription {
        file: desc.name(),
        library: library_name(txn, desc.header.library),
        formats,
    })
}

impl FieldDescription {
    /// Render: a header per format, one line per field, then the
    /// field keywords indented.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("Field description\n");
        kv(&mut out, "File", self.file);
        kv(&mut out, "Library", &self.library);
        for fmt in &self.formats {
            kv(&mut out, "Record format", fmt.name);
            kv(&mut out, "  Level identifier", &fmt.level_id);
            kv(&mut out, "  Record length", fmt.record_len);
            if let Some(t) = &fmt.text {
                kv(&mut out, "  Text", t);
            }
            let _ = writeln!(
                out,
                "  {:<10} {:>6} {:>6} {:<4} {:>3} {:>5} {:<4} Key",
                "Field", "Type", "Length", "Dec", "Len", "Pos", "Null"
            );
            for f in &fmt.fields {
                let key = f.key.map_or(String::new(), |(n, d)| {
                    format!("{n}{}", if d { " DESCEND" } else { "" })
                });
                let _ = writeln!(
                    out,
                    "  {:<10} {:>6} {:>6} {:>4} {:>3} {:>5} {:<4} {}",
                    f.name,
                    f.data_type,
                    f.length,
                    if f.data_type == 'A' || f.data_type == 'H' || f.data_type == 'G' {
                        String::new()
                    } else {
                        f.decimals.to_string()
                    },
                    f.byte_len,
                    f.position,
                    if f.allow_null { "Y" } else { "" },
                    key
                );
                if let Some(v) = f.varlen {
                    let _ = writeln!(
                        out,
                        "             VARLEN{}",
                        v.map_or(String::new(), |a| format!("({a})"))
                    );
                }
                for (label, value) in [
                    ("TEXT", f.text.as_ref().map(|t| format!("'{t}'"))),
                    ("ALIAS", f.alias.clone()),
                    ("DFT", f.default.clone()),
                    ("Format", f.format.clone()),
                    ("Edit", f.edit.clone()),
                ] {
                    if let Some(v) = value {
                        let _ = writeln!(out, "             {label} {v}");
                    }
                }
                for (i, h) in f.colhdg.iter().enumerate() {
                    let _ = writeln!(out, "             COLHDG {} '{h}'", i + 1);
                }
            }
        }
        out
    }
}
