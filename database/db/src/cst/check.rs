// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compiled condition of a check constraint: a small expression
//! tree over a record's fields, kept in the constraint's entry and
//! evaluated on every write by whoever writes (SQL, RPG, RLA). The SQL
//! compiler (`sql::exec::cst`) builds it; this crate does not parse SQL.
//!
//! Text is compared as CP037 bytes with blanks added to the shorter (the
//! `*HEX` sort sequence); a condition that is unknown (a null took part)
//! is met, as SQL says.

#![allow(clippy::many_single_char_names)]

use std::cmp::Ordering;

use qdecimal::Decimal;
use qsname::Name;

use crate::error::{DbError, Result};
use crate::format::FormatDef;
use crate::record::{self, Value};

/// A comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    /// `=`.
    Eq,
    /// `<>`.
    Ne,
    /// `<`.
    Lt,
    /// `<=`.
    Le,
    /// `>`.
    Gt,
    /// `>=`.
    Ge,
}

/// An arithmetic operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arith {
    /// `+`.
    Add,
    /// `-`.
    Sub,
    /// `*`.
    Mul,
    /// `/`.
    Div,
}

/// A built-in function a check may call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Func {
    /// `UPPER`/`UCASE`.
    Upper,
    /// `LOWER`/`LCASE`.
    Lower,
    /// `LENGTH`.
    Length,
    /// `SUBSTR(s, start [, len])`.
    Substr,
    /// `TRIM`.
    Trim,
    /// `LTRIM`.
    Ltrim,
    /// `RTRIM`.
    Rtrim,
    /// `ABS`.
    Abs,
    /// `MOD`.
    Mod,
    /// `COALESCE`/`IFNULL`/`VALUE`.
    Coalesce,
}

impl Func {
    /// The function a name calls, when a check may call it.
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        Some(match name {
            "UPPER" | "UCASE" => Self::Upper,
            "LOWER" | "LCASE" => Self::Lower,
            "LENGTH" => Self::Length,
            "SUBSTR" | "SUBSTRING" => Self::Substr,
            "TRIM" | "STRIP" => Self::Trim,
            "LTRIM" => Self::Ltrim,
            "RTRIM" => Self::Rtrim,
            "ABS" => Self::Abs,
            "MOD" => Self::Mod,
            "COALESCE" | "IFNULL" | "VALUE" => Self::Coalesce,
            _ => return None,
        })
    }

    fn code(self) -> u8 {
        self as u8
    }

    fn from_code(b: u8) -> Option<Self> {
        const ALL: [Func; 10] = [
            Func::Upper,
            Func::Lower,
            Func::Length,
            Func::Substr,
            Func::Trim,
            Func::Ltrim,
            Func::Rtrim,
            Func::Abs,
            Func::Mod,
            Func::Coalesce,
        ];
        ALL.into_iter().find(|f| f.code() == b)
    }
}

/// A check condition or a value in it.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// A field of the record, by system name.
    Col(Name),
    /// A number.
    Num(Decimal),
    /// A floating-point number.
    Float(f64),
    /// A character string.
    Str(String),
    /// `NULL`.
    Null,
    /// Unary minus.
    Neg(Box<Expr>),
    /// `a op b`.
    Arith(Arith, Box<Expr>, Box<Expr>),
    /// `a || b`.
    Concat(Box<Expr>, Box<Expr>),
    /// `a op b`.
    Cmp(Cmp, Box<Expr>, Box<Expr>),
    /// `a AND b`.
    And(Box<Expr>, Box<Expr>),
    /// `a OR b`.
    Or(Box<Expr>, Box<Expr>),
    /// `NOT a`.
    Not(Box<Expr>),
    /// `a IS [NOT] NULL` (true for `NOT`).
    IsNull(Box<Expr>, bool),
    /// `a [NOT] BETWEEN lo AND hi`.
    Between(Box<Expr>, Box<Expr>, Box<Expr>, bool),
    /// `a [NOT] IN (list)`.
    In(Box<Expr>, Vec<Expr>, bool),
    /// `a [NOT] LIKE pattern [ESCAPE e]`.
    Like(Box<Expr>, Box<Expr>, Option<Box<Expr>>, bool),
    /// A function call.
    Call(Func, Vec<Expr>),
}

