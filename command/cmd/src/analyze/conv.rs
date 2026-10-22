// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One token into a scalar value of a parameter's type: names by the
//! name rules, numbers by their length, dates and times into `CYMD`
//! and `HMS`, with `RSTD`, `VALUES`, `REL` and `RANGE` applied.

use qdecimal::Decimal;

use super::Env;
use crate::def::{Case, Parm, ParmType, RelOp};
use crate::error::Diag;
use crate::token::Tok;
use crate::value::Value;

/// The type of a CL variable, for `&VAR` values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarType {
    /// `*CHAR`.
    Char,
    /// `*DEC`.
    Dec,
    /// `*LGL`.
    Lgl,
    /// `*INT`.
    Int,
    /// `*UINT`.
    Uint,
    /// `*PTR`.
    Ptr,
}

/// A CL variable as the compiler declares it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VarInfo {
    /// Its type.
    pub ty: VarType,
    /// Its length (characters, or digits).
    pub len: u32,
    /// Decimal positions.
    pub dec: u8,
}

/// Convert `tok` for `parm`; `trusted` skips the restrictions (a
/// default or a special value's replacement).
pub fn scalar(
    env: &Env<'_>,
    parm: &Parm,
    kwd: &str,
    tok: &Tok,
    trusted: bool,
) -> Result<Value, Diag> {
    let bad = |id: &str| Diag::new(id).data(&[(kwd, 10)]).parm(kwd);
    if let Tok::Var(v) = tok {
        return variable(env, parm, kwd, v);
    }
    if parm.rtnval {
        return Err(bad("CPD0084").detail(format!("{kwd} returns a value and needs a CL variable")));
    }
    let len = parm.length();
    let value = match &parm.ty {
        ParmType::Char | ParmType::Cmd | ParmType::CmdStr | ParmType::PName | ParmType::X => {
            match tok {
                Tok::Hex(b) => {
                    if b.len() > len.len as usize {
                        return Err(bad("CPD0074").detail(format!(
                            "{kwd}: {} bytes where {} are allowed",
                            b.len(),
                            len.len
                        )));
                    }
                    Value::Hex(b.clone())
                }
                Tok::Str(s) => Value::Char(s.clone()),
                Tok::Num(n) if parm.ty == ParmType::X => Value::Num(
                    Decimal::parse(&n.replace(env.opts.point(), "."))
                        .map_err(|_| bad("CPD0084"))?,
                ),
                Tok::Word(w) | Tok::Num(w) => Value::Char(fold(parm, w)),
                Tok::Paren(items) if parm.ty == ParmType::CmdStr || parm.ty == ParmType::Char => {
                    Value::Char(Tok::join(items))
                }
                other => Value::Char(other.text()),
            }
        }
        ParmType::Name | ParmType::SName | ParmType::CName | ParmType::Generic => {
            let w = match tok {
                Tok::Word(w) => w.clone(),
                Tok::Str(s) => s.clone(),
                other => {
                    return Err(
                        bad("CPD0078").detail(format!("{kwd}: {} is not a name", other.text()))
                    );
                }
            };
            name(parm, kwd, &w, trusted)?
        }
        ParmType::VarName => {
            return Err(bad("CPD0084").detail(format!("{kwd} needs a CL variable name")));
        }
        ParmType::Dec => {
            let Tok::Num(n) = tok else {
                return Err(bad("CPD0084").detail(format!("{kwd}: {} is not a number", tok.text())));
            };
            let d =
                Decimal::parse(&n.replace(env.opts.point(), ".")).map_err(|_| bad("CPD0084"))?;
            if u32::from(d.scale()) > u32::from(len.dec) {
                return Err(bad("CPD0085")
                    .detail(format!("{kwd}: more than {} decimal positions", len.dec)));
            }
            if u32::from(d.integer_digits()) > len.len - u32::from(len.dec) {
                return Err(bad("CPD0074").detail(format!("{kwd}: more than {} digits", len.len)));
            }
            Value::Num(d)
        }
        ParmType::Int2 | ParmType::Int4 | ParmType::Uint2 | ParmType::Uint4 => {
            let Tok::Num(n) = tok else {
                return Err(bad("CPD0084").detail(format!("{kwd}: {} is not a number", tok.text())));
            };
            let d = Decimal::parse(n).map_err(|_| bad("CPD0084"))?;
            if d.scale() != 0 {
                return Err(bad("CPD0085").detail(format!("{kwd}: an integer is needed")));
            }
            let v = d.mantissa_i128().ok_or_else(|| bad("CPD0074"))?;
            let ok = match parm.ty {
                ParmType::Int2 => i16::try_from(v).is_ok(),
                ParmType::Int4 => i32::try_from(v).is_ok(),
                ParmType::Uint2 => u16::try_from(v).is_ok(),
                _ => u32::try_from(v).is_ok(),
            };
            if !ok {
                return Err(bad("CPD0074").detail(format!("{kwd}: {v} is out of range")));
            }
            Value::Num(d)
        }
        ParmType::Lgl => match tok {
            Tok::Str(s) | Tok::Num(s) | Tok::Word(s) if s == "0" || s == "1" => {
                Value::Char(s.clone())
            }
            _ => return Err(bad("CPD0084").detail(format!("{kwd}: a logical value is '0' or '1'"))),
        },
        ParmType::Date => Value::Char(date(env, kwd, tok.text().trim_matches('\''))?),
        ParmType::Time => Value::Char(time(kwd, tok.text().trim_matches('\''))?),
        ParmType::Null | ParmType::ZeroElem => Value::Null,
        ParmType::Elem { .. } | ParmType::Qual { .. } => {
            return Err(bad("CPD0084").detail(format!("{kwd}: a group needs its parts")));
        }
    };
    if trusted {
        return Ok(value);
    }
    restrictions(env, parm, kwd, &value, tok)?;
    Ok(value)
}

