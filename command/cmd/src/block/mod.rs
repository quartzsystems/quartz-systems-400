// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The parameter block a command processing program receives: one
//! segment per parameter in definition order, laid out as `CMD.md` §4
//! says (fixed character fields, packed decimals, big-endian integers,
//! counted lists, concatenated qualifiers). `build` makes it from bound
//! values; `read` turns it back into [`Parms`], which every system
//! processor reads its parameters through.

use qdecimal::{Decimal, Packed, Rounding};

use crate::analyze::Bound;
use crate::def::{CmdDef, Parm, ParmType, PassVal, Vary};
use crate::error::{CmdError, Result};
use crate::value::Value;

/// One parameter's bytes; `None` is a null pointer (`PASSVAL(*NULL)`,
/// `TYPE(*NULL)`).
pub type Segment = Option<Vec<u8>>;

/// `PASSATR` byte: the value was specified.
pub const ATR_SPECIFIED: u8 = 0x80;
/// `PASSATR` byte: the default was taken.
pub const ATR_DEFAULT: u8 = 0x40;
/// `PASSATR` byte: no value (`*NULL`).
pub const ATR_NULL: u8 = 0x20;

/// A place in a segment a CL variable's value goes at run time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    /// The parameter (segment index).
    pub parm: usize,
    /// Offset in the segment.
    pub offset: usize,
    /// Bytes.
    pub len: usize,
    /// The variable.
    pub var: String,
    /// The element's definition the bytes are laid out for.
    pub ty: ParmType,
    /// Decimal positions for `*DEC`.
    pub dec: u8,
    /// `VARY`: the length prefix before the data.
    pub vary: Option<Vary>,
}

/// A built block.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Built {
    /// The segments.
    pub segments: Vec<Segment>,
    /// Where variables go.
    pub patches: Vec<Patch>,
}

/// Build the block of a resolved command.
pub fn build(def: &CmdDef, parms: &[Bound]) -> Result<Built> {
    let mut b = Built::default();
    for (i, (parm, bound)) in def.parms.iter().zip(parms).enumerate() {
        if parm.ty == ParmType::Null
            || (bound.value.is_none() && parm.passval == PassVal::Null)
            || bound.value == Some(Value::Null)
        {
            b.segments.push(None);
            continue;
        }
        let mut out = Vec::new();
        if parm.passatr {
            out.push(if bound.specified {
                ATR_SPECIFIED
            } else if bound.value.is_some() {
                ATR_DEFAULT
            } else {
                ATR_NULL
            });
        }
        let mut e = Enc {
            parm: i,
            patches: &mut b.patches,
        };
        e.value(parm, bound.value.as_ref(), &mut out)?;
        b.segments.push(Some(out));
    }
    Ok(b)
}

struct Enc<'a> {
    parm: usize,
    patches: &'a mut Vec<Patch>,
}

/// The fixed size of one scalar of a parameter's type.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // LEN is at most 24 digits
pub fn scalar_size(p: &Parm) -> usize {
    let len = p.length();
    match p.ty {
        ParmType::Dec => Packed::new(len.len as u8, len.dec).map_or(8, |x| x.byte_len()),
        ParmType::X => Packed::new(len.len as u8, len.dec)
            .map_or(1, |x| x.byte_len())
            .max(len.len as usize),
        ParmType::Int2 | ParmType::Uint2 => 2,
        ParmType::Int4 | ParmType::Uint4 => 4,
        ParmType::Lgl => 1,
        ParmType::Date => 7,
        ParmType::Time => 6,
        ParmType::Null | ParmType::ZeroElem => 0,
        ParmType::Elem { .. } | ParmType::Qual { .. } => 0,
        _ => {
            let prefix = match p.vary {
                Some(Vary::Int2) => 2,
                Some(Vary::Int4) => 4,
                None => 0,
            };
            prefix + len.len as usize
        }
    }
}