/// A value while a condition is evaluated.
#[derive(Clone, Debug, PartialEq)]
enum V {
    Null,
    Num(Decimal),
    Float(f64),
    /// CP037 text.
    Text(Vec<u8>),
    Bool(bool),
}

fn eval_err(what: impl Into<String>) -> DbError {
    DbError::Definition { what: format!("check condition: {}", what.into()) }
}

fn cp037(s: &str) -> Vec<u8> {
    s.chars().map(|c| ccsid::byte_of(c).unwrap_or(ccsid::SPACE)).collect()
}

fn text_of(b: &[u8]) -> String {
    b.iter().map(|&c| ccsid::char_of(c)).collect()
}

/// Where a condition reads its fields.
pub struct Row<'a> {
    /// The record's format.
    pub format: &'a FormatDef,
    /// The record.
    pub data: &'a [u8],
    /// Its null map.
    pub nulls: &'a [u8],
}

impl Row<'_> {
    fn field(&self, name: &Name) -> Result<V> {
        let (i, f) = self
            .format
            .format
            .field(name)
            .ok_or_else(|| eval_err(format!("field {name} is not in format {}", self.format.format.name)))?;
        let v = record::read_field(f, self.data, self.nulls, self.format.null_index(i))?;
        Ok(match v {
            Value::Null => V::Null,
            Value::Num(d) => V::Num(d),
            Value::Float(x) => V::Float(x),
            Value::DecFloat(d) => V::Float(d.to_f64()),
            Value::Char(b) => V::Text(b),
        })
    }
}

impl Expr {
    /// Whether the condition is met by the record: true unless it is
    /// false (an unknown result meets it).
    pub fn met(&self, row: &Row<'_>) -> Result<bool> {
        Ok(match self.eval(row)? {
            V::Bool(b) => b,
            V::Null => true,
            _ => return Err(eval_err("the condition is not a predicate")),
        })
    }

    /// The fields the condition reads.
    #[must_use]
    pub fn columns(&self) -> Vec<Name> {
        let mut out = Vec::new();
        self.walk(&mut |e| {
            if let Expr::Col(n) = e
                && !out.contains(n)
            {
                out.push(*n);
            }
        });
        out
    }

    fn walk(&self, f: &mut dyn FnMut(&Expr)) {
        f(self);
        match self {
            Expr::Col(_) | Expr::Num(_) | Expr::Float(_) | Expr::Str(_) | Expr::Null => {}
            Expr::Neg(a) | Expr::Not(a) | Expr::IsNull(a, _) => a.walk(f),
            Expr::Arith(_, a, b) | Expr::Concat(a, b) | Expr::Cmp(_, a, b) | Expr::And(a, b) | Expr::Or(a, b) => {
                a.walk(f);
                b.walk(f);
            }
            Expr::Between(a, b, c, _) => {
                a.walk(f);
                b.walk(f);
                c.walk(f);
            }
            Expr::In(a, l, _) => {
                a.walk(f);
                for e in l { e.walk(f); }
            }
            Expr::Like(a, b, c, _) => {
                a.walk(f);
                b.walk(f);
                if let Some(c) = c {
                    c.walk(f);
                }
            }
            Expr::Call(_, args) => args.iter().for_each(|e| e.walk(f)),
        }
    }

