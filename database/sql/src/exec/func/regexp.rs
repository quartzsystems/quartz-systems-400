// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `REGEXP_LIKE`, `REGEXP_COUNT`, `REGEXP_INSTR`, `REGEXP_SUBSTR` and
//! `REGEXP_REPLACE` over the `regex` crate. Positions count characters
//! from 1; the flags are IBM's (`c` case, `i` no case, `m` lines, `n`
//! or `s` a dot that matches a new line, `x` blanks ignored); a
//! replacement names groups as `\1` to `\9`.

use std::fmt::Write as _;

use regex::{Regex, RegexBuilder};

use super::{arity, int, text};
use crate::error::{Result, SqlError};
use crate::value::Value;

fn compile(pattern: &str, flags: &str) -> Result<Regex> {
    let mut b = RegexBuilder::new(pattern);
    for f in flags.chars() {
        match f {
            'c' => {
                b.case_insensitive(false);
            }
            'i' => {
                b.case_insensitive(true);
            }
            'm' => {
                b.multi_line(true);
            }
            'n' | 's' => {
                b.dot_matches_new_line(true);
            }
            'x' => {
                b.ignore_whitespace(true);
            }
            ' ' => {}
            other => return Err(SqlError::Overflow { what: format!("regular expression flag {other} is not valid") }),
        }
    }
    b.build().map_err(|e| SqlError::Conversion { what: format!("the regular expression is not valid: {e}") })
}

/// The byte offset of character `n` (from 1) of `s`; `None` past the end.
fn byte_at(s: &str, n: i64) -> Result<Option<usize>> {
    if n < 1 {
        return Err(SqlError::Overflow { what: format!("start {n} is below 1") });
    }
    let n = usize::try_from(n - 1).expect("positive");
    if n == s.chars().count() {
        return Ok(Some(s.len()));
    }
    Ok(s.char_indices().nth(n).map(|(b, _)| b))
}

/// The character position (from 1) of byte offset `b`.
fn char_pos(s: &str, b: usize) -> i64 {
    i64::try_from(s[..b].chars().count() + 1).expect("fits")
}

fn opt_i64(args: &[Value], i: usize, d: i64) -> Result<i64> {
    args.get(i).map_or(Ok(d), Value::as_i64)
}

fn opt_text(args: &[Value], i: usize) -> String {
    args.get(i).map(Value::as_text).unwrap_or_default()
}

/// A replacement in IBM's form (`\1`) in the crate's (`${1}`).
fn replacement(r: &str) -> String {
    let mut out = String::new();
    let mut it = r.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' => match it.peek() {
                Some(d) if d.is_ascii_digit() => {
                    let _ = write!(out, "${{{d}}}");
                    it.next();
                }
                Some('\\') => {
                    out.push('\\');
                    it.next();
                }
                _ => out.push('\\'),
            },
            '$' => out.push_str("$$"),
            c => out.push(c),
        }
    }
    out
}

pub(super) fn call(name: &str, args: &[Value]) -> Result<Value> {
    let src = args.first().map(Value::as_text).unwrap_or_default();
    let src = src.as_str();
    match name {
        "REGEXP_LIKE" => {
            arity(name, args, 2, 4)?;
            // REGEXP_LIKE(s, p [, start] [, flags]).
            let (start, flags) = match args.get(2) {
                Some(Value::Str(f)) => (1, f.clone()),
                Some(v) => (v.as_i64()?, opt_text(args, 3)),
                None => (1, String::new()),
            };
            let re = compile(&args[1].as_text(), &flags)?;
            let Some(b) = byte_at(src, start)? else { return Ok(Value::Bool(false)) };
            Ok(Value::Bool(re.is_match(&src[b..])))
        }
        "REGEXP_COUNT" => {
            arity(name, args, 2, 4)?;
            let re = compile(&args[1].as_text(), &opt_text(args, 3))?;
            let Some(b) = byte_at(src, opt_i64(args, 2, 1)?)? else { return Ok(int(0)) };
            Ok(int(i64::try_from(re.find_iter(&src[b..]).count()).expect("fits")))
        }
        "REGEXP_INSTR" => {
            arity(name, args, 2, 7)?;
            let re = compile(&args[1].as_text(), &opt_text(args, 5))?;
            let occurrence = opt_i64(args, 3, 1)?;
            let after = opt_i64(args, 4, 0)?;
            let group = usize::try_from(opt_i64(args, 6, 0)?).unwrap_or(0);
            if occurrence < 1 || !(0..=1).contains(&after) {
                return Err(SqlError::Overflow { what: "REGEXP_INSTR occurrence or return option not valid".into() });
            }
            let Some(b) = byte_at(src, opt_i64(args, 2, 1)?)? else { return Ok(int(0)) };
            let n = usize::try_from(occurrence - 1).expect("positive");
            let Some(caps) = re.captures_iter(&src[b..]).nth(n) else { return Ok(int(0)) };
            let Some(m) = caps.get(group) else { return Ok(int(0)) };
            Ok(int(char_pos(src, b + if after == 1 { m.end() } else { m.start() })))
        }
        "REGEXP_SUBSTR" => {
            arity(name, args, 2, 6)?;
            let re = compile(&args[1].as_text(), &opt_text(args, 4))?;
            let occurrence = opt_i64(args, 3, 1)?;
            let group = usize::try_from(opt_i64(args, 5, 0)?).unwrap_or(0);
            if occurrence < 1 {
                return Err(SqlError::Overflow { what: "REGEXP_SUBSTR occurrence below 1".into() });
            }
            let Some(b) = byte_at(src, opt_i64(args, 2, 1)?)? else { return Ok(Value::Null) };
            let n = usize::try_from(occurrence - 1).expect("positive");
            Ok(re
                .captures_iter(&src[b..])
                .nth(n)
                .and_then(|c| c.get(group).map(|m| text(m.as_str())))
                .unwrap_or(Value::Null))
        }
        "REGEXP_REPLACE" => {
            arity(name, args, 2, 6)?;
            let re = compile(&args[1].as_text(), &opt_text(args, 5))?;
            let rep = replacement(&opt_text(args, 2));
            let occurrence = opt_i64(args, 4, 0)?;
            if occurrence < 0 {
                return Err(SqlError::Overflow { what: "REGEXP_REPLACE occurrence below 0".into() });
            }
            let Some(b) = byte_at(src, opt_i64(args, 3, 1)?)? else { return Ok(text(src)) };
            let (head, tail) = src.split_at(b);
            let replaced = if occurrence == 0 {
                re.replace_all(tail, rep.as_str()).into_owned()
            } else {
                let n = usize::try_from(occurrence - 1).expect("positive");
                match re.captures_iter(tail).nth(n) {
                    None => tail.to_string(),
                    Some(c) => {
                        let m = c.get(0).expect("whole match");
                        let mut out = String::from(&tail[..m.start()]);
                        c.expand(&rep, &mut out);
                        out.push_str(&tail[m.end()..]);
                        out
                    }
                }
            };
            Ok(text(format!("{head}{replaced}")))
        }
        _ => unreachable!("regexp names"),
    }
}