/// `RSTD`, `VALUES`, `REL` and `RANGE`.
fn restrictions(
    env: &Env<'_>,
    parm: &Parm,
    kwd: &str,
    value: &Value,
    tok: &Tok,
) -> Result<(), Diag> {
    let bad = |id: &str| Diag::new(id).data(&[(kwd, 10)]).parm(kwd);
    if parm.rstd {
        let text = tok.text();
        let listed = parm
            .values
            .iter()
            .any(|v| *v == text || v.trim_matches('\'') == text.trim_matches('\''))
            || parm.spcval.iter().any(|s| s.from == text)
            || parm.sngval.iter().any(|s| s.from == text);
        if !listed {
            return Err(
                bad("CPD0084").detail(format!("{kwd}: {text} is not one of the values allowed"))
            );
        }
    }
    if let Value::Num(d) = value {
        if let Some((op, lim)) = &parm.rel {
            let l = Decimal::parse(lim).map_err(|_| bad("CPD0084"))?;
            if !op.holds(d.cmp(&l)) {
                return Err(bad("CPD0094").detail(format!("{kwd}: {d} is not {} {lim}", op.text())));
            }
        }
        if let Some((lo, hi)) = &parm.range {
            let (l, h) = (
                Decimal::parse(lo).map_err(|_| bad("CPD0084"))?,
                Decimal::parse(hi).map_err(|_| bad("CPD0084"))?,
            );
            if *d < l || *d > h {
                return Err(bad("CPD0093").detail(format!("{kwd}: {d} is outside {lo} to {hi}")));
            }
        }
    } else if let Value::Char(s) = value {
        // REL and RANGE on character values compare as text.
        if let Some((op, lim)) = &parm.rel {
            let lim = lim.trim_matches('\'');
            if !op.holds(ccsid::ebcdic_cmp_str(s, lim)) {
                return Err(bad("CPD0094").detail(format!("{kwd}: {s} is not {} {lim}", op.text())));
            }
        }
        if let Some((lo, hi)) = &parm.range
            && !(Decimal::parse(lo).is_ok() && Decimal::parse(hi).is_ok())
        {
            let (lo, hi) = (lo.trim_matches('\''), hi.trim_matches('\''));
            if RelOp::Lt.holds(ccsid::ebcdic_cmp_str(s, lo))
                || RelOp::Gt.holds(ccsid::ebcdic_cmp_str(s, hi))
            {
                return Err(bad("CPD0093").detail(format!("{kwd}: {s} is outside {lo} to {hi}")));
            }
        }
    }
    let _ = env;
    let max = parm.length().len as usize;
    if let Value::Char(s) = value
        && parm.ty.is_char()
        && !matches!(
            parm.ty,
            ParmType::Date | ParmType::Time | ParmType::Lgl | ParmType::X
        )
        && s.chars().count() > max
    {
        return Err(bad("CPD0074").detail(format!(
            "{kwd}: {} characters where {max} are allowed",
            s.chars().count()
        )));
    }
    Ok(())
}