    fn eval(&self, row: &Row<'_>) -> Result<V> {
        Ok(match self {
            Expr::Col(n) => row.field(n)?,
            Expr::Num(d) => V::Num(d.clone()),
            Expr::Float(x) => V::Float(*x),
            Expr::Str(s) => V::Text(cp037(s)),
            Expr::Null => V::Null,
            Expr::Neg(a) => match a.eval(row)? {
                V::Num(d) => V::Num(d.negated()),
                V::Float(x) => V::Float(-x),
                V::Null => V::Null,
                _ => return Err(eval_err("minus on a string")),
            },
            Expr::Arith(op, a, b) => arith(*op, a.eval(row)?, b.eval(row)?)?,
            Expr::Concat(a, b) => match (a.eval(row)?, b.eval(row)?) {
                (V::Null, _) | (_, V::Null) => V::Null,
                (V::Text(mut x), V::Text(y)) => {
                    x.extend_from_slice(&y);
                    V::Text(x)
                }
                _ => return Err(eval_err("|| needs strings")),
            },
            Expr::Cmp(op, a, b) => match compare(&a.eval(row)?, &b.eval(row)?)? {
                None => V::Null,
                Some(o) => V::Bool(match op {
                    Cmp::Eq => o == Ordering::Equal,
                    Cmp::Ne => o != Ordering::Equal,
                    Cmp::Lt => o == Ordering::Less,
                    Cmp::Le => o != Ordering::Greater,
                    Cmp::Gt => o == Ordering::Greater,
                    Cmp::Ge => o != Ordering::Less,
                }),
            },
            Expr::And(a, b) => match (truth(&a.eval(row)?)?, truth(&b.eval(row)?)?) {
                (Some(false), _) | (_, Some(false)) => V::Bool(false),
                (Some(true), Some(true)) => V::Bool(true),
                _ => V::Null,
            },
            Expr::Or(a, b) => match (truth(&a.eval(row)?)?, truth(&b.eval(row)?)?) {
                (Some(true), _) | (_, Some(true)) => V::Bool(true),
                (Some(false), Some(false)) => V::Bool(false),
                _ => V::Null,
            },
            Expr::Not(a) => match truth(&a.eval(row)?)? {
                Some(b) => V::Bool(!b),
                None => V::Null,
            },
            Expr::IsNull(a, not) => V::Bool((a.eval(row)? == V::Null) != *not),
            Expr::Between(a, lo, hi, not) => {
                let v = a.eval(row)?;
                let ge = compare(&v, &lo.eval(row)?)?.map(|o| o != Ordering::Less);
                let le = compare(&v, &hi.eval(row)?)?.map(|o| o != Ordering::Greater);
                match (ge, le) {
                    (Some(false), _) | (_, Some(false)) => V::Bool(*not),
                    (Some(true), Some(true)) => V::Bool(!*not),
                    _ => V::Null,
                }
            }
            Expr::In(a, list, not) => {
                let v = a.eval(row)?;
                let mut unknown = false;
                let mut found = false;
                for e in list {
                    match compare(&v, &e.eval(row)?)? {
                        Some(Ordering::Equal) => found = true,
                        None => unknown = true,
                        Some(_) => {}
                    }
                }
                if found {
                    V::Bool(!*not)
                } else if unknown {
                    V::Null
                } else {
                    V::Bool(*not)
                }
            }
            Expr::Like(a, p, esc, not) => {
                let (V::Text(s), V::Text(pat)) = (a.eval(row)?, p.eval(row)?) else {
                    return Ok(V::Null);
                };
                let esc = match esc {
                    Some(e) => match e.eval(row)? {
                        V::Text(t) if t.len() == 1 => Some(t[0]),
                        V::Null => return Ok(V::Null),
                        _ => return Err(eval_err("ESCAPE needs one character")),
                    },
                    None => None,
                };
                V::Bool(like(&trim_end(&s), &pat, esc) != *not)
            }
            Expr::Call(f, args) => call(*f, args, row)?,
        })
    }

