// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Column types on the wire and the values in them: the SQL type
//! (even, one more when null-capable), the bytes a value takes in a
//! row, scale and precision as the data format carries them, the CCSID.

use qdecimal::{Decimal, Float, Integer, Packed, Rounding, Zoned};
use sql::value::{Timestamp, Value};

use crate::text;

/// SQL types.
#[allow(missing_docs)]
pub mod sqltype {
    pub const DATE: u16 = 384;
    pub const TIME: u16 = 388;
    pub const TIMESTAMP: u16 = 392;
    pub const VARCHAR: u16 = 448;
    pub const CHAR: u16 = 452;
    pub const LONGVARCHAR: u16 = 456;
    pub const VARGRAPHIC: u16 = 464;
    pub const GRAPHIC: u16 = 468;
    pub const LONGVARGRAPHIC: u16 = 472;
    pub const FLOAT: u16 = 480;
    pub const DECIMAL: u16 = 484;
    pub const NUMERIC: u16 = 488;
    pub const BIGINT: u16 = 492;
    pub const INTEGER: u16 = 496;
    pub const SMALLINT: u16 = 500;
    pub const VARBINARY: u16 = 908;
    pub const BINARY: u16 = 912;
}

/// A column or parameter marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColType {
    /// The even SQL type.
    pub base: u16,
    /// Null-capable.
    pub null: bool,
    /// Bytes in a row.
    pub len: u32,
    /// The format's first number: scale, or characters for text.
    pub scale: u16,
    /// The format's second number: precision of numbers.
    pub precision: u16,
    /// CCSID (0 for numbers).
    pub ccsid: u16,
    /// The name.
    pub name: String,
    /// Whether it comes from a table's column (the format's join byte).
    pub column: bool,
}

impl ColType {
    /// From a `DESCRIBE` entry: SQLTYPE, SQLLEN (precision * 256 +
    /// scale for decimals) and the name.
    pub fn from_describe(sqltype: i64, sqllen: i64, name: &str) -> Self {
        let t = u16::try_from(sqltype.max(0)).unwrap_or(sqltype::VARCHAR);
        let base = t & !1;
        let null = t & 1 == 1;
        let n = u32::try_from(sqllen.max(0)).unwrap_or(0);
        let (len, scale, precision, ccsid) = match base {
            sqltype::VARCHAR | sqltype::LONGVARCHAR => (n + 2, n, 0, 37),
            sqltype::CHAR => (n, n, 0, 37),
            sqltype::VARGRAPHIC => (2 * n + 2, n, 0, 13488),
            sqltype::GRAPHIC => (2 * n, n, 0, 13488),
            sqltype::DECIMAL => (n / 256 / 2 + 1, n % 256, n / 256, 0),
            sqltype::NUMERIC => (n / 256, n % 256, n / 256, 0),
            sqltype::BIGINT => (8, 0, 19, 0),
            sqltype::INTEGER => (4, 0, 9, 0),
            sqltype::SMALLINT => (2, 0, 4, 0),
            sqltype::FLOAT => (n, 0, if n == 4 { 24 } else { 53 }, 0),
            sqltype::DATE => (n.max(8), n.max(8), 0, 37),
            sqltype::TIME => (8, 8, 0, 37),
            sqltype::TIMESTAMP => (26, 26, 0, 37),
            _ => (n, n, 0, 65535),
        };
        Self {
            base,
            null,
            len,
            scale: u16::try_from(scale).unwrap_or(u16::MAX),
            precision: u16::try_from(precision).unwrap_or(u16::MAX),
            ccsid: u16::try_from(ccsid).unwrap_or(37),
            name: name.trim_end().to_string(),
            column: false,
        }
    }

    /// A VARCHAR of `n` bytes.
    pub fn varchar(name: &str, n: u32, null: bool) -> Self {
        Self::from_describe(i64::from(sqltype::VARCHAR) + i64::from(null), i64::from(n), name)
    }

    /// An INTEGER.
    pub fn integer(name: &str, null: bool) -> Self {
        Self::from_describe(i64::from(sqltype::INTEGER) + i64::from(null), 4, name)
    }

    /// A SMALLINT.
    pub fn smallint(name: &str, null: bool) -> Self {
        Self::from_describe(i64::from(sqltype::SMALLINT) + i64::from(null), 2, name)
    }

