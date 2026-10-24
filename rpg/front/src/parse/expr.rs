// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Expressions, with IBM's precedence: parentheses, then built-in and
//! procedure calls, then unary `+ - NOT`, `**` (right to left), `* /`,
//! binary `+ -`, the comparisons, `AND`, `OR`.

use crate::ast::{AssignOp, BinOp, Expr, Fig, Lit, Pos, Step, UnOp};
use crate::diag::{Diag, ids};
use crate::lex::{Kind, Tok};

/// A cursor over tokens.
pub struct Cursor<'a> {
    toks: &'a [Tok],
    i: usize,
    /// Where the tokens end, for messages about a missing operand.
    pub end: Pos,
}

impl<'a> Cursor<'a> {
    /// A cursor at the first token.
    #[must_use]
    pub fn new(toks: &'a [Tok], end: Pos) -> Self {
        Self { toks, i: 0, end }
    }

    /// The token under the cursor.
    #[must_use]
    pub fn peek(&self) -> Option<&'a Tok> {
        self.toks.get(self.i)
    }

    /// The token `n` past the cursor.
    #[must_use]
    pub fn peek_at(&self, n: usize) -> Option<&'a Tok> {
        self.toks.get(self.i + n)
    }

    /// Take the token under the cursor.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<&'a Tok> {
        let t = self.toks.get(self.i);
        if t.is_some() {
            self.i += 1;
        }
        t
    }

    /// Whether every token was taken.
    #[must_use]
    pub fn done(&self) -> bool {
        self.i >= self.toks.len()
    }

    /// Where the cursor is.
    #[must_use]
    pub fn pos(&self) -> Pos {
        self.peek().map_or(self.end, |t| t.at)
    }

    /// Take punctuation `p` if it is next.
    pub fn eat(&mut self, p: &str) -> bool {
        if self.peek().is_some_and(|t| t.is(p)) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// Take the name `w` if it is next.
    pub fn eat_word(&mut self, w: &str) -> bool {
        if self.peek().and_then(Tok::name) == Some(w) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// The tokens not yet taken.
    #[must_use]
    pub fn rest(&self) -> &'a [Tok] {
        &self.toks[self.i.min(self.toks.len())..]
    }

    /// Move the cursor to the end.
    pub fn skip_all(&mut self) {
        self.i = self.toks.len();
    }
}

/// Parse a whole token list as one expression; extra tokens are an
/// error.
pub fn expr_all(toks: &[Tok], end: Pos, diags: &mut Vec<Diag>) -> Option<Expr> {
    let mut c = Cursor::new(toks, end);
    let e = expr(&mut c, diags)?;
    if !c.done() {
        diags.push(Diag::new(ids::END_OF_EXPRESSION, 20, c.pos()));
    }
    Some(e)
}

/// Parse a token list as operands split at top-level colons.
pub fn operands(toks: &[Tok], end: Pos, diags: &mut Vec<Diag>) -> Vec<Expr> {
    let mut out = Vec::new();
    for part in split_colons(toks) {
        if part.is_empty() {
            out.push(Expr::Empty(end));
            continue;
        }
        if let Some(e) = expr_all(part, end, diags) {
            out.push(e);
        }
    }
    out
}

/// Split tokens at colons outside parentheses.
#[must_use]
pub fn split_colons(toks: &[Tok]) -> Vec<&[Tok]> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0;
    for (i, t) in toks.iter().enumerate() {
        if t.is("(") {
            depth += 1;
        } else if t.is(")") {
            depth -= 1;
        } else if t.is(":") && depth == 0 {
            out.push(&toks[start..i]);
            start = i + 1;
        }
    }
    if start < toks.len() || !out.is_empty() {
        out.push(&toks[start..]);
    }
    out
}

