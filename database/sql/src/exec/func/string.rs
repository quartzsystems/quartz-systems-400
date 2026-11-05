// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Scalar functions on character strings: `TRANSLATE`, `OVERLAY`,
//! `INSERT`, `REPEAT`, `SPACE`, `LPAD`/`RPAD`, `INSTR` and
//! `LOCATE_IN_STRING`, `ASCII`/`CHR`, `SOUNDEX`/`DIFFERENCE`, the
//! `REGEXP_*` family, `GREATEST`/`LEAST`, `NVL` and `DECODE`.

use super::super::SqlContext;
use super::super::rtype::{Kind, SqlType, unify};
use super::{arity, int, text};
use crate::ast::Function;
use crate::error::{Result, SqlError};
use crate::value::Value;

/// The longest string a function makes.
const MAX: usize = 32_740;

fn chars(v: &Value) -> Vec<char> {
    v.as_text().chars().collect()
}

fn count(v: &Value) -> Result<usize> {
    let n = v.as_i64()?;
    usize::try_from(n).map_err(|_| SqlError::Overflow { what: format!("{n} is not a length") })
}

fn too_long(n: usize) -> Result<()> {
    if n > MAX {
        return Err(SqlError::TooLong { what: format!("a string of {n} characters") });
    }
    Ok(())
}

/// Calls `name` when this family has it.
pub(crate) fn call(name: &str, args: &[Value], ctx: &SqlContext) -> Option<Result<Value>> {
    let r = match name {
        // STRIP(s, B|L|T, c) as TRIM(B c FROM s).
        "STRIP" if args.len() == 3 && matches!(&args[1], Value::Str(_)) => {
            super::call("TRIM", &[args[1].clone(), args[2].clone(), args[0].clone()], ctx)
        }
        "NVL" => nvl(args),
        "DECODE" => decode(args),
        "GREATEST" | "LEAST" => {
            if args.iter().any(Value::is_null) {
                return Some(arity(name, args, 2, 255).map(|()| Value::Null));
            }
            extreme(name, args)
        }
        "TRANSLATE" | "OVERLAY" | "INSERT" | "REPEAT" | "SPACE" | "LPAD" | "RPAD" | "INSTR" | "LOCATE_IN_STRING"
        | "ASCII" | "CHR" | "SOUNDEX" | "DIFFERENCE" | "REGEXP_COUNT" | "REGEXP_INSTR" | "REGEXP_SUBSTR"
        | "REGEXP_REPLACE" | "REGEXP_LIKE" => {
            if args.iter().any(Value::is_null) {
                return Some(Ok(Value::Null));
            }
            match name {
                "TRANSLATE" => translate(args),
                "OVERLAY" | "INSERT" => overlay(name, args),
                "REPEAT" => repeat(args),
                "SPACE" => arity(name, args, 1, 1).and_then(|()| {
                    let n = count(&args[0])?;
                    too_long(n)?;
                    Ok(text(" ".repeat(n)))
                }),
                "LPAD" | "RPAD" => pad(name, args),
                "INSTR" | "LOCATE_IN_STRING" => instr(name, args),
                "ASCII" => arity(name, args, 1, 1).map(|()| int(args[0].as_text().chars().next().map_or(0, |c| i64::from(u32::from(c))))),
                "CHR" => arity(name, args, 1, 1).and_then(|()| {
                    let n = args[0].as_i64()?;
                    Ok(u32::try_from(n).ok().and_then(char::from_u32).map_or(Value::Null, |c| text(c.to_string())))
                }),
                "SOUNDEX" => arity(name, args, 1, 1).map(|()| text(soundex(&args[0].as_text()))),
                "DIFFERENCE" => arity(name, args, 2, 2).map(|()| {
                    let a: Vec<char> = soundex(&args[0].as_text()).chars().collect();
                    let b: Vec<char> = soundex(&args[1].as_text()).chars().collect();
                    int(i64::try_from(a.iter().zip(&b).filter(|(x, y)| x == y).count()).expect("small"))
                }),
                _ => super::regexp::call(name, args),
            }
        }
        _ => return None,
    };
    Some(r)
}

fn nvl(args: &[Value]) -> Result<Value> {
    arity("NVL", args, 2, 255)?;
    Ok(args.iter().find(|v| !v.is_null()).cloned().unwrap_or(Value::Null))
}

/// `DECODE(e, s1, r1, s2, r2, ..., [else])`: the result of the first
/// search value equal to `e`, a null equal to a null.
fn decode(args: &[Value]) -> Result<Value> {
    arity("DECODE", args, 3, 255)?;
    let e = &args[0];
    let pairs = &args[1..];
    for p in pairs.chunks(2) {
        if p.len() == 2 && super::super::eval::not_distinct(e, &p[0])? {
            return Ok(p[1].clone());
        }
    }
    Ok(if pairs.len() % 2 == 1 { pairs[pairs.len() - 1].clone() } else { Value::Null })
}

