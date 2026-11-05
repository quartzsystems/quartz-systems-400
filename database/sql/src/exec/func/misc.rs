// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Binary and miscellaneous scalar functions: `HASH_MD5`, `HASH_SHA1`,
//! `HASH_SHA256`, `HASH_SHA512` and `HASH` over the string's bytes in
//! its CCSID, `GENERATE_UNIQUE`, `HEXTORAW`, `VARCHAR_BIT_FORMAT`,
//! `VARCHAR_FORMAT_BINARY`; the encryption functions refused; and the
//! names DB2 for i does not have, refused as it refuses them.

#![allow(clippy::many_single_char_names)]

use std::cell::Cell;
use std::fmt::Write as _;

use sha1::Digest as _;

use super::super::SqlContext;
use super::super::rtype::{Kind, SqlType};
use super::arity;
use crate::ast::Function;
use crate::error::{Result, SqlError};
use crate::value::Value;

/// Names DB2 for i 7.5 has no function for (pub400 answers `SQL0204`).
const NOT_FUNCTIONS: &[&str] = &[
    "ASINH", "ACOSH", "DATE_TRUNC", "DATE_PART", "DAYS_BETWEEN", "HOURS_BETWEEN", "MINUTES_BETWEEN", "SECONDS_BETWEEN",
    "YEARS_BETWEEN", "INITCAP", "LEN", "TO_MULTI_BYTE", "RAWTOHEX", "EBCDIC_CHR",
];

thread_local! {
    /// The values `GENERATE_UNIQUE` gave in the same microsecond.
    static UNIQUE: Cell<(u64, u16)> = const { Cell::new((0, 0)) };
}

fn bytes(v: &Value) -> Result<Vec<u8>> {
    v.as_bytes()
}

