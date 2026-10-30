// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `expr operand...`: evaluate an expression and print the result.
//!
//! The POSIX grammar, lowest precedence first: `|`, `&`, the
//! comparisons `= > >= < <= !=` (numeric when both sides are integers,
//! else byte order), `+ -`, `* / %`, then `:` (a basic regular
//! expression anchored at the start; the first group's text when it
//! has one, else the length of the match), with parentheses. Integers
//! are signed 64-bit; overflow and division by zero are errors.
//! Status 0 when the result is neither empty nor zero, 1 when it is,
//! 2 for an invalid expression or an arithmetic error.

use qsh::sys::UtilCtx;

use crate::common::complain;
use crate::common::io::Out;
use crate::regex::{Regex, Syntax};

const NAME: &str = "expr";

/// A value: an integer or a string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// An integer.
    Int(i64),
    /// An operand's text (an integer when it reads as one).
    Str(String),
}

impl Value {
    /// An operand keeps its text (`expr 007` prints `007`).
    fn of(s: &str) -> Self {
        Self::Str(s.to_string())
    }

    /// The value as text.
    pub fn text(&self) -> String {
        match self {
            Self::Int(n) => n.to_string(),
            Self::Str(s) => s.clone(),
        }
    }

    /// Empty or zero.
    pub fn is_null(&self) -> bool {
        match self {
            Self::Int(n) => *n == 0,
            Self::Str(s) => s.is_empty() || int(s) == Some(0),
        }
    }

    fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(n) => Some(*n),
            Self::Str(s) => int(s),
        }
    }
}

/// A decimal integer with an optional leading minus.
fn int(s: &str) -> Option<i64> {
    let digits = s.strip_prefix('-').unwrap_or(s);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

struct Parser<'a> {
    toks: &'a [String],
    i: usize,
}

fn is_op(t: &str) -> bool {
    matches!(
        t,
        "|" | "&" | "=" | ">" | ">=" | "<" | "<=" | "!=" | "+" | "-" | "*" | "/" | "%" | ":"
    )
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a str> {
        self.toks.get(self.i).map(String::as_str)
    }

    fn take(&mut self, ops: &[&str]) -> Option<String> {
        let t = self.peek()?;
        if ops.contains(&t) {
            self.i += 1;
            Some(t.to_string())
        } else {
            None
        }
    }

    fn or(&mut self) -> Result<Value, String> {
        let mut l = self.and()?;
        while self.take(&["|"]).is_some() {
            let r = self.and()?;
            l = if l.is_null() { r } else { l };
        }
        Ok(l)
    }

    fn and(&mut self) -> Result<Value, String> {
        let mut l = self.cmp()?;
        while self.take(&["&"]).is_some() {
            let r = self.cmp()?;
            l = if l.is_null() || r.is_null() {
                Value::Int(0)
            } else {
                l
            };
        }
        Ok(l)
    }

    fn cmp(&mut self) -> Result<Value, String> {
        let mut l = self.add()?;
        while let Some(op) = self.take(&["=", ">", ">=", "<", "<=", "!="]) {
            let r = self.add()?;
            let ord = match (l.as_int(), r.as_int()) {
                (Some(a), Some(b)) => a.cmp(&b),
                _ => l.text().as_bytes().cmp(r.text().as_bytes()),
            };
            let t = match op.as_str() {
                "=" => ord.is_eq(),
                ">" => ord.is_gt(),
                ">=" => ord.is_ge(),
                "<" => ord.is_lt(),
                "<=" => ord.is_le(),
                _ => ord.is_ne(),
            };
            l = Value::Int(i64::from(t));
        }
        Ok(l)
    }

    fn add(&mut self) -> Result<Value, String> {
        let mut l = self.mul()?;
        while let Some(op) = self.take(&["+", "-"]) {
            let r = self.mul()?;
            let (a, b) = ints(&l, &r)?;
            let v = if op == "+" {
                a.checked_add(b)
            } else {
                a.checked_sub(b)
            };
            l = Value::Int(v.ok_or("The result is too large.")?);
        }
        Ok(l)
    }

    fn mul(&mut self) -> Result<Value, String> {
        let mut l = self.colon()?;
        while let Some(op) = self.take(&["*", "/", "%"]) {
            let r = self.colon()?;
            let (a, b) = ints(&l, &r)?;
            if op != "*" && b == 0 {
                return Err("Division by zero.".to_string());
            }
            let v = match op.as_str() {
                "*" => a.checked_mul(b),
                "/" => a.checked_div(b),
                _ => a.checked_rem(b),
            };
            l = Value::Int(v.ok_or("The result is too large.")?);
        }
        Ok(l)
    }

    fn colon(&mut self) -> Result<Value, String> {
        let mut l = self.primary()?;
        while self.take(&[":"]).is_some() {
            let r = self.primary()?;
            l = matches(&l.text(), &r.text())?;
        }
        Ok(l)
    }

    fn primary(&mut self) -> Result<Value, String> {
        let Some(t) = self.peek().map(str::to_string) else {
            return Err("An operand is missing.".to_string());
        };
        if t == "(" && self.i + 1 < self.toks.len() {
            self.i += 1;
            let v = self.or()?;
            if self.take(&[")"]).is_none() {
                return Err("The closing ) is missing.".to_string());
            }
            return Ok(v);
        }
        if t == ")" && self.i > 0 && self.i + 1 < self.toks.len() {
            return Err("The ) is not expected.".to_string());
        }
        self.i += 1;
        Ok(Value::of(&t))
    }
}

