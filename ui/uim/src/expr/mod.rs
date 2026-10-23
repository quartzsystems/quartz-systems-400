// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The condition expression grammar (`docs/UIM.md` §4):
//!
//! ```text
//! expr    := and ( OR and )*
//! and     := not ( AND not )*
//! not     := NOT not | cmp
//! cmp     := operand ( (EQ|NE|GT|LT|GE|LE) operand )?
//! operand := '(' expr ')' | %LIST '(' name ')' | %NODE '(' name ')'
//!          | &name. | "string" | 'string' | number | name
//! ```
//!
//! A bare name is a variable, like `&name.`.

use crate::model::{CmpOp, Expr};

/// Parse an `EXPR` value.
pub fn parse(text: &str) -> Result<Expr, String> {
    let tokens = tokenize(text)?;
    let mut p = Parser { tokens, at: 0 };
    let e = p.or()?;
    match p.peek() {
        None => Ok(e),
        Some(t) => Err(format!("unexpected {}", t.describe())),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    LParen,
    RParen,
    Var(String),
    Str(String),
    Num(String),
    Word(String),
    Func(String, String),
}

impl Tok {
    fn describe(&self) -> String {
        match self {
            Self::LParen => "'('".into(),
            Self::RParen => "')'".into(),
            Self::Var(v) => format!("&{v}."),
            Self::Str(s) => format!("\"{s}\""),
            Self::Num(n) => n.clone(),
            Self::Word(w) => w.clone(),
            Self::Func(f, a) => format!("%{f}({a})"),
        }
    }
}

fn tokenize(text: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut at = 0;
    let mut out = Vec::new();
    while at < chars.len() {
        let c = chars[at];
        match c {
            ' ' => at += 1,
            '(' => {
                out.push(Tok::LParen);
                at += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                at += 1;
            }
            '&' => {
                at += 1;
                let name = word(&chars, &mut at);
                if name.is_empty() {
                    return Err("'&' without a variable name".to_string());
                }
                if chars.get(at) == Some(&'.') {
                    at += 1;
                }
                out.push(Tok::Var(name.to_ascii_uppercase()));
            }
            '"' | '\'' => {
                at += 1;
                let start = at;
                while at < chars.len() && chars[at] != c {
                    at += 1;
                }
                if at >= chars.len() {
                    return Err("string constant is not closed".to_string());
                }
                out.push(Tok::Str(chars[start..at].iter().collect()));
                at += 1;
            }
            '%' => {
                at += 1;
                let f = word(&chars, &mut at).to_ascii_uppercase();
                if f != "LIST" && f != "NODE" {
                    return Err(format!("%{f} is not a function; use %LIST or %NODE"));
                }
                skip_blanks(&chars, &mut at);
                if chars.get(at) != Some(&'(') {
                    return Err(format!("%{f} needs a name in parentheses"));
                }
                at += 1;
                skip_blanks(&chars, &mut at);
                let name = word(&chars, &mut at).to_ascii_uppercase();
                skip_blanks(&chars, &mut at);
                if name.is_empty() || chars.get(at) != Some(&')') {
                    return Err(format!("%{f} needs a name in parentheses"));
                }
                at += 1;
                out.push(Tok::Func(f, name));
            }
            c if c.is_ascii_digit() || c == '-' || c == '+' => {
                let start = at;
                at += 1;
                while at < chars.len() && (chars[at].is_ascii_digit() || chars[at] == '.') {
                    at += 1;
                }
                let n: String = chars[start..at].iter().collect();
                if n.len() == 1 && !c.is_ascii_digit() {
                    return Err(format!("{n} is not a number"));
                }
                out.push(Tok::Num(n));
            }
            c if c.is_ascii_alphabetic() => {
                let w = word(&chars, &mut at).to_ascii_uppercase();
                out.push(Tok::Word(w));
            }
            other => return Err(format!("character {other:?} is not expected")),
        }
    }
    Ok(out)
}

fn word(chars: &[char], at: &mut usize) -> String {
    let start = *at;
    while *at < chars.len() && (chars[*at].is_ascii_alphanumeric() || chars[*at] == '_') {
        *at += 1;
    }
    chars[start..*at].iter().collect()
}

fn skip_blanks(chars: &[char], at: &mut usize) {
    while *at < chars.len() && chars[*at] == ' ' {
        *at += 1;
    }
}

struct Parser {
    tokens: Vec<Tok>,
    at: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.at)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.tokens.get(self.at).cloned();
        self.at += 1;
        t
    }

    fn is_word(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Word(x)) if x == w)
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut left = self.and()?;
        while self.is_word("OR") {
            self.at += 1;
            let right = self.and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut left = self.not()?;
        while self.is_word("AND") {
            self.at += 1;
            let right = self.not()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn not(&mut self) -> Result<Expr, String> {
        if self.is_word("NOT") {
            self.at += 1;
            return Ok(Expr::Not(Box::new(self.not()?)));
        }
        self.cmp()
    }

    fn cmp(&mut self) -> Result<Expr, String> {
        let left = self.operand()?;
        let op = match self.peek() {
            Some(Tok::Word(w)) => match w.as_str() {
                "EQ" => CmpOp::Eq,
                "NE" => CmpOp::Ne,
                "GT" => CmpOp::Gt,
                "LT" => CmpOp::Lt,
                "GE" => CmpOp::Ge,
                "LE" => CmpOp::Le,
                _ => return Ok(left),
            },
            _ => return Ok(left),
        };
        self.at += 1;
        let right = self.operand()?;
        Ok(Expr::Cmp(op, Box::new(left), Box::new(right)))
    }

    fn operand(&mut self) -> Result<Expr, String> {
        match self.next() {
            None => Err("operand expected at the end".to_string()),
            Some(Tok::LParen) => {
                let e = self.or()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(e),
                    _ => Err("')' expected".to_string()),
                }
            }
            Some(Tok::RParen) => Err("')' without '('".to_string()),
            Some(Tok::Var(v)) => Ok(Expr::Var(v)),
            Some(Tok::Str(s)) => Ok(Expr::Str(s)),
            Some(Tok::Num(n)) => Ok(Expr::Num(n)),
            Some(Tok::Func(f, name)) => Ok(if f == "LIST" {
                Expr::List(name)
            } else {
                Expr::Node(name)
            }),
            Some(Tok::Word(w)) => match w.as_str() {
                "EQ" | "NE" | "GT" | "LT" | "GE" | "LE" | "AND" | "OR" | "NOT" => {
                    Err(format!("operand expected before {w}"))
                }
                _ => Ok(Expr::Var(w)),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_and_operands() {
        let expr = parse("&a. EQ 'x' AND NOT (&b GT 2 OR %LIST(l) EQ 0)").unwrap();
        let Expr::And(left, right) = expr else {
            panic!("{expr:?}")
        };
        assert_eq!(
            *left,
            Expr::Cmp(
                CmpOp::Eq,
                Box::new(Expr::Var("A".into())),
                Box::new(Expr::Str("x".into()))
            )
        );
        let Expr::Not(inner) = *right else {
            panic!("{right:?}")
        };
        let Expr::Or(first, second) = *inner else {
            panic!("{inner:?}")
        };
        assert_eq!(
            *first,
            Expr::Cmp(
                CmpOp::Gt,
                Box::new(Expr::Var("B".into())),
                Box::new(Expr::Num("2".into()))
            )
        );
        assert_eq!(
            *second,
            Expr::Cmp(
                CmpOp::Eq,
                Box::new(Expr::List("L".into())),
                Box::new(Expr::Num("0".into()))
            )
        );
        assert_eq!(parse("%NODE(n)").unwrap(), Expr::Node("N".into()));
        assert_eq!(parse("flag").unwrap(), Expr::Var("FLAG".into()));
        let vars = parse("&a EQ &b OR c").unwrap();
        assert_eq!(vars.variables(), ["A", "B", "C"]);
    }

    #[test]
    fn errors() {
        assert!(parse("").is_err());
        assert!(parse("&a EQ").is_err());
        assert!(parse("(&a EQ 1").is_err());
        assert!(parse("&a EQ 'x").is_err());
        assert!(parse("%SUM(x)").is_err());
        assert!(parse("&a EQ 1 2").is_err());
        assert!(parse("EQ 1").is_err());
        assert!(parse("&a ? 1").is_err());
    }
}
