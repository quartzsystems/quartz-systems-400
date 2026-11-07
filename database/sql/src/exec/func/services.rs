// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `QSYS2` scalar services: `BASE64_ENCODE`, `BASE64_DECODE`,
//! `URL_ENCODE`, `URL_DECODE` as pub400 answers them
//! (tools/conformance/sql/expected/services/SVCVALS.out), the HTTP
//! functions through the host, and the `name => value` argument marker
//! a named argument parses to.

use std::fmt::Write as _;

use super::super::SqlContext;
use super::super::restype::{Kind, SqlType};
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The name without a `QSYS2.` qualifier, when it is one of ours.
fn ours(name: &str) -> Option<&str> {
    let bare = name.strip_prefix("QSYS2.").unwrap_or(name);
    matches!(bare, "BASE64_ENCODE" | "BASE64_DECODE" | "URL_ENCODE" | "URL_DECODE").then_some(bare)
}

/// SQL0443 with SQLSTATE 38501, as pub400 fails these functions.
fn failed(what: impl Into<String>) -> SqlError {
    SqlError::Coded { code: -443, state: "38501", what: what.into() }
}

/// The family's functions.
pub(crate) fn call(name: &str, args: &[Value], ctx: &SqlContext) -> Option<Result<Value>> {
    if name.starts_with("=>") {
        // A named argument where a plain function call reads it.
        return Some(Ok(args.first().cloned().unwrap_or(Value::Null)));
    }
    if let Some(r) = super::super::catalog::services::http::scalar(name, args, ctx) {
        return Some(r);
    }
    if let Some(r) = super::super::catalog::services::procs::qcmdexc_scalar(name, args, ctx) {
        return Some(r);
    }
    if matches!(name, "GENERATE_SPREADSHEET" | "SYSTOOLS.GENERATE_SPREADSHEET") {
        // IBM's runs the Access Client Solutions Java program.
        return Some(Err(SqlError::Coded {
            code: -84,
            state: "42612",
            what: "SYSTOOLS.GENERATE_SPREADSHEET: spreadsheets are not supported; IFS_WRITE writes a CSV file".into(),
        }));
    }
    let bare = ours(name)?;
    Some(run(bare, args))
}

fn run(name: &str, args: &[Value]) -> Result<Value> {
    let max = if name == "URL_ENCODE" { 2 } else { 1 };
    if args.is_empty() || args.len() > max {
        return Err(SqlError::Coded { code: -440, state: "42884", what: format!("QSYS2.{name} with {} arguments", args.len()) });
    }
    if args[0].is_null() {
        return Ok(Value::Null);
    }
    match name {
        "BASE64_ENCODE" => {
            let bytes = match &args[0] {
                Value::Bin(b) => b.clone(),
                v => v.as_bytes()?,
            };
            Ok(Value::Str(base64_encode(&bytes)))
        }
        "BASE64_DECODE" => base64_decode(&args[0].as_text()).map(Value::Bin).ok_or_else(|| failed("the text is not base64")),
        "URL_ENCODE" => {
            if args.get(1).is_some_and(|m| !m.is_null()) {
                // BLOCKED(rel): the encoding modes pub400 accepts are not
                // yet settled; every mode it was given was SQL0171.
                return Err(SqlError::Coded { code: -171, state: "42815", what: "argument 2 of URL_ENCODE".into() });
            }
            Ok(Value::Str(url_encode(&args[0].as_text())))
        }
        _ => url_decode(&args[0].as_text()).map(Value::Str).ok_or_else(|| failed("the text is not URL encoded")),
    }
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 with padding (RFC 4648).
#[must_use]
pub fn base64_encode(b: &[u8]) -> String {
    let mut out = String::with_capacity(b.len().div_ceil(3) * 4);
    for chunk in b.chunks(3) {
        let n = (u32::from(chunk[0]) << 16) | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8) | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Base64 decoded; line breaks and blanks between groups are allowed,
/// anything else outside the alphabet is not.
#[must_use]
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let t: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace()).collect();
    if !t.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(t.len() / 4 * 3);
    for (k, q) in t.chunks(4).enumerate() {
        let last = k + 1 == t.len() / 4;
        let mut n = 0u32;
        let mut pad = 0;
        for (i, &c) in q.iter().enumerate() {
            let v = if c == b'=' {
                if !last || i < 2 {
                    return None;
                }
                pad += 1;
                0
            } else {
                if pad > 0 {
                    return None;
                }
                u32::try_from(ALPHABET.iter().position(|a| *a == c)?).ok()?
            };
            n = (n << 6) | v;
        }
        let bytes = n.to_be_bytes();
        out.extend_from_slice(&bytes[1..4 - pad]);
    }
    Some(out)
}

/// Percent-encoded UTF-8: letters, digits, `.`, `-` and `_` kept, a
/// blank as `+`, every other byte `%XX`.
#[must_use]
pub fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => out.push(char::from(b)),
            b' ' => out.push('+'),
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

/// `+` as a blank, `%XX` as its byte, the bytes as UTF-8; `None` for a
/// broken escape or bytes that are not UTF-8.
#[must_use]
pub fn url_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' => {
                let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8(out).ok()
}

/// The result types, as pub400 describes them.
pub(crate) fn result_type(name: &str, given: &[Option<SqlType>], _exprs: &[crate::ast::Expr]) -> Option<SqlType> {
    if name.starts_with("=>") {
        return given.first().cloned().flatten();
    }
    let bare = name.strip_prefix("QSYS2.").unwrap_or(name);
    if let Some(t) = super::super::catalog::services::http::result_type(bare) {
        return Some(t);
    }
    if bare == "QCMDEXC" || name.ends_with("GENERATE_SPREADSHEET") {
        return Some(SqlType::new(Kind::Integer, 4, 0, true));
    }
    if ours(name)? == "BASE64_DECODE" {
        return Some(SqlType::new(Kind::Blob, 1_610_612_736, 0, true));
    }
    let mut t = SqlType::new(Kind::Clob, 2_147_483_647, 0, true);
    t.spec.ccsid = Some(1208);
    Some(t)
}