/// Case folding of an unquoted value.
fn fold(parm: &Parm, w: &str) -> String {
    match parm.case {
        Case::Mono => w.to_ascii_uppercase(),
        Case::Mixed => w.to_string(),
    }
}

/// A name of the parameter's name type.
fn name(parm: &Parm, kwd: &str, w: &str, trusted: bool) -> Result<Value, Diag> {
    let bad = || {
        Diag::new("CPD0078")
            .data(&[(kwd, 10)])
            .parm(kwd)
            .detail(format!("{kwd}: {w} is not a valid name"))
    };
    if w.starts_with('*') {
        if trusted {
            return Ok(Value::Char(w.to_string()));
        }
        return Err(Diag::new("CPD0084")
            .data(&[(kwd, 10)])
            .parm(kwd)
            .detail(format!("{kwd}: {w} is not a value allowed here")));
    }
    let max = parm.length().len as usize;
    let core = match parm.ty {
        ParmType::Generic => w.strip_suffix('*').unwrap_or(w),
        _ => w,
    };
    if core.is_empty() && parm.ty == ParmType::Generic {
        return Ok(Value::Char(w.to_string()));
    }
    if core.chars().count() > max.max(1) {
        return Err(Diag::new("CPD0074")
            .data(&[(kwd, 10)])
            .parm(kwd)
            .detail(format!("{kwd}: {w} is longer than {max}")));
    }
    let upper = core.to_ascii_uppercase();
    match parm.ty {
        ParmType::CName => {
            // Communications names: letters, digits, $ # @, no period.
            let mut c = upper.chars();
            let first = c.next().ok_or_else(bad)?;
            if !(first.is_ascii_uppercase() || "$#@".contains(first)) {
                return Err(bad());
            }
            if !c.all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || "$#@".contains(ch)) {
                return Err(bad());
            }
        }
        _ => {
            qsname::Name::parse(&upper).map_err(|_| bad())?;
            if parm.ty == ParmType::SName && upper.contains('.') {
                return Err(bad());
            }
        }
    }
    Ok(Value::Char(
        if parm.ty == ParmType::Generic && w.ends_with('*') {
            format!("{upper}*")
        } else {
            upper
        },
    ))
}

/// A `&VAR` value.
fn variable(env: &Env<'_>, parm: &Parm, kwd: &str, v: &str) -> Result<Value, Diag> {
    let bad = |id: &str| Diag::new(id).data(&[(kwd, 10)]).parm(kwd);
    let Some(vars) = env.vars else {
        return Err(bad("CPD0084").detail(format!(
            "{kwd}: &{v} is a variable; variables are for CL programs"
        )));
    };
    if parm.ty == ParmType::VarName {
        return Ok(Value::Var(v.to_string()));
    }
    if !parm.alwvar {
        return Err(bad("CPD0084").detail(format!("{kwd} does not take a variable")));
    }
    let Some(info) = vars(v) else {
        return Err(bad("CPD0727").detail(format!("variable &{v} is not declared")));
    };
    let ok = match parm.ty {
        ParmType::Dec | ParmType::Int2 | ParmType::Int4 | ParmType::Uint2 | ParmType::Uint4 => {
            matches!(info.ty, VarType::Dec | VarType::Int | VarType::Uint)
        }
        ParmType::Lgl => info.ty == VarType::Lgl,
        ParmType::X | ParmType::Null | ParmType::ZeroElem => true,
        _ => info.ty == VarType::Char,
    };
    if !ok {
        return Err(bad("CPD0084").detail(format!(
            "{kwd}: &{v} is not of the type the parameter takes"
        )));
    }
    if parm.rtnval {
        let len = parm.length();
        let fits = match parm.ty {
            ParmType::Dec => info.len == len.len && u32::from(info.dec) == u32::from(len.dec),
            // A long return area (message data, replies) takes any length.
            ParmType::Char => info.len >= len.len || len.len == 0 || len.len > 100,
            _ => true,
        };
        if !fits {
            return Err(
                bad("CPD0084").detail(format!("{kwd}: &{v} does not match the length returned"))
            );
        }
    }
    Ok(Value::Var(v.to_string()))
}