impl Enc<'_> {
    /// Encode a parameter, element or qualifier value.
    fn value(&mut self, p: &Parm, v: Option<&Value>, out: &mut Vec<u8>) -> Result<()> {
        if p.is_list() {
            return self.list(p, v, out);
        }
        match &p.ty {
            ParmType::Elem { .. } => self.group(p, v, out),
            ParmType::Qual { .. } => self.qual(p, v, out),
            _ => self.scalar(p, v, out),
        }
    }

    /// A list: a two-byte count, then the elements; a list of groups
    /// adds two-byte offsets to each group after the count.
    fn list(&mut self, p: &Parm, v: Option<&Value>, out: &mut Vec<u8>) -> Result<()> {
        let items: Vec<&Value> = match v {
            None => Vec::new(),
            Some(Value::List(items)) => items.iter().collect(),
            // A single value stands for a one-element list.
            Some(other) => vec![other],
        };
        let n = u16::try_from(items.len()).map_err(|_| CmdError::escape("CPF0006"))?;
        out.extend_from_slice(&n.to_be_bytes());
        if matches!(p.ty, ParmType::Elem { .. }) {
            let base = out.len() - 2;
            let table = out.len();
            out.resize(table + 2 * items.len(), 0);
            for (i, item) in items.iter().enumerate() {
                let off =
                    u16::try_from(out.len() - base).map_err(|_| CmdError::escape("CPF0006"))?;
                out[table + 2 * i..table + 2 * i + 2].copy_from_slice(&off.to_be_bytes());
                self.group(p, Some(item), out)?;
            }
            return Ok(());
        }
        for item in items {
            match &p.ty {
                ParmType::Qual { .. } => self.qual(p, Some(item), out)?,
                _ => self.scalar(p, Some(item), out)?,
            }
        }
        Ok(())
    }

    /// A group: a two-byte element count, then each element as its own
    /// type says.
    fn group(&mut self, p: &Parm, v: Option<&Value>, out: &mut Vec<u8>) -> Result<()> {
        let items: Vec<Option<&Value>> = match v {
            None => Vec::new(),
            Some(Value::Elems(items)) => items.iter().map(Option::as_ref).collect(),
            Some(other) => vec![Some(other)],
        };
        if items.is_empty() {
            out.extend_from_slice(&0u16.to_be_bytes());
            return Ok(());
        }
        let n = u16::try_from(p.items.len()).map_err(|_| CmdError::escape("CPF0006"))?;
        out.extend_from_slice(&n.to_be_bytes());
        for (i, item) in p.items.iter().enumerate() {
            self.value(item, items.get(i).copied().flatten(), out)?;
        }
        Ok(())
    }

    /// A qualified name: the parts, each a fixed field.
    fn qual(&mut self, p: &Parm, v: Option<&Value>, out: &mut Vec<u8>) -> Result<()> {
        let parts: Vec<Option<&Value>> = match v {
            None => Vec::new(),
            Some(Value::Qual(parts)) => parts.iter().map(Some).collect(),
            Some(other) => vec![Some(other)],
        };
        for (i, q) in p.items.iter().enumerate() {
            self.scalar(q, parts.get(i).copied().flatten(), out)?;
        }
        Ok(())
    }

    /// One scalar.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // the analyzer checked the ranges
    fn scalar(&mut self, p: &Parm, v: Option<&Value>, out: &mut Vec<u8>) -> Result<()> {
        let len = p.length();
        let size = scalar_size(p);
        let start = out.len();
        match &p.ty {
            ParmType::Dec => {
                let packed = Packed::new(len.len as u8, len.dec)
                    .map_err(|e| CmdError::detail("CPF0006", e.to_string()))?;
                let d = match v {
                    Some(Value::Num(d)) => d.clone(),
                    Some(Value::Var(name)) => {
                        self.patch(start, size, name, p);
                        Decimal::zero()
                    }
                    _ => Decimal::zero(),
                };
                let b = packed
                    .encode(&d, Rounding::Truncate)
                    .map_err(|e| CmdError::detail("CPF0006", e.to_string()))?;
                out.extend_from_slice(&b);
            }
            ParmType::Int2 | ParmType::Int4 | ParmType::Uint2 | ParmType::Uint4 => {
                let n = match v {
                    Some(Value::Num(d)) => d.mantissa_i128().unwrap_or(0),
                    Some(Value::Var(name)) => {
                        self.patch(start, size, name, p);
                        0
                    }
                    _ => 0,
                };
                match p.ty {
                    ParmType::Int2 => out.extend_from_slice(&(n as i16).to_be_bytes()),
                    ParmType::Uint2 => out.extend_from_slice(&(n as u16).to_be_bytes()),
                    ParmType::Int4 => out.extend_from_slice(&(n as i32).to_be_bytes()),
                    _ => out.extend_from_slice(&(n as u32).to_be_bytes()),
                }
            }
            ParmType::Null | ParmType::ZeroElem => {}
            ParmType::Lgl => {
                let c = match v {
                    Some(Value::Char(s)) if s == "1" => 0xF1,
                    Some(Value::Var(name)) => {
                        self.patch(start, 1, name, p);
                        0xF0
                    }
                    _ => 0xF0,
                };
                out.push(c);
            }
            ParmType::X => {
                // Passed as text; a number as its digits.
                let text = match v {
                    Some(Value::Num(d)) => d.to_string(),
                    Some(Value::Char(s)) => s.clone(),
                    _ => String::new(),
                };
                let mut b = ccsid::to_ebcdic(&text).unwrap_or_default();
                b.resize(size, 0x40);
                out.extend_from_slice(&b);
            }
            _ => {
                let bytes: Vec<u8> = match v {
                    Some(Value::Char(s)) => {
                        ccsid::to_ebcdic(s).unwrap_or_else(|_| vec![0x40; s.len()])
                    }
                    Some(Value::Hex(b)) => b.clone(),
                    Some(Value::Num(d)) => ccsid::to_ebcdic(&d.to_string()).unwrap_or_default(),
                    Some(Value::Var(name)) => {
                        self.patch(start, size, name, p);
                        Vec::new()
                    }
                    Some(Value::Expr(_)) => Vec::new(),
                    Some(Value::Qual(parts)) => {
                        let text = Value::Qual(parts.clone()).source();
                        ccsid::to_ebcdic(&text).unwrap_or_default()
                    }
                    Some(Value::List(items)) => {
                        let text = Value::List(items.clone()).source();
                        ccsid::to_ebcdic(&text).unwrap_or_default()
                    }
                    _ => Vec::new(),
                };
                let data_len = len.len as usize;
                let n = bytes.len().min(data_len);
                match p.vary {
                    Some(Vary::Int2) => out.extend_from_slice(&(n as u16).to_be_bytes()),
                    Some(Vary::Int4) => out.extend_from_slice(&(n as u32).to_be_bytes()),
                    None => {}
                }
                out.extend_from_slice(&bytes[..n]);
                out.resize(start + size, 0x40);
            }
        }
        Ok(())
    }

    fn patch(&mut self, offset: usize, len: usize, var: &str, p: &Parm) {
        self.patches.push(Patch {
            parm: self.parm,
            offset,
            len,
            var: var.to_string(),
            ty: p.ty.clone(),
            dec: p.length().dec,
            vary: p.vary,
        });
    }
}