fn extreme(name: &str, args: &[Value]) -> Result<Value> {
    arity(name, args, 2, 255)?;
    let want = if name == "LEAST" { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater };
    let mut best = args[0].clone();
    for v in &args[1..] {
        if v.compare(&best)? == Some(want) {
            best = v.clone();
        }
    }
    Ok(best)
}

/// `TRANSLATE(s)` is `UPPER`; `TRANSLATE(s, to, from [, pad])` maps each
/// character of `from` to the one of `to` at its place, `pad` (blank)
/// past the end of `to`.
fn translate(args: &[Value]) -> Result<Value> {
    arity("TRANSLATE", args, 1, 4)?;
    let s = args[0].as_text();
    if args.len() == 1 {
        return Ok(text(s.to_uppercase()));
    }
    let to = chars(&args[1]);
    let from = if args.len() > 2 { chars(&args[2]) } else { Vec::new() };
    let pad = args.get(3).and_then(|p| p.as_text().chars().next()).unwrap_or(' ');
    Ok(text(
        s.chars()
            .map(|c| match from.iter().position(|&f| f == c) {
                Some(i) => to.get(i).copied().unwrap_or(pad),
                None => c,
            })
            .collect::<String>(),
    ))
}

/// `OVERLAY(s, t, start [, len])` and `INSERT(s, start, len, t)`: `len`
/// characters of `s` from `start` replaced by `t` (by default, as many
/// as `t` has).
fn overlay(name: &str, args: &[Value]) -> Result<Value> {
    let (s, t, start, len) = if name == "INSERT" {
        arity(name, args, 4, 4)?;
        (chars(&args[0]), chars(&args[3]), args[1].as_i64()?, Some(args[2].as_i64()?))
    } else {
        arity(name, args, 3, 5)?;
        (chars(&args[0]), chars(&args[1]), args[2].as_i64()?, args.get(3).map(Value::as_i64).transpose()?)
    };
    let n = i64::try_from(s.len()).expect("fits");
    if start < 1 || start > n + 1 {
        return Err(SqlError::Overflow { what: format!("{name} start {start} is outside the string") });
    }
    // Without a length one character is replaced, none past the end
    // (pub400).
    let len = len.unwrap_or(i64::from(start <= n));
    if len < 0 {
        return Err(SqlError::Overflow { what: format!("{name} length {len} is below 0") });
    }
    let at = usize::try_from(start - 1).expect("positive");
    let end = (at + usize::try_from(len).expect("positive")).min(s.len());
    let mut out: String = s[..at].iter().collect();
    out.extend(&t);
    out.extend(&s[end..]);
    Ok(text(out))
}

fn repeat(args: &[Value]) -> Result<Value> {
    arity("REPEAT", args, 2, 2)?;
    let s = args[0].as_text();
    let n = count(&args[1])?;
    too_long(s.chars().count() * n)?;
    Ok(text(s.repeat(n)))
}

/// `LPAD(s, n [, pad])`, `RPAD`: `s` to `n` characters, cut or filled
/// with `pad` (blanks) repeated.
fn pad(name: &str, args: &[Value]) -> Result<Value> {
    arity(name, args, 2, 3)?;
    let s = chars(&args[0]);
    let n = count(&args[1])?;
    too_long(n)?;
    let fill = args.get(2).map_or_else(|| vec![' '], chars);
    if n <= s.len() {
        return Ok(text(s[..n].iter().collect::<String>()));
    }
    if fill.is_empty() {
        return Ok(text(s.iter().collect::<String>()));
    }
    let padding: String = fill.iter().cycle().take(n - s.len()).collect();
    let body: String = s.iter().collect();
    Ok(text(if name == "LPAD" { padding + &body } else { body + &padding }))
}

/// `INSTR(hay, needle [, start [, n]])`, `LOCATE_IN_STRING`: where the
/// `n`th match starts (1 based), 0 when none. A negative start searches
/// back from that place from the end.
fn instr(name: &str, args: &[Value]) -> Result<Value> {
    arity(name, args, 2, 4)?;
    let hay = chars(&args[0]);
    let needle = chars(&args[1]);
    let start = args.get(2).map_or(Ok(1), Value::as_i64)?;
    let nth = args.get(3).map_or(Ok(1), Value::as_i64)?;
    if start == 0 || nth < 1 {
        return Err(SqlError::Overflow { what: format!("{name}: start 0 or an occurrence below 1") });
    }
    let len = i64::try_from(hay.len()).expect("fits");
    let nl = i64::try_from(needle.len()).expect("fits");
    let at = |i: i64| -> bool {
        usize::try_from(i).is_ok_and(|i| i + needle.len() <= hay.len() && hay[i..i + needle.len()] == needle[..])
    };
    let mut seen = 0;
    if start > 0 {
        let mut i = start - 1;
        while i + nl <= len {
            if at(i) {
                seen += 1;
                if seen == nth {
                    return Ok(int(i + 1));
                }
            }
            i += 1;
        }
    } else {
        let mut i = (len + start).min(len - nl);
        while i >= 0 {
            if at(i) {
                seen += 1;
                if seen == nth {
                    return Ok(int(i + 1));
                }
            }
            i -= 1;
        }
    }
    Ok(int(0))
}

