// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Arithmetic tokens.

use super::ArithError;

/// A token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Tok {
    Num(i64),
    Ident(String),
    Op(&'static str),
}

const OPS: &[&str] = &[
    "<<=", ">>=", "&&", "||", "<<", ">>", "<=", ">=", "==", "!=", "++", "--", "+=", "-=", "*=",
    "/=", "%=", "&=", "^=", "|=", "+", "-", "*", "/", "%", "<", ">", "&", "^", "|", "!", "~", "=",
    "?", ":", ",", "(", ")",
];

/// Split an expression into tokens.
pub(super) fn tokens(s: &str) -> Result<Vec<Tok>, ArithError> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    'outer: while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '#' | '_' | '@'))
            {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            out.push(Tok::Num(number(&text)?));
            continue;
        }
        if c == '_' || c.is_ascii_alphabetic() {
            let start = i;
            while i < chars.len() && (chars[i] == '_' || chars[i].is_ascii_alphanumeric()) {
                i += 1;
            }
            out.push(Tok::Ident(chars[start..i].iter().collect()));
            continue;
        }
        for op in OPS {
            let n = op.chars().count();
            if i + n <= chars.len() && chars[i..i + n].iter().copied().eq(op.chars()) {
                out.push(Tok::Op(op));
                i += n;
                continue 'outer;
            }
        }
        return Err(ArithError::Syntax(s.trim().to_string()));
    }
    Ok(out)
}

/// A constant: decimal, `0` octal, `0x` hex or `base#digits`.
pub(super) fn number(text: &str) -> Result<i64, ArithError> {
    let bad = || ArithError::BadNumber(text.to_string());
    let (base, digits) = if let Some((b, d)) = text.split_once('#') {
        let b: u32 = b.parse().map_err(|_| bad())?;
        if !(2..=64).contains(&b) || d.is_empty() {
            return Err(bad());
        }
        (b, d)
    } else if let Some(h) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        if h.is_empty() {
            return Err(bad());
        }
        (16, h)
    } else if text.len() > 1 && text.starts_with('0') {
        (8, &text[1..])
    } else {
        (10, text)
    };
    let mut v: i64 = 0;
    for c in digits.chars() {
        let d = digit(c, base).ok_or_else(bad)?;
        v = v.wrapping_mul(i64::from(base)).wrapping_add(i64::from(d));
    }
    Ok(v)
}

fn digit(c: char, base: u32) -> Option<u32> {
    let d = match c {
        '0'..='9' => c as u32 - '0' as u32,
        'a'..='z' => c as u32 - 'a' as u32 + 10,
        'A'..='Z' if base <= 36 => c as u32 - 'A' as u32 + 10,
        'A'..='Z' => c as u32 - 'A' as u32 + 36,
        '@' => 62,
        '_' => 63,
        _ => return None,
    };
    (d < base).then_some(d)
}
