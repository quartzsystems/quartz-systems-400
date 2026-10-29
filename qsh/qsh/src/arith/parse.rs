// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Arithmetic expressions by precedence climbing.

use super::ArithError;
use super::lex::Tok;

/// An expression.
#[derive(Clone, Debug)]
pub(super) enum Expr {
    Num(i64),
    Var(String),
    Unary(&'static str, Box<Expr>),
    /// `++x` / `--x` (`post` false) or `x++` / `x--`.
    Step {
        name: String,
        delta: i64,
        post: bool,
    },
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `x = e`, `x op= e` (the operator without `=`).
    Assign(String, Option<&'static str>, Box<Expr>),
    Comma(Box<Expr>, Box<Expr>),
}

const MAX_DEPTH: u32 = 200;

pub(super) struct P<'a> {
    toks: &'a [Tok],
    i: usize,
    src: &'a str,
    depth: u32,
}

/// Parse a whole token list.
pub(super) fn parse(toks: &[Tok], src: &str) -> Result<Expr, ArithError> {
    let mut p = P {
        toks,
        i: 0,
        src,
        depth: 0,
    };
    let e = p.comma()?;
    if p.i != toks.len() {
        return Err(p.syntax());
    }
    Ok(e)
}

fn binary_prec(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 1,
        "&&" => 2,
        "|" => 3,
        "^" => 4,
        "&" => 5,
        "==" | "!=" => 6,
        "<" | "<=" | ">" | ">=" => 7,
        "<<" | ">>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "%" => 10,
        _ => return None,
    })
}

impl P<'_> {
    fn syntax(&self) -> ArithError {
        ArithError::Syntax(self.src.trim().to_string())
    }

    fn peek_op(&self) -> Option<&'static str> {
        match self.toks.get(self.i) {
            Some(Tok::Op(o)) => Some(o),
            _ => None,
        }
    }

    fn eat(&mut self, op: &str) -> bool {
        if self.peek_op() == Some(op) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn comma(&mut self) -> Result<Expr, ArithError> {
        let mut e = self.assign()?;
        while self.eat(",") {
            let r = self.assign()?;
            e = Expr::Comma(Box::new(e), Box::new(r));
        }
        Ok(e)
    }

    fn assign(&mut self) -> Result<Expr, ArithError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ArithError::TooDeep);
        }
        let r = self.assign_inner();
        self.depth -= 1;
        r
    }

    fn assign_inner(&mut self) -> Result<Expr, ArithError> {
        if let (Some(Tok::Ident(name)), Some(Tok::Op(op))) =
            (self.toks.get(self.i), self.toks.get(self.i + 1))
        {
            let compound = match *op {
                "=" => Some(None),
                "+=" => Some(Some("+")),
                "-=" => Some(Some("-")),
                "*=" => Some(Some("*")),
                "/=" => Some(Some("/")),
                "%=" => Some(Some("%")),
                "<<=" => Some(Some("<<")),
                ">>=" => Some(Some(">>")),
                "&=" => Some(Some("&")),
                "^=" => Some(Some("^")),
                "|=" => Some(Some("|")),
                _ => None,
            };
            if let Some(c) = compound {
                let name = name.clone();
                self.i += 2;
                let rhs = self.assign()?;
                return Ok(Expr::Assign(name, c, Box::new(rhs)));
            }
        }
        self.cond()
    }

    fn cond(&mut self) -> Result<Expr, ArithError> {
        let c = self.binary(1)?;
        if self.eat("?") {
            let a = self.comma()?;
            if !self.eat(":") {
                return Err(self.syntax());
            }
            let b = self.assign()?;
            return Ok(Expr::Cond(Box::new(c), Box::new(a), Box::new(b)));
        }
        Ok(c)
    }

    fn binary(&mut self, min: u8) -> Result<Expr, ArithError> {
        let mut lhs = self.unary()?;
        while let Some(op) = self.peek_op() {
            let Some(prec) = binary_prec(op) else { break };
            if prec < min {
                break;
            }
            self.i += 1;
            let rhs = self.binary(prec + 1)?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<Expr, ArithError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(ArithError::TooDeep);
        }
        let r = self.unary_inner();
        self.depth -= 1;
        r
    }

    fn unary_inner(&mut self) -> Result<Expr, ArithError> {
        match self.peek_op() {
            Some(op @ ("++" | "--")) => {
                self.i += 1;
                let Some(Tok::Ident(name)) = self.toks.get(self.i) else {
                    return Err(self.syntax());
                };
                let name = name.clone();
                self.i += 1;
                Ok(Expr::Step {
                    name,
                    delta: if op == "++" { 1 } else { -1 },
                    post: false,
                })
            }
            Some(op @ ("!" | "~" | "+" | "-")) => {
                self.i += 1;
                let e = self.unary()?;
                Ok(Expr::Unary(op, Box::new(e)))
            }
            _ => self.postfix(),
        }
    }

    fn postfix(&mut self) -> Result<Expr, ArithError> {
        match self.toks.get(self.i).cloned() {
            Some(Tok::Num(n)) => {
                self.i += 1;
                Ok(Expr::Num(n))
            }
            Some(Tok::Ident(name)) => {
                self.i += 1;
                match self.peek_op() {
                    Some(op @ ("++" | "--")) => {
                        self.i += 1;
                        Ok(Expr::Step {
                            name,
                            delta: if op == "++" { 1 } else { -1 },
                            post: true,
                        })
                    }
                    _ => Ok(Expr::Var(name)),
                }
            }
            Some(Tok::Op("(")) => {
                self.i += 1;
                let e = self.comma()?;
                if !self.eat(")") {
                    return Err(self.syntax());
                }
                Ok(e)
            }
            _ => Err(self.syntax()),
        }
    }
}
