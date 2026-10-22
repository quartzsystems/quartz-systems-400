// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Constant expressions in parameter values (`EXPR(*YES)`): string
//! concatenation and arithmetic over literals. Anything with a
//! variable or a built-in function is the CL compiler's to evaluate.

use qdecimal::Decimal;

use super::Env;
use crate::token::Tok;

/// Why an expression could not be reduced to one literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unevaluated {
    /// It names variables or built-in functions.
    Variables,
    /// It is not a well-formed expression.
    Bad(String),
}

/// Reduce `toks` to one literal token.
pub fn eval(toks: &[Tok], env: &Env<'_>) -> Result<Tok, Unevaluated> {
    if has_vars(toks) {
        return Err(Unevaluated::Variables);
    }
    let mut p = Parser {
        toks,
        at: 0,
        point: env.opts.point(),
    };
    let v = p.expr()?;
    if p.at != toks.len() {
        return Err(Unevaluated::Bad(format!(
            "unexpected {}",
            toks[p.at].text()
        )));
    }
    Ok(match v {
        Val::Str(s) => Tok::Str(s),
        Val::Num(d) => Tok::Num(d.to_string()),
    })
}

/// Whether tokens mention a variable or a built-in anywhere.
pub fn has_vars(toks: &[Tok]) -> bool {
    toks.iter().any(|t| match t {
        Tok::Var(_) | Tok::Builtin { .. } => true,
        Tok::Paren(items) | Tok::Kw { args: items, .. } => has_vars(items),
        _ => false,
    })
}

enum Val {
    Str(String),
    Num(Decimal),
}

struct Parser<'a> {
    toks: &'a [Tok],
    at: usize,
    point: char,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn op(&self) -> Option<&'static str> {
        match self.peek()? {
            Tok::Op(o) => Some(o),
            Tok::Word(w) => match w.as_str() {
                "*CAT" => Some("*CAT"),
                "*BCAT" => Some("*BCAT"),
                "*TCAT" => Some("*TCAT"),
                _ => None,
            },
            _ => None,
        }
    }

    fn expr(&mut self) -> Result<Val, Unevaluated> {
        let mut left = self.term()?;
        while let Some(op) = self.op() {
            if !matches!(op, "+" | "-" | "*CAT" | "*BCAT" | "*TCAT") {
                break;
            }
            self.at += 1;
            let right = self.term()?;
            left = apply(op, left, right)?;
        }
        Ok(left)
    }

    fn term(&mut self) -> Result<Val, Unevaluated> {
        let mut left = self.factor()?;
        while let Some(op) = self.op() {
            if !matches!(op, "*" | "/") {
                break;
            }
            self.at += 1;
            let right = self.factor()?;
            left = apply(op, left, right)?;
        }
        Ok(left)
    }

    fn factor(&mut self) -> Result<Val, Unevaluated> {
        let Some(t) = self.toks.get(self.at) else {
            return Err(Unevaluated::Bad("an operand is missing".into()));
        };
        self.at += 1;
        match t {
            Tok::Num(n) => Decimal::parse(&n.replace(self.point, "."))
                .map(Val::Num)
                .map_err(|_| Unevaluated::Bad(format!("{n} is not a number"))),
            Tok::Str(s) => Ok(Val::Str(s.clone())),
            Tok::Hex(b) => Ok(Val::Str(ccsid::from_ebcdic(b))),
            Tok::Word(w) if !w.starts_with('*') => Ok(Val::Str(w.clone())),
            Tok::Op("-") => match self.factor()? {
                Val::Num(d) => {
                    let m = d
                        .mantissa_i128()
                        .ok_or_else(|| Unevaluated::Bad("too large".into()))?;
                    Decimal::new(-m, d.scale())
                        .map(Val::Num)
                        .map_err(|e| Unevaluated::Bad(e.to_string()))
                }
                Val::Str(_) => Err(Unevaluated::Bad("a sign before text".into())),
            },
            Tok::Paren(items) => {
                let mut inner = Parser {
                    toks: items,
                    at: 0,
                    point: self.point,
                };
                let v = inner.expr()?;
                if inner.at != items.len() {
                    return Err(Unevaluated::Bad("unexpected text in parentheses".into()));
                }
                Ok(v)
            }
            other => Err(Unevaluated::Bad(format!(
                "{} cannot be an operand",
                other.text()
            ))),
        }
    }
}