/// An assignment `target op expr`, or a bare expression when there is
/// no assignment operator.
pub fn assignment(c: &mut Cursor<'_>, diags: &mut Vec<Diag>) -> Option<Expr> {
    let start = c.pos();
    let target = expr_prec(c, diags, Level::Unary)?;
    let op = match c.peek().map(|t| &t.kind) {
        Some(Kind::Punct("=")) => AssignOp::Set,
        Some(Kind::Punct("+=")) => AssignOp::Add,
        Some(Kind::Punct("-=")) => AssignOp::Sub,
        Some(Kind::Punct("*=")) => AssignOp::Mul,
        Some(Kind::Punct("/=")) => AssignOp::Div,
        Some(Kind::Punct("**=")) => AssignOp::Pow,
        _ => {
            diags.push(Diag::new(ids::ASSIGNMENT_EXPECTED, 30, c.pos()));
            return None;
        }
    };
    c.next();
    let value = expr(c, diags)?;
    Some(Expr::Assign(op, Box::new(target), Box::new(value), start))
}

/// Precedence levels, loosest first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Or,
    And,
    Compare,
    Add,
    Mul,
    Pow,
    Unary,
}

/// Parse an expression.
pub fn expr(c: &mut Cursor<'_>, diags: &mut Vec<Diag>) -> Option<Expr> {
    expr_prec(c, diags, Level::Or)
}

fn binop(t: &Tok) -> Option<(BinOp, Level)> {
    Some(match &t.kind {
        Kind::Name(n) if n == "OR" => (BinOp::Or, Level::Or),
        Kind::Name(n) if n == "AND" => (BinOp::And, Level::And),
        Kind::Punct("=") => (BinOp::Eq, Level::Compare),
        Kind::Punct("<>") => (BinOp::Ne, Level::Compare),
        Kind::Punct("<") => (BinOp::Lt, Level::Compare),
        Kind::Punct(">") => (BinOp::Gt, Level::Compare),
        Kind::Punct("<=") => (BinOp::Le, Level::Compare),
        Kind::Punct(">=") => (BinOp::Ge, Level::Compare),
        Kind::Name(n) if n == "IN" => (BinOp::In, Level::Compare),
        Kind::Punct("+") => (BinOp::Add, Level::Add),
        Kind::Punct("-") => (BinOp::Sub, Level::Add),
        Kind::Punct("*") => (BinOp::Mul, Level::Mul),
        Kind::Punct("/") => (BinOp::Div, Level::Mul),
        Kind::Punct("**") => (BinOp::Pow, Level::Pow),
        _ => return None,
    })
}

fn expr_prec(c: &mut Cursor<'_>, diags: &mut Vec<Diag>, min: Level) -> Option<Expr> {
    let mut left = if min <= Level::Unary {
        unary(c, diags)?
    } else {
        primary(c, diags)?
    };
    while let Some(t) = c.peek() {
        let Some((op, lvl)) = binop(t) else { break };
        if lvl < min {
            break;
        }
        let at = t.at;
        c.next();
        // `**` groups right to left; everything else left to right.
        let next = if op == BinOp::Pow {
            Level::Pow
        } else {
            match lvl {
                Level::Or => Level::And,
                Level::And => Level::Compare,
                Level::Compare => Level::Add,
                Level::Add => Level::Mul,
                Level::Mul => Level::Pow,
                Level::Pow | Level::Unary => Level::Unary,
            }
        };
        let right = expr_prec(c, diags, next)?;
        left = Expr::Binary(op, Box::new(left), Box::new(right), at);
    }
    Some(left)
}

fn unary(c: &mut Cursor<'_>, diags: &mut Vec<Diag>) -> Option<Expr> {
    let Some(t) = c.peek() else {
        diags.push(Diag::new(ids::MISSING_OPERAND, 30, c.end));
        return None;
    };
    let at = t.at;
    let op = match &t.kind {
        Kind::Punct("-") => Some(UnOp::Neg),
        Kind::Punct("+") => Some(UnOp::Plus),
        Kind::Name(n) if n == "NOT" => Some(UnOp::Not),
        _ => None,
    };
    if let Some(op) = op {
        c.next();
        // A signed numeric literal stays a literal.
        if op != UnOp::Not
            && let Some(Tok {
                kind: Kind::Num(n), ..
            }) = c.peek()
        {
            let n = n.clone();
            c.next();
            let text = if op == UnOp::Neg { format!("-{n}") } else { n };
            return postfix_pow(c, diags, Expr::Lit(Lit::Num(text), at));
        }
        let inner = unary(c, diags)?;
        return postfix_pow(c, diags, Expr::Unary(op, Box::new(inner), at));
    }
    primary(c, diags)
}