/// The four-character Soundex code.
fn soundex(s: &str) -> String {
    let code = |c: char| match c.to_ascii_uppercase() {
        'B' | 'F' | 'P' | 'V' => Some('1'),
        'C' | 'G' | 'J' | 'K' | 'Q' | 'S' | 'X' | 'Z' => Some('2'),
        'D' | 'T' => Some('3'),
        'L' => Some('4'),
        'M' | 'N' => Some('5'),
        'R' => Some('6'),
        _ => None,
    };
    let letters: Vec<char> = s.chars().filter(char::is_ascii_alphabetic).collect();
    let Some(&first) = letters.first() else {
        return "0000".into();
    };
    let mut out = String::from(first.to_ascii_uppercase());
    let mut last = code(first);
    for &c in &letters[1..] {
        let k = code(c);
        let u = c.to_ascii_uppercase();
        if let Some(code) = k
            && k != last
        {
            out.push(code);
            if out.len() == 4 {
                break;
            }
        }
        // H and W do not separate letters of one code; vowels do.
        if u != 'H' && u != 'W' {
            last = k;
        }
    }
    while out.len() < 4 {
        out.push('0');
    }
    out
}

/// The result type of `name` when this family has it.
pub(crate) fn result_type(name: &str, args: &[SqlType], f: &Function) -> Option<SqlType> {
    let null = args.iter().any(|a| a.null);
    let first = args.first().copied().unwrap_or_else(SqlType::unknown);
    let lit = |i: usize| match f.args.get(i) {
        Some(crate::ast::Expr::Literal(crate::ast::Literal::Num(n))) => n.parse::<u32>().ok(),
        _ => None,
    };
    let len = |t: &SqlType| t.len.max(1);
    Some(match name {
        "DECODE" => {
            let mut t: Option<SqlType> = None;
            for (i, a) in args.iter().enumerate().skip(1) {
                if i % 2 == 0 || i == args.len() - 1 {
                    t = Some(t.map_or(*a, |t| unify(t, *a)));
                }
            }
            t.unwrap_or_else(SqlType::unknown).nullable(true)
        }
        "GREATEST" | "LEAST" => {
            let mut t = first;
            for a in &args[1.min(args.len())..] {
                t = unify(t, *a);
            }
            t.nullable(null)
        }
        // SUBSTRING(s, n, m, OCTETS): a CHAR of m (pub400).
        "SUBSTRING" if args.len() == 4 && lit(2).is_some() => SqlType::char(lit(2).unwrap_or(1), null),
        "TRANSLATE" => SqlType::varchar(len(&first), null),
        "OVERLAY" => {
            let t = args.get(1).map_or(0, |t| t.len);
            let cut = if args.len() > 3 { lit(3).unwrap_or(0) } else { u32::from(lit(2).is_none_or(|s| s <= first.len)) };
            SqlType::varchar((first.len + t).saturating_sub(cut).max(1), null)
        }
        "INSERT" => {
            let t = args.get(3).map_or(0, |t| t.len);
            SqlType::varchar((first.len + t).saturating_sub(lit(2).unwrap_or(0)).max(1), null)
        }
        "REPEAT" => SqlType::varchar(lit(1).map_or(4000, |n| first.len * n).clamp(1, 32740), null),
        "SPACE" => SqlType::varchar(lit(0).unwrap_or(4000).clamp(1, 32740), null),
        "LPAD" | "RPAD" => SqlType::varchar(lit(1).unwrap_or(4000).clamp(1, 32740), null),
        "INSTR" | "LOCATE_IN_STRING" | "ASCII" | "DIFFERENCE" => SqlType::int(null),
        "CHR" => SqlType::char(1, true),
        "SOUNDEX" => SqlType::char(4, null),
        "REGEXP_COUNT" | "REGEXP_INSTR" => SqlType::int(true),
        "REGEXP_SUBSTR" => SqlType::varchar(len(&first), true),
        "REGEXP_REPLACE" => {
            let r = args.get(2).map_or(0, |t| t.len);
            SqlType::varchar((first.len * (r + 1) + r).clamp(1, 32740), true)
        }
        "REGEXP_LIKE" => SqlType::new(Kind::Boolean, 1, 0, true),
        _ => return None,
    })
}