    /// The tree as bytes.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.put(&mut out);
        out
    }

    fn put(&self, o: &mut Vec<u8>) {
        let s = |o: &mut Vec<u8>, t: &[u8]| {
            o.extend_from_slice(&u16::try_from(t.len()).expect("short").to_le_bytes());
            o.extend_from_slice(t);
        };
        match self {
            Expr::Col(n) => {
                o.push(1);
                o.extend_from_slice(n.as_bytes());
            }
            Expr::Num(d) => {
                o.push(2);
                s(o, d.to_string().as_bytes());
            }
            Expr::Float(x) => {
                o.push(3);
                o.extend_from_slice(&x.to_bits().to_le_bytes());
            }
            Expr::Str(t) => {
                o.push(4);
                s(o, t.as_bytes());
            }
            Expr::Null => o.push(5),
            Expr::Neg(a) => {
                o.push(6);
                a.put(o);
            }
            Expr::Arith(op, a, b) => {
                o.extend_from_slice(&[7, *op as u8]);
                a.put(o);
                b.put(o);
            }
            Expr::Concat(a, b) => {
                o.push(8);
                a.put(o);
                b.put(o);
            }
            Expr::Cmp(op, a, b) => {
                o.extend_from_slice(&[9, *op as u8]);
                a.put(o);
                b.put(o);
            }
            Expr::And(a, b) => {
                o.push(10);
                a.put(o);
                b.put(o);
            }
            Expr::Or(a, b) => {
                o.push(11);
                a.put(o);
                b.put(o);
            }
            Expr::Not(a) => {
                o.push(12);
                a.put(o);
            }
            Expr::IsNull(a, not) => {
                o.extend_from_slice(&[13, u8::from(*not)]);
                a.put(o);
            }
            Expr::Between(a, b, c, not) => {
                o.extend_from_slice(&[14, u8::from(*not)]);
                a.put(o);
                b.put(o);
                c.put(o);
            }
            Expr::In(a, l, not) => {
                o.extend_from_slice(&[15, u8::from(*not)]);
                o.extend_from_slice(&u16::try_from(l.len()).expect("few").to_le_bytes());
                a.put(o);
                for e in l { e.put(o); }
            }
            Expr::Like(a, b, c, not) => {
                o.extend_from_slice(&[16, u8::from(*not), u8::from(c.is_some())]);
                a.put(o);
                b.put(o);
                if let Some(c) = c {
                    c.put(o);
                }
            }
            Expr::Call(f, args) => {
                o.extend_from_slice(&[17, f.code(), u8::try_from(args.len()).expect("few")]);
                for e in args { e.put(o); }
            }
        }
    }

    /// From [`Self::encode`]'s bytes.
    pub fn decode(b: &[u8]) -> Result<Self> {
        let mut at = 0;
        let e = Self::get(b, &mut at)?;
        if at != b.len() {
            return Err(bad());
        }
        Ok(e)
    }

    #[allow(clippy::too_many_lines)]
    fn get(b: &[u8], at: &mut usize) -> Result<Self> {
        let byte = |at: &mut usize| -> Result<u8> {
            let v = *b.get(*at).ok_or_else(bad)?;
            *at += 1;
            Ok(v)
        };
        let bytes = |at: &mut usize, n: usize| -> Result<Vec<u8>> {
            let v = b.get(*at..*at + n).ok_or_else(bad)?.to_vec();
            *at += n;
            Ok(v)
        };
        let string = |at: &mut usize| -> Result<String> {
            let n = bytes(at, 2)?;
            let n = usize::from(u16::from_le_bytes([n[0], n[1]]));
            String::from_utf8(bytes(at, n)?).map_err(|_| bad())
        };
        let sub = |at: &mut usize| -> Result<Box<Self>> { Ok(Box::new(Self::get(b, at)?)) };
        let cmp = |c: u8| -> Result<Cmp> {
            Ok(match c {
                0 => Cmp::Eq,
                1 => Cmp::Ne,
                2 => Cmp::Lt,
                3 => Cmp::Le,
                4 => Cmp::Gt,
                5 => Cmp::Ge,
                _ => return Err(bad()),
            })
        };
        let ar = |c: u8| -> Result<Arith> {
            Ok(match c {
                0 => Arith::Add,
                1 => Arith::Sub,
                2 => Arith::Mul,
                3 => Arith::Div,
                _ => return Err(bad()),
            })
        };
        Ok(match byte(at)? {
            1 => {
                let raw: [u8; 10] = bytes(at, 10)?.try_into().expect("10");
                Self::Col(Name::from_bytes(raw))
            }
            2 => Self::Num(Decimal::parse(&string(at)?).map_err(|_| bad())?),
            3 => {
                let raw: [u8; 8] = bytes(at, 8)?.try_into().expect("8");
                Self::Float(f64::from_bits(u64::from_le_bytes(raw)))
            }
            4 => Self::Str(string(at)?),
            5 => Self::Null,
            6 => Self::Neg(sub(at)?),
            7 => {
                let op = ar(byte(at)?)?;
                Self::Arith(op, sub(at)?, sub(at)?)
            }
            8 => Self::Concat(sub(at)?, sub(at)?),
            9 => {
                let op = cmp(byte(at)?)?;
                Self::Cmp(op, sub(at)?, sub(at)?)
            }
            10 => Self::And(sub(at)?, sub(at)?),
            11 => Self::Or(sub(at)?, sub(at)?),
            12 => Self::Not(sub(at)?),
            13 => {
                let not = byte(at)? != 0;
                Self::IsNull(sub(at)?, not)
            }
            14 => {
                let not = byte(at)? != 0;
                Self::Between(sub(at)?, sub(at)?, sub(at)?, not)
            }
            15 => {
                let not = byte(at)? != 0;
                let n = bytes(at, 2)?;
                let n = u16::from_le_bytes([n[0], n[1]]);
                let a = sub(at)?;
                let mut l = Vec::new();
                for _ in 0..n {
                    l.push(Self::get(b, at)?);
                }
                Self::In(a, l, not)
            }
            16 => {
                let not = byte(at)? != 0;
                let has = byte(at)? != 0;
                let a = sub(at)?;
                let p = sub(at)?;
                let e = if has { Some(sub(at)?) } else { None };
                Self::Like(a, p, e, not)
            }
            17 => {
                let f = Func::from_code(byte(at)?).ok_or_else(bad)?;
                let n = byte(at)?;
                let mut args = Vec::new();
                for _ in 0..n {
                    args.push(Self::get(b, at)?);
                }
                Self::Call(f, args)
            }
            _ => return Err(bad()),
        })
    }
}