    /// The SQL type with its null bit.
    pub fn sqltype(&self) -> u16 {
        self.base + u16::from(self.null)
    }

    /// Whether the type holds UTF-16 text.
    fn graphic(&self) -> bool {
        matches!(self.ccsid, 1200 | 13488 | 61952)
    }

    /// The precision and scale of a decimal type.
    fn digits(&self) -> (u8, u8) {
        let p = match self.base {
            sqltype::DECIMAL | sqltype::NUMERIC => self.precision,
            _ => 31,
        };
        (u8::try_from(p.min(63)).unwrap_or(31), u8::try_from(self.scale.min(63)).unwrap_or(0))
    }
}

/// A date in the job's format (`*MDY`, `/`), as an 8-byte field holds it.
fn date_text(d: db::record::datetime::Date, width: u32) -> String {
    if width >= 10 {
        format!("{:04}-{:02}-{:02}", d.year, d.month, d.day)
    } else {
        format!("{:02}/{:02}/{:02}", d.month, d.day, d.year % 100)
    }
}

fn text_of(v: &Value, c: &ColType) -> String {
    match v {
        Value::Date(d) => date_text(*d, c.len),
        Value::Time(t) => format!("{:02}.{:02}.{:02}", t.hour, t.minute, t.second),
        Value::Timestamp(t) => t.render_field(),
        other => other.as_text(),
    }
}

/// The value a null field's bytes hold: empty text or zero.
pub fn empty(c: &ColType) -> Value {
    match c.base {
        sqltype::DECIMAL | sqltype::NUMERIC | sqltype::BIGINT | sqltype::INTEGER | sqltype::SMALLINT | sqltype::FLOAT => {
            Value::int(0)
        }
        sqltype::DATE | sqltype::TIME | sqltype::TIMESTAMP => Value::Null,
        _ => Value::Str(String::new()),
    }
}

/// `v` as the bytes of a field of type `c` (exactly `c.len` bytes).
pub fn encode(v: &Value, c: &ColType) -> Vec<u8> {
    let n = c.len as usize;
    let mut out = match c.base {
        sqltype::VARCHAR | sqltype::LONGVARCHAR | sqltype::VARGRAPHIC => {
            let mut b = if c.graphic() {
                text::utf16(&text_of(v, c))
            } else {
                text::ebcdic(&text_of(v, c))
            };
            b.truncate(n.saturating_sub(2));
            let mut out = crate::reply::len16(if c.graphic() { b.len() / 2 } else { b.len() })
                .to_be_bytes()
                .to_vec();
            out.extend(b);
            let pad = if c.graphic() { [0x00, 0x20] } else { [0x40, 0x40] };
            while out.len() < n {
                out.push(pad[out.len() % 2]);
            }
            out
        }
        sqltype::CHAR | sqltype::DATE | sqltype::TIME | sqltype::TIMESTAMP | sqltype::GRAPHIC => {
            if c.graphic() {
                let mut b = text::utf16(&text_of(v, c));
                while b.len() < n {
                    b.extend([0x00, 0x20]);
                }
                b
            } else {
                text::ebcdic_fixed(&text_of(v, c), n)
            }
        }
        sqltype::DECIMAL | sqltype::NUMERIC | sqltype::BIGINT | sqltype::INTEGER | sqltype::SMALLINT | sqltype::FLOAT => {
            let d = v.as_decimal().unwrap_or_else(|_| Decimal::zero());
            number(&d, c)
        }
        _ => text::ebcdic_fixed(&text_of(v, c), n),
    };
    out.resize(n, 0);
    out
}

#[allow(clippy::many_single_char_names)]
fn number(d: &Decimal, c: &ColType) -> Vec<u8> {
    let (p, s) = c.digits();
    let r = match c.base {
        // Positive packed values carry the preferred sign F.
        sqltype::DECIMAL => Packed::new(p, s).and_then(|k| k.encode(d, Rounding::Truncate)).map(|mut b| {
            if let Some(last) = b.last_mut()
                && *last & 0x0F == 0x0C
            {
                *last |= 0x0F;
            }
            b
        }),
        sqltype::NUMERIC => Zoned::new(p, s).and_then(|k| k.encode(d, Rounding::Truncate)),
        sqltype::BIGINT => Integer::new(8, true).and_then(|k| k.encode(d)),
        sqltype::INTEGER => Integer::new(4, true).and_then(|k| k.encode(d)),
        sqltype::SMALLINT => Integer::new(2, true).and_then(|k| k.encode(d)),
        _ => if c.len == 4 { Float::SINGLE } else { Float::DOUBLE }.encode(d),
    };
    r.unwrap_or_default()
}