/// The parameters a processing program reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parms {
    /// The definition.
    pub def: CmdDef,
    /// The values, in definition order.
    pub values: Vec<Bound>,
}

/// Decode a block back into values.
pub fn read(def: &CmdDef, segments: &[Segment], specified: &[bool]) -> Result<Parms> {
    let mut values = Vec::with_capacity(def.parms.len());
    for (i, p) in def.parms.iter().enumerate() {
        let seg = segments.get(i).cloned().flatten();
        let spec = specified.get(i).copied().unwrap_or(false);
        let value = match seg {
            None => None,
            Some(b) => {
                let mut at = 0;
                let mut specified_atr = spec;
                if p.passatr {
                    let a = *b.first().unwrap_or(&0);
                    specified_atr = a & ATR_SPECIFIED != 0;
                    at = 1;
                    if a & ATR_NULL != 0 {
                        values.push(Bound {
                            kwd: p.kwd.clone(),
                            value: None,
                            specified: false,
                            sel: None,
                        });
                        continue;
                    }
                }
                let v = decode(p, &b, &mut at)?;
                values.push(Bound {
                    kwd: p.kwd.clone(),
                    value: Some(v),
                    specified: specified_atr,
                    sel: None,
                });
                continue;
            }
        };
        values.push(Bound {
            kwd: p.kwd.clone(),
            value,
            specified: spec,
            sel: None,
        });
    }
    Ok(Parms {
        def: def.clone(),
        values,
    })
}

fn take<'a>(b: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8]> {
    let s = b
        .get(*at..*at + n)
        .ok_or_else(|| CmdError::detail("CPF0006", "parameter block truncated"))?;
    *at += n;
    Ok(s)
}