fn bad() -> DbError {
    DbError::Corrupt { what: "check constraint program".into() }
}

fn truth(v: &V) -> Result<Option<bool>> {
    match v {
        V::Bool(b) => Ok(Some(*b)),
        V::Null => Ok(None),
        _ => Err(eval_err("a value where a predicate belongs")),
    }
}

fn to_f64(d: &Decimal) -> f64 {
    record::decimal_f64(d)
}

fn arith(op: Arith, a: V, b: V) -> Result<V> {
    Ok(match (a, b) {
        (V::Null, _) | (_, V::Null) => V::Null,
        (V::Num(x), V::Num(y)) => V::Num(
            match op {
                Arith::Add => x.add_exact(&y),
                Arith::Sub => x.sub_exact(&y),
                Arith::Mul => x.mul_exact(&y),
                Arith::Div => {
                    if y.is_zero() {
                        return Err(eval_err("division by zero"));
                    }
                    x.div_to_scale(&y, x.scale().max(y.scale()).clamp(6, 31))
                }
            }
            .map_err(|e| eval_err(e.to_string()))?,
        ),
        (x, y) => {
            let f = |v: V| match v {
                V::Num(d) => Ok(to_f64(&d)),
                V::Float(x) => Ok(x),
                _ => Err(eval_err("arithmetic on a string")),
            };
            let (x, y) = (f(x)?, f(y)?);
            V::Float(match op {
                Arith::Add => x + y,
                Arith::Sub => x - y,
                Arith::Mul => x * y,
                Arith::Div => x / y,
            })
        }
    })
}