/// MD5 (RFC 1321).
fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4,
        11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64)
        .map(|i| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let v = (f64::from(i + 1).sin().abs() * 4_294_967_296.0) as u32;
            v
        })
        .collect();
    let mut msg = data.to_vec();
    let bit_len = u64::try_from(data.len()).expect("fits").wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());
    let (mut a0, mut b0, mut c0, mut d0) = (0x6745_2301u32, 0xefcd_ab89u32, 0x98ba_dcfeu32, 0x1032_5476u32);
    for chunk in msg.chunks(64) {
        let m: Vec<u32> = chunk.chunks(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect();
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut out = [0u8; 16];
    for (i, w) in [a0, b0, c0, d0].iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    out
}

fn digest(algorithm: i64, data: &[u8]) -> Result<Vec<u8>> {
    Ok(match algorithm {
        0 => md5(data).to_vec(),
        1 => sha1::Sha1::digest(data).to_vec(),
        2 => sha2::Sha256::digest(data).to_vec(),
        3 => sha2::Sha512::digest(data).to_vec(),
        n => return Err(SqlError::Overflow { what: format!("hash algorithm {n} is not 0 to 3") }),
    })
}

fn from_hex(s: &str) -> Result<Vec<u8>> {
    crate::value::unhex(s).ok_or_else(|| SqlError::Conversion { what: format!("'{s}' is not hexadecimal digits") })
}

/// Calls `name` when this family has it.
pub(crate) fn call(name: &str, args: &[Value], ctx: &SqlContext) -> Option<Result<Value>> {
    if NOT_FUNCTIONS.contains(&name) {
        return Some(Err(SqlError::ObjectNotFound { name: name.into(), kind: "function" }));
    }
    let null = args.iter().any(Value::is_null);
    Some(match name {
        "ENCRYPT" | "ENCRYPT_AES" | "ENCRYPT_AES256" | "ENCRYPT_RC2" | "ENCRYPT_TDES" | "DECRYPT_BIT" | "DECRYPT_BINARY"
        | "DECRYPT_CHAR" | "DECRYPT_DB" | "GETHINT" => Err(SqlError::Unsupported {
            what: format!("{name}: DB2 for i's encrypted value layout is its own and not published, so no value made here could be read there"),
        }),
        "VERIFY_GROUP_FOR_USER" => Err(SqlError::Unsupported { what: "VERIFY_GROUP_FOR_USER".into() }),
        "HASH_MD5" | "HASH_SHA1" | "HASH_SHA256" | "HASH_SHA512" | "HASH" => (|| {
            let (lo, hi) = if name == "HASH" { (1, 2) } else { (1, 1) };
            arity(name, args, lo, hi)?;
            if null {
                return Ok(Value::Null);
            }
            let alg = match name {
                "HASH_MD5" => 0,
                "HASH_SHA1" => 1,
                "HASH_SHA256" => 2,
                "HASH_SHA512" => 3,
                _ => args.get(1).map_or(Ok(0), Value::as_i64)?,
            };
            Ok(Value::Bin(digest(alg, &bytes(&args[0])?)?))
        })(),
        "GENERATE_UNIQUE" => arity(name, args, 0, 0).map(|()| {
            // A 13-byte value that only grows: the microseconds since 1970
            // and a count within the microsecond, then the job.
            let (secs, picos) = ctx.clock_picos();
            let micros = secs * 1_000_000 + picos / 1_000_000;
            let n = UNIQUE.with(|u| {
                let (last, count) = u.get();
                let next = if micros <= last { (last, count.wrapping_add(1)) } else { (micros, 0) };
                u.set(next);
                next
            });
            let mut b = Vec::with_capacity(13);
            b.extend_from_slice(&n.0.to_be_bytes());
            b.extend_from_slice(&n.1.to_be_bytes());
            let job = ctx.job.0.to_be_bytes();
            b.extend_from_slice(&job[job.len() - 3..]);
            Value::Bin(b)
        }),
        "HEXTORAW" | "VARCHAR_BIT_FORMAT" => (|| {
            arity(name, args, 1, 2)?;
            if null {
                return Ok(Value::Null);
            }
            Ok(Value::Bin(from_hex(&args[0].as_text())?))
        })(),
        "VARCHAR_FORMAT_BINARY" => (|| {
            arity(name, args, 2, 2)?;
            if null {
                return Ok(Value::Null);
            }
            let b = bytes(&args[0])?;
            let mut hex = String::new();
            for x in &b {
                let _ = write!(hex, "{x:02x}");
            }
            let mut digits = hex.chars();
            let mut out = String::new();
            for c in args[1].as_text().chars() {
                match c {
                    'x' => out.push(digits.next().ok_or_else(|| SqlError::Conversion { what: "the format has more digits than the value".into() })?),
                    'X' => out.push(digits.next().ok_or_else(|| SqlError::Conversion { what: "the format has more digits than the value".into() })?.to_ascii_uppercase()),
                    c => out.push(c),
                }
            }
            if digits.next().is_some() {
                return Err(SqlError::Conversion { what: "the value has more digits than the format".into() });
            }
            Ok(Value::Str(out))
        })(),
        _ => return None,
    })
}

/// The result type of `name` when this family has it.
pub(crate) fn result_type(name: &str, args: &[SqlType], f: &Function) -> Option<SqlType> {
    let null = args.iter().any(|a| a.null);
    let first = args.first().copied().unwrap_or_else(SqlType::unknown);
    Some(match name {
        "HASH_MD5" => SqlType::new(Kind::Binary, 16, 0, null),
        "HASH_SHA1" => SqlType::new(Kind::Binary, 20, 0, null),
        "HASH_SHA256" => SqlType::new(Kind::Binary, 32, 0, null),
        "HASH_SHA512" => SqlType::new(Kind::Binary, 64, 0, null),
        "HASH" => {
            let n = match f.args.get(1) {
                Some(crate::ast::Expr::Literal(crate::ast::Literal::Num(n))) => match n.as_str() {
                    "1" => 20,
                    "2" => 32,
                    "3" => 64,
                    _ => 16,
                },
                None => 16,
                _ => 64,
            };
            SqlType::new(Kind::Varbinary, n, 0, null)
        }
        "GENERATE_UNIQUE" => SqlType::new(Kind::Binary, 13, 0, false),
        "HEXTORAW" | "VARCHAR_BIT_FORMAT" => SqlType::new(Kind::Varbinary, (first.len / 2).max(1), 0, null),
        "VARCHAR_FORMAT_BINARY" => SqlType::varchar(40, null),
        _ => return None,
    })
}

/// `INTERPRET(e AS type)`: the bytes of `e` read as a value of the type.
pub(crate) fn interpret(v: &Value, t: &crate::ast::TypeSpec) -> Result<Value> {
    if v.is_null() {
        return Ok(Value::Null);
    }
    let b = v.as_bytes()?;
    let f = SqlType::of_data_type(&t.data_type, false).to_field();
    if b.len() != usize::try_from(f.byte_len).expect("fits") {
        return Err(SqlError::Conversion {
            what: format!("INTERPRET of {} bytes as a type of {} bytes", b.len(), f.byte_len),
        });
    }
    let dbv = db::record::read_field(&f, &b, &[], None).map_err(|e| SqlError::Conversion { what: e.to_string() })?;
    Value::from_field(&f, &dbv)
}