fn count(b: &[u8], at: &mut usize) -> Result<usize> {
    let c = take(b, at, 2)?;
    Ok(usize::from(u16::from_be_bytes([c[0], c[1]])))
}

fn decode(p: &Parm, b: &[u8], at: &mut usize) -> Result<Value> {
    if p.is_list() {
        let n = count(b, at)?;
        let mut items = Vec::with_capacity(n);
        if matches!(p.ty, ParmType::Elem { .. }) {
            let base = *at - 2;
            let offs = take(b, at, 2 * n)?.to_vec();
            for i in 0..n {
                let off = usize::from(u16::from_be_bytes([offs[2 * i], offs[2 * i + 1]]));
                let mut g = base + off;
                items.push(decode_group(p, b, &mut g)?);
                *at = g;
            }
        } else {
            for _ in 0..n {
                items.push(match &p.ty {
                    ParmType::Qual { .. } => decode_qual(p, b, at)?,
                    _ => decode_scalar(p, b, at)?,
                });
            }
        }
        return Ok(Value::List(items));
    }
    match &p.ty {
        ParmType::Elem { .. } => decode_group(p, b, at),
        ParmType::Qual { .. } => decode_qual(p, b, at),
        _ => decode_scalar(p, b, at),
    }
}

fn decode_group(p: &Parm, b: &[u8], at: &mut usize) -> Result<Value> {
    let n = count(b, at)?;
    let mut items = Vec::with_capacity(n);
    for item in p.items.iter().take(n) {
        items.push(Some(decode(item, b, at)?));
    }
    Ok(Value::Elems(items))
}

fn decode_qual(p: &Parm, b: &[u8], at: &mut usize) -> Result<Value> {
    let mut parts = Vec::with_capacity(p.items.len());
    for q in &p.items {
        parts.push(decode_scalar(q, b, at)?);
    }
    Ok(Value::Qual(parts))
}

#[allow(clippy::cast_possible_truncation)] // LEN is at most 24 digits
fn decode_scalar(p: &Parm, b: &[u8], at: &mut usize) -> Result<Value> {
    let len = p.length();
    let size = scalar_size(p);
    Ok(match &p.ty {
        ParmType::Dec => {
            let packed = Packed::new(len.len as u8, len.dec)
                .map_err(|e| CmdError::detail("CPF0006", e.to_string()))?;
            let d = packed
                .decode(take(b, at, size)?)
                .map_err(|e| CmdError::detail("CPF0006", e.to_string()))?;
            Value::Num(d)
        }
        ParmType::Int2 => Value::Num(
            Decimal::new(
                i128::from(i16::from_be_bytes(take(b, at, 2)?.try_into().expect("2"))),
                0,
            )
            .expect("fits"),
        ),
        ParmType::Uint2 => Value::Num(
            Decimal::new(
                i128::from(u16::from_be_bytes(take(b, at, 2)?.try_into().expect("2"))),
                0,
            )
            .expect("fits"),
        ),
        ParmType::Int4 => Value::Num(
            Decimal::new(
                i128::from(i32::from_be_bytes(take(b, at, 4)?.try_into().expect("4"))),
                0,
            )
            .expect("fits"),
        ),
        ParmType::Uint4 => Value::Num(
            Decimal::new(
                i128::from(u32::from_be_bytes(take(b, at, 4)?.try_into().expect("4"))),
                0,
            )
            .expect("fits"),
        ),
        ParmType::Null | ParmType::ZeroElem => Value::Null,
        ParmType::Lgl => Value::Char(if take(b, at, 1)?[0] == 0xF1 { "1" } else { "0" }.into()),
        ParmType::X => Value::Char(ccsid::decode_trimmed(take(b, at, size)?)),
        _ => {
            let data = match p.vary {
                Some(Vary::Int2) => {
                    let n = count(b, at)?;
                    let d = take(b, at, len.len as usize)?;
                    &d[..n.min(d.len())]
                }
                Some(Vary::Int4) => {
                    let c = take(b, at, 4)?;
                    let n = u32::from_be_bytes(c.try_into().expect("4")) as usize;
                    let d = take(b, at, len.len as usize)?;
                    &d[..n.min(d.len())]
                }
                None => take(b, at, size)?,
            };
            Value::Char(ccsid::from_ebcdic(data).trim_end().to_string())
        }
    })
}