fn ints(l: &Value, r: &Value) -> Result<(i64, i64), String> {
    match (l.as_int(), r.as_int()) {
        (Some(a), Some(b)) => Ok((a, b)),
        _ => Err("An operand is not an integer.".to_string()),
    }
}

/// `string : pattern`.
fn matches(s: &str, pat: &str) -> Result<Value, String> {
    let mut full = b"^".to_vec();
    full.extend_from_slice(pat.as_bytes());
    let re = Regex::new(&full, Syntax::Basic, false).map_err(|e| e.to_string())?;
    let text = s.as_bytes();
    let m = re.find_at(text, 0);
    if re.groups() > 0 {
        let g = m
            .and_then(|m| m.group(text, 1).map(|b| String::from_utf8_lossy(b).into_owned()))
            .unwrap_or_default();
        return Ok(Value::Str(g));
    }
    let n = m.map_or(0, |m| String::from_utf8_lossy(&text[m.start..m.end]).chars().count());
    Ok(Value::Int(n as i64))
}

/// Evaluate the arguments of `expr`.
pub fn eval(args: &[String]) -> Result<Value, String> {
    if args.is_empty() {
        return Err("An expression is needed.".to_string());
    }
    let mut p = Parser { toks: args, i: 0 };
    let v = p.or()?;
    if let Some(t) = p.peek() {
        return Err(if is_op(t) {
            format!("The operator {t} is not expected.")
        } else {
            format!("The operand {t} is not expected.")
        });
    }
    Ok(v)
}

/// Run `expr`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let mut args = &cx.argv[1..];
    if args.len() > 1 && args[0] == "--" {
        args = &args[1..];
    }
    match eval(args) {
        Ok(v) => {
            let mut out = Out::stdout(cx);
            out.text(&v.text());
            out.text("\n");
            out.finish(cx, NAME, i32::from(v.is_null()))
        }
        Err(e) => {
            complain(cx, NAME, &e);
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(s: &[&str]) -> Result<String, String> {
        let v: Vec<String> = s.iter().map(ToString::to_string).collect();
        eval(&v).map(|v| v.text())
    }

    #[test]
    fn basics() {
        assert_eq!(e(&["1", "+", "2", "*", "3"]).unwrap(), "7");
        assert_eq!(e(&["(", "1", "+", "2", ")", "*", "3"]).unwrap(), "9");
        assert_eq!(e(&["abc", ":", "a\\(.\\)"]).unwrap(), "b");
        assert_eq!(e(&["abc", ":", ".*"]).unwrap(), "3");
        assert_eq!(e(&["", "|", "x"]).unwrap(), "x");
        assert_eq!(e(&["10", "<", "9"]).unwrap(), "0");
        assert_eq!(e(&["a10", "<", "a9"]).unwrap(), "1");
        assert!(e(&["1", "/", "0"]).is_err());
        assert!(e(&["9223372036854775807", "+", "1"]).is_err());
    }
}