/// The value of a field of type `c` in `b`.
#[allow(clippy::many_single_char_names)]
pub fn decode(b: &[u8], c: &ColType) -> Value {
    match c.base {
        sqltype::VARCHAR | sqltype::LONGVARCHAR | sqltype::VARGRAPHIC => {
            let n = usize::from(crate::reply::be16(b, 0));
            if c.graphic() {
                Value::Str(text::from_utf16(crate::reply::slice(b, 2, 2 * n)))
            } else {
                Value::Str(text::decode(u32::from(c.ccsid), crate::reply::slice(b, 2, n)))
            }
        }
        sqltype::CHAR | sqltype::GRAPHIC => {
            if c.graphic() {
                Value::Str(text::from_utf16(b))
            } else {
                Value::Str(text::decode(u32::from(c.ccsid), b))
            }
        }
        sqltype::DATE => {
            let t = text::decode(u32::from(c.ccsid), b);
            sql::value::parse_date(t.trim())
                .or_else(|| mdy(t.trim()))
                .map_or(Value::Null, Value::Date)
        }
        sqltype::TIME => {
            let t = text::decode(u32::from(c.ccsid), b).replace('.', ":");
            sql::value::parse_time(t.trim()).map_or(Value::Null, Value::Time)
        }
        sqltype::TIMESTAMP => {
            let t = text::decode(u32::from(c.ccsid), b);
            let t = t.trim();
            // `yyyy-mm-dd-hh.mm.ss.ffffff` or with a blank and colons.
            let fixed: String = t
                .char_indices()
                .map(|(i, ch)| match (i, ch) {
                    (10, '-') => ' ',
                    (13 | 16, '.') => ':',
                    _ => ch,
                })
                .collect();
            Timestamp::parse(&fixed).map_or(Value::Null, Value::Timestamp)
        }
        sqltype::DECIMAL | sqltype::NUMERIC | sqltype::BIGINT | sqltype::INTEGER | sqltype::SMALLINT | sqltype::FLOAT => {
            let (p, s) = c.digits();
            let r = match c.base {
                sqltype::DECIMAL => Packed::new(p, s).and_then(|k| k.decode(b)),
                sqltype::NUMERIC => Zoned::new(p, s).and_then(|k| k.decode(b)),
                sqltype::BIGINT => Integer::new(8, true).and_then(|k| k.decode(b)),
                sqltype::INTEGER => Integer::new(4, true).and_then(|k| k.decode(b)),
                sqltype::SMALLINT => Integer::new(2, true).and_then(|k| k.decode(b)),
                _ => if b.len() == 4 { Float::SINGLE } else { Float::DOUBLE }.decode(b),
            };
            r.map_or(Value::Null, Value::Num)
        }
        _ => Value::Str(text::decode(u32::from(c.ccsid), b)),
    }
}

#[allow(clippy::many_single_char_names)]
fn mdy(t: &str) -> Option<db::record::datetime::Date> {
    let p: Vec<&str> = t.split(['/', '-', '.']).collect();
    if p.len() != 3 {
        return None;
    }
    let (m, d, y) = (p[0].parse::<u8>().ok()?, p[1].parse::<u8>().ok()?, p[2].parse::<u16>().ok()?);
    let year = if y < 40 { 2000 + y } else if y < 100 { 1900 + y } else { y };
    Some(db::record::datetime::Date {
        year,
        month: m,
        day: d,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn round_trips() {
        let c = ColType::varchar("N", 20, true);
        assert_eq!(c.len, 22);
        let b = encode(&Value::Str("ROW 1".into()), &c);
        assert_eq!(b.len(), 22);
        assert_eq!(decode(&b, &c), Value::Str("ROW 1".into()));
        let i = ColType::integer("I", false);
        assert_eq!(encode(&Value::int(9), &i), vec![0, 0, 0, 9]);
        let d = ColType::from_describe(485, 9 * 256 + 2, "AMT");
        assert_eq!((d.len, d.scale, d.precision), (5, 2, 9));
        let v = Value::Num(Decimal::parse("1000.01").unwrap());
        assert_eq!(decode(&encode(&v, &d), &d), v);
    }
}