impl Parms {
    /// From bound values, without a block.
    #[must_use]
    pub fn from_bound(def: &CmdDef, values: Vec<Bound>) -> Self {
        Self {
            def: def.clone(),
            values,
        }
    }

    /// A parameter's value.
    #[must_use]
    pub fn get(&self, kwd: &str) -> Option<&Value> {
        self.values
            .iter()
            .find(|b| b.kwd == kwd)
            .and_then(|b| b.value.as_ref())
    }

    /// Whether the caller gave the parameter.
    #[must_use]
    pub fn specified(&self, kwd: &str) -> bool {
        self.values.iter().any(|b| b.kwd == kwd && b.specified)
    }

    /// Character text, trailing blanks trimmed; empty when absent.
    #[must_use]
    pub fn text(&self, kwd: &str) -> String {
        match self.get(kwd) {
            Some(Value::Char(s)) => s.trim_end().to_string(),
            Some(Value::Num(d)) => d.to_string(),
            Some(Value::Hex(b)) => ccsid::from_ebcdic(b),
            Some(other) => other.source(),
            None => String::new(),
        }
    }

    /// A name, upper case; empty when absent.
    #[must_use]
    pub fn name(&self, kwd: &str) -> String {
        self.text(kwd).to_ascii_uppercase()
    }

    /// `*YES`.
    #[must_use]
    pub fn yes(&self, kwd: &str) -> bool {
        self.name(kwd) == "*YES"
    }

    /// A qualified name: `(name, library)`; a single value gives an
    /// empty library (`*LIBL` is the caller's to supply).
    #[must_use]
    pub fn qual(&self, kwd: &str) -> (String, String) {
        match self.get(kwd) {
            Some(Value::Qual(parts)) => (
                parts
                    .first()
                    .map(super::value::Value::source)
                    .unwrap_or_default(),
                parts
                    .get(1)
                    .map(super::value::Value::source)
                    .unwrap_or_default(),
            ),
            Some(Value::Char(s)) => match s.split_once('/') {
                Some((l, n)) => (n.into(), l.into()),
                None => (s.clone(), String::new()),
            },
            Some(other) => (other.source(), String::new()),
            None => (String::new(), String::new()),
        }
    }

    /// A number.
    #[must_use]
    pub fn num(&self, kwd: &str) -> Option<Decimal> {
        match self.get(kwd) {
            Some(Value::Num(d)) => Some(d.clone()),
            Some(Value::Char(s)) => Decimal::parse(s).ok(),
            _ => None,
        }
    }

    /// An integer.
    #[must_use]
    pub fn int(&self, kwd: &str) -> Option<i64> {
        self.num(kwd)
            .and_then(|d| d.mantissa_i128())
            .and_then(|m| i64::try_from(m).ok())
    }

    /// The items of a list (one item for a single value).
    #[must_use]
    pub fn list(&self, kwd: &str) -> Vec<&Value> {
        self.get(kwd).map(Value::items).unwrap_or_default()
    }

    /// The elements of a group.
    #[must_use]
    pub fn elems(&self, kwd: &str) -> Vec<Option<&Value>> {
        match self.get(kwd) {
            Some(Value::Elems(items)) => items.iter().map(Option::as_ref).collect(),
            Some(other) => vec![Some(other)],
            None => Vec::new(),
        }
    }