/// A date in the job's format into `CYMD`.
fn date(env: &Env<'_>, kwd: &str, text: &str) -> Result<String, Diag> {
    let bad = || {
        Diag::new("CPD0084")
            .data(&[(kwd, 10)])
            .parm(kwd)
            .detail(format!("{kwd}: {text} is not a date in the job's format"))
    };
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let fmt = env.datfmt.as_str();
    let (y, m, d) = match (fmt, digits.len()) {
        ("*ISO" | "*JIS" | "*YMD", 8) => (digits[0..4].to_string(), &digits[4..6], &digits[6..8]),
        ("*USA" | "*MDY", 8) => (digits[4..8].to_string(), &digits[0..2], &digits[2..4]),
        ("*EUR" | "*DMY", 8) => (digits[4..8].to_string(), &digits[2..4], &digits[0..2]),
        ("*MDY", 6) => (century(&digits[4..6]), &digits[0..2], &digits[2..4]),
        ("*DMY", 6) => (century(&digits[4..6]), &digits[2..4], &digits[0..2]),
        ("*YMD", 6) => (century(&digits[0..2]), &digits[2..4], &digits[4..6]),
        ("*JUL", 5) => {
            let y = century(&digits[0..2]);
            let doy: u32 = digits[2..5].parse().map_err(|_| bad())?;
            let (m, d) = from_julian(y.parse().map_err(|_| bad())?, doy).ok_or_else(bad)?;
            return cymd(&y, &format!("{m:02}"), &format!("{d:02}")).ok_or_else(bad);
        }
        _ => return Err(bad()),
    };
    cymd(&y, m, d).ok_or_else(bad)
}

/// Two-digit years 40 to 99 are 19xx, 00 to 39 are 20xx.
fn century(yy: &str) -> String {
    let n: u32 = yy.parse().unwrap_or(0);
    if n >= 40 {
        format!("19{yy}")
    } else {
        format!("20{yy}")
    }
}

/// `CYYMMDD` when the date is valid.
fn cymd(y: &str, m: &str, d: &str) -> Option<String> {
    let (yn, mn, dn): (u32, u32, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1928..=2071).contains(&yn) || !(1..=12).contains(&mn) || dn == 0 || dn > days_in(yn, mn) {
        return None;
    }
    let c = i32::from(yn >= 2000);
    Some(format!("{c}{:02}{mn:02}{dn:02}", yn % 100))
}

fn days_in(y: u32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (y.is_multiple_of(4) && !y.is_multiple_of(100)) || y.is_multiple_of(400) {
                29
            } else {
                28
            }
        }
    }
}

fn from_julian(y: u32, doy: u32) -> Option<(u32, u32)> {
    let mut left = doy;
    for m in 1..=12 {
        let n = days_in(y, m);
        if left <= n {
            return (left > 0).then_some((m, left));
        }
        left -= n;
    }
    None
}

