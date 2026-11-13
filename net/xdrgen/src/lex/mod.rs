// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The lexer: identifiers, numbers, punctuation; `/* */` and `//`
//! comments are skipped.

use crate::error::{At, Pos, Result};

/// One token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// An identifier or keyword.
    Ident(String),
    /// A number (decimal, `0x` hexadecimal or `0` octal; decimal may be negative).
    Num(i128),
    /// One of `{ } ( ) [ ] < > ; , : = *`.
    Sym(char),
    /// The end of the description.
    Eof,
}

/// A token and where it starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The token.
    pub tok: Tok,
    /// Where it starts.
    pub pos: Pos,
}

/// The tokens of `src`, ending with [`Tok::Eof`].
pub fn tokens(src: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let (mut i, mut line, mut col) = (0usize, 1u32, 1u32);
    let at = |i: usize| chars.get(i).copied();
    while let Some(c) = at(i) {
        let pos = Pos { line, col };
        // Advance one character, keeping line and column.
        let mut step = |i: &mut usize, n: usize| {
            for _ in 0..n {
                if at(*i) == Some('\n') {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
                *i += 1;
            }
        };
        if c.is_whitespace() {
            step(&mut i, 1);
        } else if c == '/' && at(i + 1) == Some('*') {
            step(&mut i, 2);
            loop {
                match at(i) {
                    None => return Err(At::new(pos, "comment not closed")),
                    Some('*') if at(i + 1) == Some('/') => {
                        step(&mut i, 2);
                        break;
                    }
                    Some(_) => step(&mut i, 1),
                }
            }
        } else if c == '/' && at(i + 1) == Some('/') {
            while at(i).is_some_and(|c| c != '\n') {
                step(&mut i, 1);
            }
        } else if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while at(i).is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                step(&mut i, 1);
            }
            let s: String = chars.get(start..i).unwrap_or_default().iter().collect();
            out.push(Token {
                tok: Tok::Ident(s),
                pos,
            });
        } else if c.is_ascii_digit() || (c == '-' && at(i + 1).is_some_and(|d| d.is_ascii_digit()))
        {
            let start = i;
            step(&mut i, 1);
            while at(i).is_some_and(|c| c.is_ascii_alphanumeric()) {
                step(&mut i, 1);
            }
            let s: String = chars.get(start..i).unwrap_or_default().iter().collect();
            let n = number(&s).ok_or_else(|| At::new(pos, format!("bad number `{s}`")))?;
            out.push(Token {
                tok: Tok::Num(n),
                pos,
            });
        } else if "{}()[]<>;,:=*".contains(c) {
            step(&mut i, 1);
            out.push(Token {
                tok: Tok::Sym(c),
                pos,
            });
        } else {
            return Err(At::new(pos, format!("unexpected character `{c}`")));
        }
    }
    out.push(Token {
        tok: Tok::Eof,
        pos: Pos { line, col },
    });
    Ok(out)
}

/// A number in XDR's forms, within 65 bits signed.
fn number(s: &str) -> Option<i128> {
    let (neg, digits) = match s.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, s),
    };
    let v = if let Some(h) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        if neg {
            return None;
        }
        i128::from_str_radix(h, 16).ok()?
    } else if digits.len() > 1 && digits.starts_with('0') {
        if neg {
            return None;
        }
        i128::from_str_radix(&digits[1..], 8).ok()?
    } else {
        digits.parse::<i128>().ok()?
    };
    let v = if neg { -v } else { v };
    (-(1i128 << 63)..=i128::from(u64::MAX))
        .contains(&v)
        .then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        tokens(s).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn numbers_and_symbols() {
        assert_eq!(
            toks("const X = 0x10; -5 017 0"),
            vec![
                Tok::Ident("const".into()),
                Tok::Ident("X".into()),
                Tok::Sym('='),
                Tok::Num(16),
                Tok::Sym(';'),
                Tok::Num(-5),
                Tok::Num(15),
                Tok::Num(0),
                Tok::Eof
            ]
        );
    }

    #[test]
    fn comments_and_positions() {
        let t = tokens("/* a\n b */ x // c\n  y").unwrap();
        assert_eq!(t[0].pos, Pos { line: 2, col: 7 });
        assert_eq!(t[1].pos, Pos { line: 3, col: 3 });
    }

    #[test]
    fn errors() {
        assert_eq!(tokens("/* open").unwrap_err().msg, "comment not closed");
        assert!(tokens("x $").unwrap_err().msg.contains('$'));
        assert!(tokens("0x").is_err());
        assert!(tokens("99999999999999999999999").is_err());
        assert!(tokens("12ab").is_err());
    }
}
