// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Index-only access: a query that needs no column of a table but the
//! key fields of the path it reads takes their values from the path's
//! keys and never reads the records. Only key parts that give back the
//! field's value exactly are read so: fixed-length `CHAR` (no `ZONE` or
//! `DIGIT`) and zoned, packed or binary numbers kept `SIGNED`, null
//! capable or not, ascending or descending.

use db::format::FormatDef;
use db::format::keys::part_len;
use dds::{DataType, FieldDef, KeyTransform};
use qdecimal::Decimal;

/// Whether every key field of `fmt` named in `fields` (system names)
/// can be read back from the key, and every name is a key field.
#[must_use]
pub fn covers(fmt: &FormatDef, fields: &[String]) -> bool {
    !fields.is_empty()
        && fields.iter().all(|n| {
            fmt.format.keys.iter().any(|k| {
                let f = &fmt.format.fields[k.field];
                f.name.to_string() == *n && readable(f, k.transform)
            })
        })
}

fn readable(f: &FieldDef, t: KeyTransform) -> bool {
    match f.data_type {
        DataType::Char => {
            f.varlen.is_none() && matches!(t, KeyTransform::Signed | KeyTransform::Unsigned)
        }
        DataType::Zoned | DataType::Packed | DataType::Binary => {
            t == KeyTransform::Signed && f.digits() <= 38
        }
        _ => false,
    }
}

/// The values of the key fields of `fmt` in `key` (a constraint tree's
/// number left off), by field index; `None` where a part cannot be
/// read back.
#[must_use]
pub fn decode(fmt: &FormatDef, key: &[u8]) -> Option<Vec<(usize, db::record::Value)>> {
    let mut out = Vec::with_capacity(fmt.format.keys.len());
    let mut at = 0;
    for k in &fmt.format.keys {
        let f = &fmt.format.fields[k.field];
        let len = part_len(f, k.transform);
        let mut part = key.get(at..at + len)?.to_vec();
        at += len;
        if k.descend {
            for b in &mut part {
                *b = !*b;
            }
        }
        if !readable(f, k.transform) {
            continue;
        }
        let data = if f.allow_null {
            if part[0] == 0xFF {
                out.push((k.field, db::record::Value::Null));
                continue;
            }
            &part[1..]
        } else {
            &part[..]
        };
        let v = if f.data_type == DataType::Char {
            db::record::Value::Char(data.to_vec())
        } else {
            let neg = data[0] == 0x01;
            let mut m: i128 = 0;
            for &b in &data[1..] {
                let d = if neg {
                    0x39u8.checked_sub(b)?
                } else {
                    b.checked_sub(0x30)?
                };
                if d > 9 {
                    return None;
                }
                m = m.checked_mul(10)?.checked_add(i128::from(d))?;
            }
            db::record::Value::Num(Decimal::new(if neg { -m } else { m }, f.decimals).ok()?)
        };
        out.push((k.field, v));
    }
    Some(out)
}