/// A `**` right after a unary operand binds to it (`-2 ** 2` is 4).
fn postfix_pow(c: &mut Cursor<'_>, diags: &mut Vec<Diag>, e: Expr) -> Option<Expr> {
    if c.peek().is_some_and(|t| t.is("**")) {
        let at = c.pos();
        c.next();
        let r = expr_prec(c, diags, Level::Pow)?;
        return Some(Expr::Binary(BinOp::Pow, Box::new(e), Box::new(r), at));
    }
    Some(e)
}

fn fig(word: &str) -> Option<Fig> {
    Some(match word {
        "*BLANK" | "*BLANKS" => Fig::Blanks,
        "*ZERO" | "*ZEROS" => Fig::Zeros,
        "*HIVAL" => Fig::Hival,
        "*LOVAL" => Fig::Loval,
        "*ON" => Fig::On,
        "*OFF" => Fig::Off,
        "*NULL" => Fig::Null,
        _ => return None,
    })
}

/// Arguments in parentheses, the `(` already taken: colon separated,
/// an empty one allowed.
fn args(c: &mut Cursor<'_>, diags: &mut Vec<Diag>) -> Option<Vec<Expr>> {
    let mut out = Vec::new();
    if c.eat(")") {
        return Some(out);
    }
    loop {
        let at = c.pos();
        if c.peek().is_some_and(|t| t.is(":") || t.is(")")) {
            out.push(Expr::Empty(at));
        } else if c.peek().is_some_and(|t| t.is("*")) && c.peek_at(1).is_some_and(|t| t.is(")") || t.is(":")) {
            c.next();
            out.push(Expr::Special {
                word: "*".into(),
                index: None,
                at,
            });
        } else {
            out.push(expr(c, diags)?);
        }
        if c.eat(":") {
            continue;
        }
        if c.eat(")") {
            return Some(out);
        }
        diags.push(Diag::new(ids::RPAREN_EXPECTED, 20, c.pos()));
        return None;
    }
}

