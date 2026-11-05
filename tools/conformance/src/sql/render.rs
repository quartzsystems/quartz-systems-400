// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A statement's outcome as text, the way `SqlKit.java` writes what `JTOpen`
//! returns: status lines, column lines, value cells.

use std::fmt::Write as _;
use sql::embed::describe::Col;
use sql::{ColumnDesc, ResultSet, Value};

/// The status line.
#[must_use]
pub fn status(code: i32, state: &str, msgid: &str, count: Option<u64>) -> String {
    let mut s = format!("sqlcode {code} sqlstate {state} msgid {msgid}");
    if let Some(n) = count {
        let _ = write!(s, " count {n}");
    }
    s
}

/// A column type as `JTOpen` names it: no blanks, `TIMESTAMP` for precision
/// 6, `NUMERIC` and `DECIMAL` with precision and scale.
#[must_use]
pub fn column_type(data_type: &str) -> String {
    let t: String = data_type.chars().filter(|c| !c.is_whitespace()).collect();
    if t == "TIMESTAMP(6)" { "TIMESTAMP".into() } else { t }
}

/// The scale of a `DECIMAL`/`NUMERIC` type, when it is one.
fn scale(data_type: &str) -> Option<usize> {
    let t = column_type(data_type);
    let args = t.strip_prefix("DECIMAL(").or_else(|| t.strip_prefix("NUMERIC("))?;
    args.trim_end_matches(')').split(',').nth(1)?.parse().ok()
}

/// A column type as `JTOpen` names it from what the server described:
/// graphic types in CCSID 1200 are `NCHAR`, `NVARCHAR` and `NCLOB`;
/// character types in CCSID 65535 `CHAR () FOR BIT DATA` and the like.
#[must_use]
pub fn jtopen_type(data_type: &str, described: Option<&Col>) -> String {
    let t = column_type(data_type);
    let Some(d) = described else { return t };
    let head = t.split('(').next().unwrap_or("");
    let n = |name: &str| format!("{name}{}", &t[head.len()..]);
    match (head, d.base, d.ccsid) {
        ("GRAPHIC", 468, 1200) => n("NCHAR"),
        ("VARGRAPHIC", 464, 1200) => n("NVARCHAR"),
        ("DBCLOB", 412, 1200) => n("NCLOB"),
        ("CHAR", 452, 65535) => "CHAR () FOR BIT DATA".into(),
        ("VARCHAR", 448, 65535) => "VARCHAR () FOR BIT DATA".into(),
        _ => t,
    }
}

/// A column line.
#[must_use]
pub fn column(c: &ColumnDesc, described: Option<&Col>) -> String {
    format!("# {} {}{}", c.name, jtopen_type(&c.data_type, described), if c.nullable { "" } else { " NOT NULL" })
}

/// The server's name written `{server}` where it is a value, on both
/// sides (`CURRENT SERVER`, the catalog name of the `SYSIBM` views).
#[must_use]
pub fn mask_server(text: &str, server: &str) -> String {
    text.replace(&format!("'{server}'"), "'{server}'")
}

/// Java's `Double.toString`/`Float.toString`: the shortest digits that
/// round to the value, plain between 10^-3 and 10^7, else `d.dddE<n>`.
#[must_use]
pub fn java_float(x: f64, single: bool) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    // Rust's `{:e}` is shortest round-trip digits too.
    #[allow(clippy::cast_possible_truncation)]
    let e = if single { format!("{:e}", x as f32) } else { format!("{x:e}") };
    let (mant, exp) = e.split_once('e').expect("exponent");
    let exp: i32 = exp.parse().expect("number");
    let neg = mant.starts_with('-');
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let sign = if neg { "-" } else { "" };
    let abs = x.abs();
    if (1e-3..1e7).contains(&abs) {
        let point = exp + 1;
        let s = if point <= 0 {
            format!("0.{}{digits}", "0".repeat(point.unsigned_abs() as usize))
        } else {
            let p = point.unsigned_abs() as usize;
            if digits.len() > p {
                format!("{}.{}", &digits[..p], &digits[p..])
            } else {
                format!("{}{}.0", digits, "0".repeat(p - digits.len()))
            }
        };
        format!("{sign}{s}")
    } else {
        let rest = if digits.len() > 1 { &digits[1..] } else { "0" };
        format!("{sign}{}.{rest}E{exp}", &digits[..1])
    }
}

/// One cell.
#[must_use]
pub fn value(v: &Value, data_type: &str) -> String {
    match v {
        Value::Null => "NULL".into(),
        Value::Num(d) => match scale(data_type) {
            Some(s) => u8::try_from(s)
                .ok()
                .and_then(|s| d.rescale(s, qdecimal::Rounding::Truncate).ok())
                .map_or_else(|| d.to_string(), |r| r.to_string()),
            None => d.to_string(),
        },
        Value::Double(x) => java_float(*x, false),
        Value::Real(x) => java_float(f64::from(*x), true),
        // JTOpen gives a DECFLOAT as Types.OTHER, which the oracle side quotes.
        Value::DecFloat(d) => format!("'{d}'"),
        Value::Str(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Bin(b) => {
            let mut h = String::from("X'");
            for x in b {
                let _ = write!(h, "{x:02X}");
            }
            h.push('\'');
            h
        }
        // JTOpen's getString of a BOOLEAN.
        Value::Bool(b) => if *b { "1" } else { "0" }.into(),
        // No result set holds an array (SQ20441 first).
        Value::Date(_) | Value::Time(_) | Value::Array(_) => format!("'{v}'"),
        // JDBC's form, then IBM's separators, as SqlKit.java does.
        Value::Timestamp(_) => v.to_string().replacen(' ', "-", 1).replace(':', "."),
    }
}

/// A result set's column and row lines; `described` is the PREPARE's
/// description of its columns, when it has as many.
#[must_use]
pub fn rows(rs: &ResultSet, described: &[Col]) -> String {
    let mut out = String::new();
    let described = if described.len() == rs.columns.len() { described } else { &[] };
    for (i, c) in rs.columns.iter().enumerate() {
        out.push_str(&column(c, described.get(i)));
        out.push('\n');
    }
    for r in &rs.rows {
        out.push('|');
        for (i, (v, c)) in r.iter().zip(&rs.columns).enumerate() {
            out.push(' ');
            let bits = described.get(i).is_some_and(|d| d.ccsid == 65535 && matches!(d.base, 448 | 452));
            match v {
                // Character data FOR BIT DATA: JTOpen reads its bytes.
                Value::Str(s) if bits => out.push_str(&value(&Value::Bin(ccsid::to_ebcdic(s).unwrap_or_default()), "")),
                _ => out.push_str(&value(v, &c.data_type)),
            }
            out.push_str(" |");
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_print_as_java_does() {
        assert_eq!(java_float(1.5e10, false), "1.5E10");
        assert_eq!(java_float(0.001, false), "0.001");
        assert_eq!(java_float(12.0, false), "12.0");
        assert_eq!(java_float(1_234_567.0, false), "1234567.0");
        assert_eq!(java_float(1e7, false), "1.0E7");
        assert_eq!(java_float(-2.5e-5, false), "-2.5E-5");
        assert_eq!(java_float(0.1, true), "0.1");
        assert_eq!(column_type("DECIMAL(9, 2)"), "DECIMAL(9,2)");
        assert_eq!(column_type("TIMESTAMP(6)"), "TIMESTAMP");
    }
}