/// `hhmmss` or `hh:mm:ss` into `HMS`.
fn time(kwd: &str, text: &str) -> Result<String, Diag> {
    let bad = || {
        Diag::new("CPD0084")
            .data(&[(kwd, 10)])
            .parm(kwd)
            .detail(format!("{kwd}: {text} is not a time"))
    };
    let digits: String = text.chars().filter(char::is_ascii_digit).collect();
    let digits = match digits.len() {
        4 => format!("{digits}00"),
        6 => digits,
        _ => return Err(bad()),
    };
    let (h, m, s): (u32, u32, u32) = (
        digits[0..2].parse().map_err(|_| bad())?,
        digits[2..4].parse().map_err(|_| bad())?,
        digits[4..6].parse().map_err(|_| bad())?,
    );
    if h > 23 || m > 59 || s > 59 {
        return Err(bad());
    }
    Ok(digits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::MemSource;

    fn env(src: &mut MemSource) -> Env<'_> {
        Env::new(src)
    }

    fn p(ty: ParmType) -> Parm {
        Parm {
            kwd: "K".into(),
            ty,
            ..Parm::default()
        }
    }

    #[test]
    fn names_and_numbers() {
        let mut s = MemSource::default();
        let e = env(&mut s);
        assert_eq!(
            scalar(&e, &p(ParmType::Name), "K", &Tok::Word("ABC".into()), false).unwrap(),
            Value::Char("ABC".into())
        );
        assert_eq!(
            scalar(&e, &p(ParmType::Name), "K", &Tok::Word("1AB".into()), false)
                .unwrap_err()
                .msgid,
            "CPD0078"
        );
        assert_eq!(
            scalar(
                &e,
                &p(ParmType::Name),
                "K",
                &Tok::Word("*ALL".into()),
                false
            )
            .unwrap_err()
            .msgid,
            "CPD0084"
        );
        assert_eq!(
            scalar(
                &e,
                &p(ParmType::Generic),
                "K",
                &Tok::Word("AB*".into()),
                false
            )
            .unwrap(),
            Value::Char("AB*".into())
        );
        let mut d = p(ParmType::Dec);
        d.len = Some(crate::def::Len { len: 5, dec: 2 });
        assert_eq!(
            scalar(&e, &d, "K", &Tok::Num("123.45".into()), false)
                .unwrap()
                .source(),
            "123.45"
        );
        assert_eq!(
            scalar(&e, &d, "K", &Tok::Num("1.234".into()), false)
                .unwrap_err()
                .msgid,
            "CPD0085"
        );
        assert_eq!(
            scalar(&e, &d, "K", &Tok::Num("1234".into()), false)
                .unwrap_err()
                .msgid,
            "CPD0074"
        );
        assert_eq!(
            scalar(
                &e,
                &p(ParmType::Int2),
                "K",
                &Tok::Num("40000".into()),
                false
            )
            .unwrap_err()
            .msgid,
            "CPD0074"
        );
        let mut c = p(ParmType::Char);
        c.len = Some(crate::def::Len::chars(3));
        assert_eq!(
            scalar(&e, &c, "K", &Tok::Str("abcd".into()), false)
                .unwrap_err()
                .msgid,
            "CPD0074"
        );
        assert_eq!(
            scalar(&e, &c, "K", &Tok::Word("ab".into()), false).unwrap(),
            Value::Char("AB".into())
        );
    }

    #[test]
    fn dates_and_times() {
        let mut s = MemSource::default();
        let mut e = env(&mut s);
        assert_eq!(
            scalar(
                &e,
                &p(ParmType::Date),
                "K",
                &Tok::Num("123126".into()),
                false
            )
            .unwrap(),
            Value::Char("1261231".into())
        );
        assert_eq!(
            scalar(
                &e,
                &p(ParmType::Date),
                "K",
                &Tok::Word("12/31/99".into()),
                false
            )
            .unwrap(),
            Value::Char("0991231".into())
        );
        e.datfmt = "*ISO".into();
        assert_eq!(
            scalar(
                &e,
                &p(ParmType::Date),
                "K",
                &Tok::Word("2026-02-29".into()),
                false
            )
            .unwrap_err()
            .msgid,
            "CPD0084"
        );
        assert_eq!(
            scalar(
                &e,
                &p(ParmType::Time),
                "K",
                &Tok::Word("13:05:09".into()),
                false
            )
            .unwrap(),
            Value::Char("130509".into())
        );
        assert_eq!(
            scalar(&e, &p(ParmType::Time), "K", &Tok::Num("2500".into()), false)
                .unwrap_err()
                .msgid,
            "CPD0084"
        );
    }

    #[test]
    fn restrictions() {
        let mut s = MemSource::default();
        let e = env(&mut s);
        let mut r = p(ParmType::Char);
        r.rstd = true;
        r.values = vec!["*YES".into(), "*NO".into()];
        assert!(scalar(&e, &r, "K", &Tok::Word("*YES".into()), false).is_ok());
        assert_eq!(
            scalar(&e, &r, "K", &Tok::Word("*MAYBE".into()), false)
                .unwrap_err()
                .msgid,
            "CPD0084"
        );
        let mut n = p(ParmType::Dec);
        n.range = Some(("1".into(), "99".into()));
        assert_eq!(
            scalar(&e, &n, "K", &Tok::Num("100".into()), false)
                .unwrap_err()
                .msgid,
            "CPD0093"
        );
        n.range = None;
        n.rel = Some((RelOp::Gt, "0".into()));
        assert_eq!(
            scalar(&e, &n, "K", &Tok::Num("0".into()), false)
                .unwrap_err()
                .msgid,
            "CPD0094"
        );
        let mut v = p(ParmType::Char);
        v.alwvar = true;
        assert_eq!(
            scalar(&e, &v, "K", &Tok::Var("X".into()), false)
                .unwrap_err()
                .msgid,
            "CPD0084"
        );
    }
}