fn primary(c: &mut Cursor<'_>, diags: &mut Vec<Diag>) -> Option<Expr> {
    let Some(t) = c.next() else {
        diags.push(Diag::new(ids::MISSING_OPERAND, 30, c.end));
        return None;
    };
    let at = t.at;
    Some(match &t.kind {
        Kind::Num(n) => {
            if n.contains('E') && n.parse::<f64>().is_err() {
                diags.push(Diag::new(ids::BAD_FLOAT, 20, at));
            }
            Expr::Lit(Lit::Num(n.clone()), at)
        }
        Kind::Str(s) => Expr::Lit(Lit::Char(s.clone()), at),
        Kind::Typed(p, s) => Expr::Lit(
            match p {
                'X' => Lit::Hex(s.clone()),
                'G' => Lit::Graph(s.clone()),
                'U' => Lit::Ucs2(s.clone()),
                'D' => Lit::Date(s.clone()),
                'T' => Lit::Time(s.clone()),
                _ => Lit::Timestamp(s.clone()),
            },
            at,
        ),
        Kind::All(k, s) => Expr::Fig(
            match k.as_str() {
                "*ALLX" => Fig::AllX(s.clone()),
                "*ALLG" => Fig::AllG(s.clone()),
                _ => Fig::All(s.clone()),
            },
            at,
        ),
        Kind::Special(w) => {
            if let Some(f) = fig(w) {
                Expr::Fig(f, at)
            } else if w == "*IN" && c.eat("(") {
                let idx = expr(c, diags)?;
                if !c.eat(")") {
                    diags.push(Diag::new(ids::RPAREN_EXPECTED, 20, c.pos()));
                }
                Expr::Special {
                    word: w.clone(),
                    index: Some(Box::new(idx)),
                    at,
                }
            } else {
                Expr::Special {
                    word: w.clone(),
                    index: None,
                    at,
                }
            }
        }
        Kind::Bif(name) => {
            let a = if c.eat("(") { args(c, diags)? } else { Vec::new() };
            Expr::Bif {
                name: name.clone(),
                args: a,
                at,
            }
        }
        Kind::Name(name) => {
            let mut steps = Vec::new();
            loop {
                if c.peek().is_some_and(|t| t.is("(")) {
                    c.next();
                    steps.push(Step::Args(args(c, diags)?));
                } else if c.peek().is_some_and(|t| t.is("."))
                    && c.peek_at(1).is_some_and(|t| t.name().is_some())
                {
                    c.next();
                    let n = c.next().and_then(Tok::name).unwrap_or_default().to_string();
                    steps.push(Step::Field(n));
                } else {
                    break;
                }
            }
            Expr::Ref {
                name: name.clone(),
                steps,
                at,
            }
        }
        Kind::Punct("(") => {
            let mut items = vec![expr(c, diags)?];
            let mut list = false;
            while c.eat(":") {
                list = true;
                items.push(expr(c, diags)?);
            }
            if !c.eat(")") {
                diags.push(Diag::new(ids::RPAREN_EXPECTED, 20, c.pos()));
                return None;
            }
            if list {
                Expr::List(items, at)
            } else {
                items.pop().expect("one item")
            }
        }
        _ => {
            diags.push(Diag::new(ids::UNEXPECTED, 30, at).arg(t.text()));
            return None;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex::lex_text;

    fn parse(s: &str) -> Expr {
        let mut d = Vec::new();
        let toks = lex_text(s, Pos { line: 1, col: 1 }, &mut d);
        let e = expr_all(&toks, Pos::default(), &mut d).unwrap_or_else(|| panic!("{s}: {d:?}"));
        assert!(d.is_empty(), "{d:?}");
        e
    }

    fn show(e: &Expr) -> String {
        match e {
            Expr::Ref { name, steps, .. } => {
                let mut s = name.clone();
                for st in steps {
                    match st {
                        Step::Args(a) => {
                            s.push('(');
                            s.push_str(&a.iter().map(show).collect::<Vec<_>>().join(":"));
                            s.push(')');
                        }
                        Step::Field(f) => {
                            s.push('.');
                            s.push_str(f);
                        }
                    }
                }
                s
            }
            Expr::Special { word, index, .. } => match index {
                Some(i) => format!("{word}({})", show(i)),
                None => word.clone(),
            },
            Expr::Bif { name, args, .. } => {
                format!("{name}({})", args.iter().map(show).collect::<Vec<_>>().join(":"))
            }
            Expr::Lit(l, _) => format!("{l:?}"),
            Expr::Fig(f, _) => format!("{f:?}"),
            Expr::Unary(op, a, _) => format!("({op:?} {})", show(a)),
            Expr::Binary(op, a, b, _) => format!("({} {op:?} {})", show(a), show(b)),
            Expr::Assign(op, a, b, _) => format!("{} {op:?} {}", show(a), show(b)),
            Expr::List(v, _) => format!("[{}]", v.iter().map(show).collect::<Vec<_>>().join(":")),
            Expr::Empty(_) => "_".into(),
        }
    }

    #[test]
    fn precedence_follows_ibm() {
        assert_eq!(show(&parse("a + b * c")), "(A Add (B Mul C))");
        assert_eq!(show(&parse("a - b - c")), "((A Sub B) Sub C)");
        assert_eq!(show(&parse("a ** b ** c")), "(A Pow (B Pow C))");
        assert_eq!(show(&parse("-2 ** 2")), "(Num(\"-2\") Pow Num(\"2\"))");
        assert_eq!(show(&parse("a = 1 or b <> 2 and not c")), "((A Eq Num(\"1\")) Or ((B Ne Num(\"2\")) And (Not C)))");
    }

    #[test]
    fn references_and_calls() {
        assert_eq!(show(&parse("ds(i).sub(2)")), "DS(I).SUB(Num(\"2\"))");
        assert_eq!(show(&parse("%subst(x:2:%len(y))")), "%SUBST(X:Num(\"2\"):%LEN(Y))");
        assert_eq!(show(&parse("*in(5) and *inlr")), "(*IN(Num(\"5\")) And *INLR)");
        assert_eq!(show(&parse("(k1:k2)")), "[K1:K2]");
        assert_eq!(show(&parse("f(a::*omit)")), "F(A:_:*OMIT)");
        assert_eq!(show(&parse("sorta(arr(*).x)")), "SORTA(ARR(*).X)");
    }
}
