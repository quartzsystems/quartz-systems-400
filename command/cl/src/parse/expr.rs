// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Expressions from tokens, with CL's precedence: `*` `/` over `+`
//! `-`, then `*CAT` `*BCAT` `*TCAT`, then the relational operators,
//! then `*NOT`, `*AND`, `*OR`. Parentheses group; `%BUILTIN(args)`
//! takes its arguments as expressions.

use cmd::token::Tok;

use crate::ast::{Expr, Lit, Op};

/// Parse `toks` as one expression.
pub fn parse(toks: &[Tok]) -> Result<Expr, String> {
    let mut p = Parser { toks, at: 0 };
    let e = p.or()?;
    if p.at != toks.len() {
        return Err(format!(
            "unexpected {} in the expression",
            toks[p.at].text()
        ));
    }
    Ok(e)
}

/// Parse a single operand (a variable, a literal, a built-in) when
/// the tokens hold one, else a whole expression.
pub fn parse_value(toks: &[Tok]) -> Result<Expr, String> {
    parse(toks)
}

struct Parser<'a> {
    toks: &'a [Tok],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }

    fn word(&self) -> Option<&str> {
        match self.peek()? {
            Tok::Word(w) => Some(w.as_str()),
            Tok::Op(o) => Some(o),
            _ => None,
        }
    }

    fn take_word(&mut self, w: &str) -> bool {
        if self.word() == Some(w) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut l = self.and()?;
        while self.take_word("*OR") {
            let r = self.and()?;
            l = Expr::Bin(Op::Or, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut l = self.not()?;
        while self.take_word("*AND") {
            let r = self.not()?;
            l = Expr::Bin(Op::And, Box::new(l), Box::new(r));
        }
        Ok(l)
    }

    fn not(&mut self) -> Result<Expr, String> {
        if self.take_word("*NOT") {
            let e = self.not()?;
            return Ok(Expr::Not(Box::new(e)));
        }
        self.rel()
    }

    fn rel(&mut self) -> Result<Expr, String> {
        let l = self.cat()?;
        let op = match self.word() {
            Some("*EQ") => Op::Eq,
            Some("*NE") => Op::Ne,
            Some("*GT") => Op::Gt,
            Some("*LT") => Op::Lt,
            Some("*GE") => Op::Ge,
            Some("*LE") => Op::Le,
            Some("*NG") => Op::Ng,
            Some("*NL") => Op::Nl,
            _ => return Ok(l),
        };
        self.at += 1;
        let r = self.cat()?;
        Ok(Expr::Bin(op, Box::new(l), Box::new(r)))
    }

    fn cat(&mut self) -> Result<Expr, String> {
        let mut l = self.add()?;
        loop {
            let op = match self.word() {
                Some("*CAT") => Op::Cat,
                Some("*BCAT") => Op::Bcat,
                Some("*TCAT") => Op::Tcat,
                _ => return Ok(l),
            };
            self.at += 1;
            let r = self.add()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn add(&mut self) -> Result<Expr, String> {
        let mut l = self.mul()?;
        loop {
            let op = match self.word() {
                Some("+") => Op::Add,
                Some("-") => Op::Sub,
                _ => return Ok(l),
            };
            self.at += 1;
            let r = self.mul()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn mul(&mut self) -> Result<Expr, String> {
        let mut l = self.unary()?;
        loop {
            let op = match self.word() {
                Some("*") => Op::Mul,
                Some("/") => Op::Div,
                _ => return Ok(l),
            };
            self.at += 1;
            let r = self.unary()?;
            l = Expr::Bin(op, Box::new(l), Box::new(r));
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        if self.take_word("-") {
            let e = self.unary()?;
            return Ok(Expr::Neg(Box::new(e)));
        }
        if self.take_word("+") {
            return self.unary();
        }
        self.operand()
    }

    fn operand(&mut self) -> Result<Expr, String> {
        let Some(t) = self.toks.get(self.at) else {
            return Err("an operand is missing".into());
        };
        self.at += 1;
        Ok(match t {
            Tok::Var(v) => Expr::Var(v.clone()),
            Tok::Str(s) => Expr::Lit(Lit::Str(s.clone())),
            Tok::Num(n) => Expr::Lit(Lit::Num(n.clone())),
            Tok::Hex(b) => Expr::Lit(Lit::Hex(b.clone())),
            Tok::Word(w) => Expr::Word(w.clone()),
            Tok::Paren(items) => parse(items)?,
            Tok::Builtin { name, args } => Expr::Builtin(name.clone(), builtin_args(name, args)?),
            Tok::Kw { name, .. } => {
                // `NAME(args)` inside an expression is a word followed by
                // a parenthesized list: not valid CL.
                return Err(format!("{name}(...) is not an operand"));
            }
            Tok::Op(o) => return Err(format!("{o} is not an operand")),
        })
    }
}

/// The arguments of a built-in: `%SST(&A 1 2)` takes three, `%TRIM(&A
/// 'x')` one or two, `%SWITCH('1X..')` one; each argument is an
/// operand, and a parenthesized one an expression.
fn builtin_args(name: &str, args: &[Tok]) -> Result<Vec<Expr>, String> {
    let mut out = Vec::new();
    for t in args {
        out.push(match t {
            Tok::Paren(items) => parse(items)?,
            other => {
                let mut p = Parser {
                    toks: std::slice::from_ref(other),
                    at: 0,
                };
                p.operand()?
            }
        });
    }
    let n = out.len();
    let ok = match name {
        "SST" | "SUBSTRING" | "BIN" | "BINARY" => n == 3 || (name.starts_with('B') && n == 1),
        "SWITCH" | "CHAR" | "INT" | "UINT" | "LEN" | "LOWER" | "UPPER" | "SIZE" | "ADDRESS"
        | "OFFSET" | "TRIM" | "TRIML" | "TRIMR" | "PARMS" => n <= 2 && (n >= 1 || name == "PARMS"),
        "DEC" => n == 3 || n == 1,
        "SCAN" => n == 2 || n == 3,
        "CHECK" | "CHECKR" => n == 2 || n == 3,
        _ => return Err(format!("%{name} is not a built-in function")),
    };
    if !ok {
        return Err(format!("%{name} takes a different number of arguments"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmd::token::{Options, tokenize_list};

    fn p(s: &str) -> Expr {
        parse(&tokenize_list(s, Options::default()).unwrap()).unwrap()
    }

    #[test]
    fn precedence() {
        let e = p("&A + 1 * 2 *CAT 'x' *EQ &B *AND *NOT &C *OR &D");
        // ((((&A + (1*2)) *CAT 'x') *EQ &B) *AND (*NOT &C)) *OR &D
        let Expr::Bin(Op::Or, l, r) = e else { panic!() };
        assert_eq!(*r, Expr::Var("D".into()));
        let Expr::Bin(Op::And, l, n) = *l else {
            panic!()
        };
        assert!(matches!(*n, Expr::Not(_)));
        let Expr::Bin(Op::Eq, l, _) = *l else {
            panic!()
        };
        let Expr::Bin(Op::Cat, l, _) = *l else {
            panic!()
        };
        let Expr::Bin(Op::Add, _, r) = *l else {
            panic!()
        };
        assert!(matches!(*r, Expr::Bin(Op::Mul, _, _)));
    }

    #[test]
    fn builtins_and_groups() {
        let e = p("%SST(&A 1 (&N + 1)) *CAT %CHAR(&X)");
        let Expr::Bin(Op::Cat, l, r) = e else {
            panic!()
        };
        assert!(matches!(*l, Expr::Builtin(ref n, ref a) if n == "SST" && a.len() == 3));
        assert!(matches!(*r, Expr::Builtin(ref n, _) if n == "CHAR"));
        assert!(parse(&tokenize_list("%NOPE(1)", Options::default()).unwrap()).is_err());
        assert!(parse(&tokenize_list("&A *EQ", Options::default()).unwrap()).is_err());
    }
}