fn apply(op: &str, l: Val, r: Val) -> Result<Val, Unevaluated> {
    match (op, l, r) {
        ("*CAT", Val::Str(a), Val::Str(b)) => Ok(Val::Str(a + &b)),
        ("*BCAT", Val::Str(a), Val::Str(b)) => Ok(Val::Str(format!("{} {b}", a.trim_end()))),
        ("*TCAT", Val::Str(a), Val::Str(b)) => Ok(Val::Str(format!("{}{b}", a.trim_end()))),
        ("+" | "-" | "*" | "/", Val::Num(a), Val::Num(b)) => arith(op, &a, &b),
        (op, _, _) => Err(Unevaluated::Bad(format!(
            "{op} does not apply to these operands"
        ))),
    }
}

fn arith(op: &str, a: &Decimal, b: &Decimal) -> Result<Val, Unevaluated> {
    let bad = |e: qdecimal::DecimalError| Unevaluated::Bad(e.to_string());
    let (am, bm) = (
        a.mantissa_i128()
            .ok_or_else(|| Unevaluated::Bad("too large".into()))?,
        b.mantissa_i128()
            .ok_or_else(|| Unevaluated::Bad("too large".into()))?,
    );
    let scale = a.scale().max(b.scale());
    let up = |m: i128, s: u8| m * 10i128.pow(u32::from(scale - s));
    let v = match op {
        "+" => Decimal::new(up(am, a.scale()) + up(bm, b.scale()), scale),
        "-" => Decimal::new(up(am, a.scale()) - up(bm, b.scale()), scale),
        "*" => Decimal::new(am * bm, a.scale() + b.scale()),
        _ => {
            if bm == 0 {
                return Err(Unevaluated::Bad("division by zero".into()));
            }
            // Keep the scale of the dividend's and divisor's sum, as a
            // quotient of literals is read.
            let s = a.scale().max(b.scale()).max(5);
            let num = am * 10i128.pow(u32::from(s) + u32::from(b.scale()));
            Decimal::new(num / bm / 10i128.pow(u32::from(a.scale())), s)
        }
    }
    .map_err(bad)?;
    Ok(Val::Num(v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analyze::MemSource;
    use crate::token::{Options, tokenize_list};

    fn ev(s: &str) -> Result<Tok, Unevaluated> {
        let mut src = MemSource::default();
        let env = Env::new(&mut src);
        eval(&tokenize_list(s, Options::default()).unwrap(), &env)
    }

    #[test]
    fn strings_and_numbers() {
        assert_eq!(ev("'a' *CAT 'b'").unwrap(), Tok::Str("ab".into()));
        assert_eq!(
            ev("'a  ' *BCAT 'b' || 'c'").unwrap(),
            Tok::Str("a bc".into())
        );
        assert_eq!(ev("'a  ' *TCAT 'b'").unwrap(), Tok::Str("ab".into()));
        assert_eq!(ev("1 + 2 * 3").unwrap(), Tok::Num("7".into()));
        assert_eq!(ev("(1 + 2) * 3").unwrap(), Tok::Num("9".into()));
        assert_eq!(ev("1.5 - 0.25").unwrap(), Tok::Num("1.25".into()));
        assert_eq!(ev("7 / 2").unwrap(), Tok::Num("3.50000".into()));
        assert_eq!(ev("- 3").unwrap(), Tok::Num("-3".into()));
        assert_eq!(ev("&A *CAT 'x'").unwrap_err(), Unevaluated::Variables);
        assert!(matches!(ev("1 + 'a'").unwrap_err(), Unevaluated::Bad(_)));
        assert!(matches!(ev("1 /  0").unwrap_err(), Unevaluated::Bad(_)));
    }
}