    /// The command in keyword form, every parameter with a value, as
    /// `word KWD(value) ...`.
    #[must_use]
    pub fn to_line(&self, word: &str) -> String {
        let mut s = word.to_string();
        for b in &self.values {
            if let Some(v) = &b.value {
                s.push(' ');
                s.push_str(&b.kwd);
                s.push('(');
                s.push_str(&v.parm_source());
                s.push(')');
            }
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::{Env, MemSource, analyze};

    const DEF: &str = "\
 CMD PROMPT('Test')
 PARM KWD(LIB) TYPE(*NAME) LEN(10) MIN(1)
 PARM KWD(SIZE) TYPE(*DEC) LEN(7 2) DFT(10.5)
 PARM KWD(N) TYPE(*INT2) DFT(-3)
 PARM KWD(FLAG) TYPE(*LGL) DFT('1')
 PARM KWD(FILE) TYPE(Q1) DFT(*NONE) SNGVAL((*NONE))
 Q1: QUAL TYPE(*NAME) LEN(10)
 QUAL TYPE(*NAME) LEN(10) DFT(*LIBL) SPCVAL((*LIBL) (*CURLIB))
 PARM KWD(LIBL) TYPE(*NAME) LEN(10) MAX(5)
 PARM KWD(LOG) TYPE(E1)
 E1: ELEM TYPE(*INT2) DFT(4)
 ELEM TYPE(*CHAR) LEN(7) DFT(*NOLIST)
 PARM KWD(POOLS) TYPE(E2) MAX(3)
 E2: ELEM TYPE(*INT2)
 ELEM TYPE(*CHAR) LEN(10)
 PARM KWD(V) TYPE(*CHAR) LEN(20) VARY(*YES) PASSATR(*YES)
";

    #[test]
    fn build_and_read() {
        let mut src = MemSource::default();
        src.add("T", DEF).unwrap();
        let mut env = Env::new(&mut src);
        let r = analyze(
            &mut env,
            "T LIB(QGPL) FILE(QGPL/F) LIBL(A B) LOG(2 *MSG) POOLS((1 *BASE) (2 X)) V('ab')",
        )
        .unwrap();
        let b = build(&r.def, &r.parms).unwrap();
        assert_eq!(b.segments.len(), 9);
        assert_eq!(
            b.segments[0].as_ref().unwrap(),
            &ccsid::to_ebcdic("QGPL      ").unwrap()
        );
        let size = b.segments[1].as_ref().unwrap();
        assert_eq!(size, &[0x00, 0x01, 0x05, 0x0C]);
        assert_eq!(b.segments[2].as_ref().unwrap(), &(-3i16).to_be_bytes());
        assert_eq!(b.segments[3].as_ref().unwrap(), &[0xF1]);
        assert_eq!(b.segments[4].as_ref().unwrap().len(), 20);
        let libl = b.segments[5].as_ref().unwrap();
        assert_eq!(&libl[..2], &[0, 2]);
        assert_eq!(libl.len(), 22);
        let log = b.segments[6].as_ref().unwrap();
        assert_eq!(&log[..4], &[0, 2, 0, 2]);
        let pools = b.segments[7].as_ref().unwrap();
        assert_eq!(&pools[..6], &[0, 2, 0, 6, 0, 20]);
        let v = b.segments[8].as_ref().unwrap();
        assert_eq!(v[0], ATR_SPECIFIED);
        assert_eq!(&v[1..3], &[0, 2]);
        assert_eq!(v.len(), 23);
        let spec: Vec<bool> = r.parms.iter().map(|p| p.specified).collect();
        let p = read(&r.def, &b.segments, &spec).unwrap();
        assert_eq!(p.name("LIB"), "QGPL");
        assert_eq!(p.num("SIZE").unwrap().to_string(), "10.50");
        assert_eq!(p.int("N"), Some(-3));
        assert_eq!(p.text("FLAG"), "1");
        assert_eq!(p.qual("FILE"), ("F".into(), "QGPL".into()));
        assert_eq!(p.list("LIBL").len(), 2);
        let log = p.elems("LOG");
        assert_eq!(log[0].unwrap().source(), "2");
        assert_eq!(log[1].unwrap().source(), "*MSG");
        let pools = p.list("POOLS");
        assert_eq!(pools[1].source(), "(2 X)");
        assert_eq!(p.text("V"), "ab");
        assert!(p.specified("V") && !p.specified("SIZE"));
        assert_eq!(
            p.to_line("t"),
            "t LIB(QGPL) SIZE(10.50) N(-3) FLAG(1) FILE(QGPL/F) LIBL(A B) LOG(2 *MSG) POOLS((1 *BASE) (2 X)) V('ab')"
        );
    }

    #[test]
    fn omitted_and_single_values() {
        let mut src = MemSource::default();
        src.add("T", DEF).unwrap();
        let mut env = Env::new(&mut src);
        let r = analyze(&mut env, "T LIB(X)").unwrap();
        let b = build(&r.def, &r.parms).unwrap();
        let spec: Vec<bool> = r.parms.iter().map(|p| p.specified).collect();
        let p = read(&r.def, &b.segments, &spec).unwrap();
        assert_eq!(p.text("FILE"), "*NONE");
        assert!(p.list("LIBL").is_empty());
        assert_eq!(p.elems("LOG")[1].unwrap().source(), "*NOLIST");
        assert_eq!(b.segments[8].as_ref().unwrap()[0], ATR_NULL);
    }
}