/// Blanks added to the shorter string, as SQL compares.
fn text_cmp(x: &[u8], y: &[u8]) -> Ordering {
    let n = x.len().max(y.len());
    for i in 0..n {
        let a = x.get(i).copied().unwrap_or(ccsid::SPACE);
        let b = y.get(i).copied().unwrap_or(ccsid::SPACE);
        match ccsid::ebcdic_cmp(&[a], &[b]) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    Ordering::Equal
}

fn compare(a: &V, b: &V) -> Result<Option<Ordering>> {
    Ok(Some(match (a, b) {
        (V::Null, _) | (_, V::Null) => return Ok(None),
        (V::Num(x), V::Num(y)) => x.cmp(y),
        (V::Num(x), V::Float(y)) => record::fcmp(to_f64(x), *y),
        (V::Float(x), V::Num(y)) => record::fcmp(*x, to_f64(y)),
        (V::Float(x), V::Float(y)) => record::fcmp(*x, *y),
        (V::Text(x), V::Text(y)) => text_cmp(x, y),
        // A number against a string: the string as a number.
        (V::Num(_) | V::Float(_), V::Text(t)) => {
            let n = Decimal::parse(text_of(t).trim()).map_err(|_| eval_err("a string that is not a number"))?;
            return compare(a, &V::Num(n));
        }
        (V::Text(_), V::Num(_) | V::Float(_)) => return compare(b, a).map(|o| o.map(Ordering::reverse)),
        _ => return Err(eval_err("a predicate compared")),
    }))
}

fn trim_end(s: &[u8]) -> Vec<u8> {
    let n = s.iter().rposition(|&c| c != ccsid::SPACE).map_or(0, |i| i + 1);
    s[..n].to_vec()
}

/// `LIKE` over CP037 bytes: `%` any run, `_` one character.
fn like(s: &[u8], p: &[u8], esc: Option<u8>) -> bool {
    fn go(s: &[u8], p: &[u8], pct: u8, und: u8, esc: Option<u8>) -> bool {
        let Some((&c, rest)) = p.split_first() else {
            return s.is_empty();
        };
        if Some(c) == esc {
            let Some((&lit, rest)) = rest.split_first() else {
                return false;
            };
            return s.first() == Some(&lit) && go(&s[1..], rest, pct, und, esc);
        }
        if c == pct {
            return (0..=s.len()).any(|i| go(&s[i..], rest, pct, und, esc));
        }
        if c == und {
            return !s.is_empty() && go(&s[1..], rest, pct, und, esc);
        }
        s.first() == Some(&c) && go(&s[1..], rest, pct, und, esc)
    }
    let pct = ccsid::byte_of('%').expect("in CP037");
    let und = ccsid::byte_of('_').expect("in CP037");
    go(s, &trim_end(p), pct, und, esc) || go(s, p, pct, und, esc)
}

fn int_arg(v: &V) -> Result<Option<i64>> {
    Ok(match v {
        V::Null => None,
        V::Num(d) => Some(
            i64::try_from(d.rescale(0, qdecimal::Rounding::Truncate).ok().and_then(|d| d.to_i128()).unwrap_or(0))
                .unwrap_or(0),
        ),
        #[allow(clippy::cast_possible_truncation)]
        V::Float(x) => Some(*x as i64),
        _ => return Err(eval_err("a number argument")),
    })
}

fn call(f: Func, args: &[Expr], row: &Row<'_>) -> Result<V> {
    let vals = args.iter().map(|a| a.eval(row)).collect::<Result<Vec<_>>>()?;
    if f == Func::Coalesce {
        return Ok(vals.into_iter().find(|v| *v != V::Null).unwrap_or(V::Null));
    }
    if vals.contains(&V::Null) {
        return Ok(V::Null);
    }
    let text = |i: usize| -> Result<Vec<u8>> {
        match vals.get(i) {
            Some(V::Text(t)) => Ok(t.clone()),
            Some(V::Num(d)) => Ok(cp037(&d.to_string())),
            _ => Err(eval_err("a string argument")),
        }
    };
    let map = |t: Vec<u8>, up: bool| -> V {
        let s = text_of(&t);
        V::Text(cp037(&if up { s.to_uppercase() } else { s.to_lowercase() }))
    };
    Ok(match f {
        Func::Upper => map(text(0)?, true),
        Func::Lower => map(text(0)?, false),
        Func::Length => V::Num(dec(i64::try_from(text(0)?.len()).expect("short"))),
        Func::Substr => {
            let t = text(0)?;
            let start = int_arg(&vals[1])?.unwrap_or(1).max(1);
            let from = usize::try_from(start - 1).unwrap_or(0).min(t.len());
            let len = match vals.get(2) {
                Some(v) => usize::try_from(int_arg(v)?.unwrap_or(0)).unwrap_or(0),
                None => t.len() - from,
            };
            let mut out: Vec<u8> = t[from..(from + len).min(t.len())].to_vec();
            out.resize(len, ccsid::SPACE);
            V::Text(out)
        }
        Func::Trim | Func::Ltrim | Func::Rtrim => {
            let t = text(0)?;
            let lo = if f == Func::Rtrim { 0 } else { t.iter().position(|&c| c != ccsid::SPACE).unwrap_or(t.len()) };
            let hi = if f == Func::Ltrim { t.len() } else { t.iter().rposition(|&c| c != ccsid::SPACE).map_or(lo, |i| i + 1) };
            V::Text(t[lo..hi.max(lo)].to_vec())
        }
        Func::Abs => match &vals[0] {
            V::Num(d) => V::Num(d.abs()),
            V::Float(x) => V::Float(x.abs()),
            _ => return Err(eval_err("ABS of a string")),
        },
        Func::Mod => match (&vals[0], &vals[1]) {
            (V::Num(_), V::Num(_)) => {
                let (a, b) = (int_arg(&vals[0])?.unwrap_or(0), int_arg(&vals[1])?.unwrap_or(0));
                if b == 0 {
                    return Err(eval_err("MOD by zero"));
                }
                V::Num(dec(a % b))
            }
            _ => return Err(eval_err("MOD needs numbers")),
        },
        Func::Coalesce => unreachable!("above"),
    })
}

fn dec(n: i64) -> Decimal {
    Decimal::new(i128::from(n), 0).expect("an integer fits")
}
