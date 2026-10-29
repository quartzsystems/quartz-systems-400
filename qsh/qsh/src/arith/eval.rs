// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Evaluation. `&&`, `||` and `?:` skip the side effects of the branch
//! not taken.

use super::parse::{Expr, parse};
use super::{ArithEnv, ArithError, lex};

const MAX_RECURSION: u32 = 32;

/// Evaluate an expression. Empty text is zero.
pub fn eval(expr: &str, env: &mut dyn ArithEnv) -> Result<i64, ArithError> {
    eval_depth(expr, env, 0)
}

fn eval_depth(expr: &str, env: &mut dyn ArithEnv, depth: u32) -> Result<i64, ArithError> {
    if depth > MAX_RECURSION {
        return Err(ArithError::TooDeep);
    }
    let toks = lex::tokens(expr)?;
    if toks.is_empty() {
        return Ok(0);
    }
    let e = parse(&toks, expr)?;
    let mut ev = Ev { env, depth };
    ev.eval(&e)
}

struct Ev<'a> {
    env: &'a mut dyn ArithEnv,
    depth: u32,
}

impl Ev<'_> {
    fn var(&mut self, name: &str) -> Result<i64, ArithError> {
        match self.env.get(name) {
            None => Ok(0),
            Some(v) if v.trim().is_empty() => Ok(0),
            Some(v) => {
                let t = v.trim();
                if let Ok(n) = t.parse::<i64>() {
                    return Ok(n);
                }
                eval_depth(t, self.env, self.depth + 1)
            }
        }
    }

    fn set(&mut self, name: &str, v: i64) -> Result<i64, ArithError> {
        self.env.set(name, v).map_err(ArithError::Assign)?;
        Ok(v)
    }

    fn eval(&mut self, e: &Expr) -> Result<i64, ArithError> {
        Ok(match e {
            Expr::Num(n) => *n,
            Expr::Var(name) => self.var(name)?,
            Expr::Unary(op, a) => {
                let a = self.eval(a)?;
                match *op {
                    "!" => i64::from(a == 0),
                    "~" => !a,
                    "-" => a.wrapping_neg(),
                    _ => a,
                }
            }
            Expr::Step { name, delta, post } => {
                let old = self.var(name)?;
                let new = old.wrapping_add(*delta);
                self.set(name, new)?;
                if *post { old } else { new }
            }
            Expr::Binary("&&", a, b) => i64::from(self.eval(a)? != 0 && self.eval(b)? != 0),
            Expr::Binary("||", a, b) => i64::from(self.eval(a)? != 0 || self.eval(b)? != 0),
            Expr::Binary(op, a, b) => {
                let a = self.eval(a)?;
                let b = self.eval(b)?;
                apply(op, a, b)?
            }
            Expr::Cond(c, a, b) => {
                if self.eval(c)? != 0 {
                    self.eval(a)?
                } else {
                    self.eval(b)?
                }
            }
            Expr::Assign(name, op, rhs) => {
                let r = self.eval(rhs)?;
                let v = match op {
                    None => r,
                    Some(op) => {
                        let old = self.var(name)?;
                        apply(op, old, r)?
                    }
                };
                self.set(name, v)?
            }
            Expr::Comma(a, b) => {
                self.eval(a)?;
                self.eval(b)?
            }
        })
    }
}

fn apply(op: &str, a: i64, b: i64) -> Result<i64, ArithError> {
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let shift = (b & 63) as u32;
    Ok(match op {
        "|" => a | b,
        "^" => a ^ b,
        "&" => a & b,
        "==" => i64::from(a == b),
        "!=" => i64::from(a != b),
        "<" => i64::from(a < b),
        "<=" => i64::from(a <= b),
        ">" => i64::from(a > b),
        ">=" => i64::from(a >= b),
        "<<" => a.wrapping_shl(shift),
        ">>" => a.wrapping_shr(shift),
        "+" => a.wrapping_add(b),
        "-" => a.wrapping_sub(b),
        "*" => a.wrapping_mul(b),
        "/" => {
            if b == 0 {
                return Err(ArithError::DivZero);
            }
            a.wrapping_div(b)
        }
        "%" => {
            if b == 0 {
                return Err(ArithError::DivZero);
            }
            a.wrapping_rem(b)
        }
        _ => 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Env(HashMap<String, String>);

    impl ArithEnv for Env {
        fn get(&mut self, name: &str) -> Option<String> {
            self.0.get(name).cloned()
        }
        fn set(&mut self, name: &str, value: i64) -> Result<(), String> {
            self.0.insert(name.to_string(), value.to_string());
            Ok(())
        }
    }

    fn ev(s: &str) -> Result<i64, ArithError> {
        eval(s, &mut Env(HashMap::new()))
    }

    #[test]
    fn precedence_and_bases() {
        assert_eq!(ev("1 + 2 * 3"), Ok(7));
        assert_eq!(ev("(1 + 2) * 3"), Ok(9));
        assert_eq!(ev("010 + 0x10 + 2#101"), Ok(8 + 16 + 5));
        assert_eq!(ev("1 ? 2 : 3"), Ok(2));
        assert!(ev("-2 ** 2").is_err());
        assert_eq!(ev("7 % 3 << 2"), Ok(4));
        assert_eq!(ev("1/0"), Err(ArithError::DivZero));
        assert_eq!(ev(""), Ok(0));
    }

    #[test]
    fn assignment_and_short_circuit() {
        let mut env = Env(HashMap::new());
        assert_eq!(eval("x = 5, y = x++ + 1, x", &mut env), Ok(6));
        assert_eq!(env.0["y"], "6");
        assert_eq!(eval("0 && (z = 1)", &mut env), Ok(0));
        assert!(!env.0.contains_key("z"));
        env.0.insert("e".into(), "x * 2".into());
        assert_eq!(eval("e + 1", &mut env), Ok(13));
    }
}
