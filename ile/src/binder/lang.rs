// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The binder language: `STRPGMEXP`, `EXPORT` and `ENDPGMEXP` in a
//! source member.
//!
//! ```text
//! STRPGMEXP PGMLVL(*CURRENT) SIGNATURE('ACCOUNTS V2') LVLCHK(*YES)
//!   EXPORT SYMBOL('OPENACCT')
//!   EXPORT SYMBOL("closeAcct")
//! ENDPGMEXP
//! STRPGMEXP PGMLVL(*PRV)
//!   EXPORT SYMBOL('OPENACCT')
//! ENDPGMEXP
//! ```
//!
//! Keywords are case-insensitive; `/* */` comments are skipped; one
//! `*CURRENT` block is required and `*PRV` blocks are older export
//! lists the service program still answers to.

use super::signature::Signature;
use crate::error::{IleError, Result};

/// `PGMLVL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// `*CURRENT`.
    Current,
    /// `*PRV`.
    Prv,
}

/// One `STRPGMEXP` … `ENDPGMEXP` block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportBlock {
    /// Current or previous.
    pub level: Level,
    /// `SIGNATURE`, when given.
    pub signature: Option<Signature>,
    /// `LVLCHK`, when given.
    pub lvlchk: Option<bool>,
    /// Exported symbols in order.
    pub symbols: Vec<String>,
}

impl ExportBlock {
    /// The signature: the explicit one or the generated one.
    #[must_use]
    pub fn effective_signature(&self) -> Signature {
        self.signature
            .unwrap_or_else(|| Signature::of_exports(&self.symbols))
    }
}

/// A parsed binder source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinderSource {
    /// Blocks in source order; exactly one is `*CURRENT`.
    pub blocks: Vec<ExportBlock>,
}

impl BinderSource {
    /// The `*CURRENT` block.
    #[must_use]
    pub fn current(&self) -> &ExportBlock {
        self.blocks
            .iter()
            .find(|b| b.level == Level::Current)
            .expect("parsed sources have a current block")
    }

    /// The `*PRV` blocks in order.
    pub fn previous(&self) -> impl Iterator<Item = &ExportBlock> {
        self.blocks.iter().filter(|b| b.level == Level::Prv)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Str { text: String, hex: bool },
    Open,
    Close,
}

fn lex(src: &str) -> Result<Vec<(u32, Tok)>> {
    let mut out = Vec::new();
    let b = src.as_bytes();
    let mut i = 0;
    let mut line = 1u32;
    while i < b.len() {
        let c = b[i];
        if c == b'\n' {
            line += 1;
            i += 1;
        } else if c.is_ascii_whitespace() {
            i += 1;
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            let start = line;
            i += 2;
            loop {
                if i + 1 >= b.len() {
                    return Err(IleError::BinderSource {
                        line: start,
                        msg: "comment is not closed".into(),
                    });
                }
                if b[i] == b'\n' {
                    line += 1;
                }
                if b[i] == b'*' && b[i + 1] == b'/' {
                    i += 2;
                    break;
                }
                i += 1;
            }
        } else if c == b'(' {
            out.push((line, Tok::Open));
            i += 1;
        } else if c == b')' {
            out.push((line, Tok::Close));
            i += 1;
        } else if c == b'\'' || c == b'"' {
            let quote = c;
            let hex = out
                .last()
                .is_some_and(|(_, t)| matches!(t, Tok::Word(w) if w.eq_ignore_ascii_case("X")));
            if hex {
                out.pop();
            }
            i += 1;
            let mut text = String::new();
            loop {
                let Some(&d) = b.get(i) else {
                    return Err(IleError::BinderSource {
                        line,
                        msg: "string is not closed".into(),
                    });
                };
                if d == quote {
                    if b.get(i + 1) == Some(&quote) {
                        text.push(char::from(quote));
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                if d == b'\n' {
                    return Err(IleError::BinderSource {
                        line,
                        msg: "string is not closed".into(),
                    });
                }
                text.push(char::from(d));
                i += 1;
            }
            out.push((line, Tok::Str { text, hex }));
        } else {
            let start = i;
            while i < b.len()
                && !b[i].is_ascii_whitespace()
                && !matches!(b[i], b'(' | b')' | b'\'' | b'"')
            {
                i += 1;
            }
            out.push((line, Tok::Word(src[start..i].to_ascii_uppercase())));
        }
    }
    Ok(out)
}

struct P {
    toks: Vec<(u32, Tok)>,
    at: usize,
}

impl P {
    fn line(&self) -> u32 {
        self.toks
            .get(self.at)
            .or(self.toks.last())
            .map_or(1, |(l, _)| *l)
    }

    fn err(&self, msg: impl Into<String>) -> IleError {
        IleError::BinderSource {
            line: self.line(),
            msg: msg.into(),
        }
    }

    fn peek_word(&self) -> Option<&str> {
        match self.toks.get(self.at) {
            Some((_, Tok::Word(w))) => Some(w.as_str()),
            _ => None,
        }
    }

    fn word(&mut self) -> Result<String> {
        match self.toks.get(self.at) {
            Some((_, Tok::Word(w))) => {
                let w = w.clone();
                self.at += 1;
                Ok(w)
            }
            _ => Err(self.err("expected a keyword")),
        }
    }

    /// `( value )` where the value is a word or a string.
    fn paren_value(&mut self) -> Result<Tok> {
        match self.toks.get(self.at) {
            Some((_, Tok::Open)) => self.at += 1,
            _ => return Err(self.err("expected (")),
        }
        let v = match self.toks.get(self.at) {
            Some((_, t @ (Tok::Word(_) | Tok::Str { .. }))) => t.clone(),
            _ => return Err(self.err("expected a value")),
        };
        self.at += 1;
        match self.toks.get(self.at) {
            Some((_, Tok::Close)) => self.at += 1,
            _ => return Err(self.err("expected )")),
        }
        Ok(v)
    }
}

/// Parse binder source.
pub fn parse_binder_source(src: &str) -> Result<BinderSource> {
    let mut p = P {
        toks: lex(src)?,
        at: 0,
    };
    let mut blocks = Vec::new();
    while p.at < p.toks.len() {
        let kw = p.word()?;
        if kw != "STRPGMEXP" {
            return Err(p.err(format!("expected STRPGMEXP, found {kw}")));
        }
        let mut block = ExportBlock {
            level: Level::Current,
            signature: None,
            lvlchk: None,
            symbols: Vec::new(),
        };
        while let Some(w) = p.peek_word() {
            if w == "EXPORT" || w == "ENDPGMEXP" {
                break;
            }
            let key = p.word()?;
            let v = p.paren_value()?;
            match (key.as_str(), v) {
                ("PGMLVL", Tok::Word(w)) if w == "*CURRENT" => block.level = Level::Current,
                ("PGMLVL", Tok::Word(w)) if w == "*PRV" => block.level = Level::Prv,
                ("LVLCHK", Tok::Word(w)) if w == "*YES" => block.lvlchk = Some(true),
                ("LVLCHK", Tok::Word(w)) if w == "*NO" => block.lvlchk = Some(false),
                ("SIGNATURE", Tok::Word(w)) if w == "*GEN" => block.signature = None,
                ("SIGNATURE", Tok::Str { text, hex }) => {
                    block.signature = Some(Signature::from_source(&text, hex)?);
                }
                (k, _) => {
                    return Err(p.err(format!(
                        "{k} is not a STRPGMEXP parameter or has a bad value"
                    )));
                }
            }
        }
        loop {
            let kw = p.word()?;
            if kw == "ENDPGMEXP" {
                break;
            }
            if kw != "EXPORT" {
                return Err(p.err(format!("expected EXPORT or ENDPGMEXP, found {kw}")));
            }
            let key = p.word()?;
            if key != "SYMBOL" {
                return Err(p.err("EXPORT takes SYMBOL"));
            }
            match p.paren_value()? {
                Tok::Str { text, hex: false } => block.symbols.push(text),
                Tok::Word(w) => block.symbols.push(w),
                _ => return Err(p.err("SYMBOL takes a name")),
            }
        }
        blocks.push(block);
    }
    let current = blocks.iter().filter(|b| b.level == Level::Current).count();
    if current != 1 {
        return Err(IleError::BinderSource {
            line: 1,
            msg: format!("{current} STRPGMEXP PGMLVL(*CURRENT) blocks; exactly one is required"),
        });
    }
    Ok(BinderSource { blocks })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blocks() {
        let src = r#"
            /* accounts */
            STRPGMEXP PGMLVL(*CURRENT) SIGNATURE('ACCT V2') LVLCHK(*YES)
              EXPORT SYMBOL('OPENACCT')
              EXPORT SYMBOL("closeAcct")
            ENDPGMEXP
            strpgmexp pgmlvl(*prv)
              export symbol('OPENACCT')
            endpgmexp
        "#;
        let b = parse_binder_source(src).unwrap();
        assert_eq!(b.blocks.len(), 2);
        assert_eq!(b.current().symbols, vec!["OPENACCT", "closeAcct"]);
        assert_eq!(b.current().lvlchk, Some(true));
        assert!(b.current().signature.is_some());
        let prv: Vec<_> = b.previous().collect();
        assert_eq!(prv[0].symbols, vec!["OPENACCT"]);
        assert_eq!(
            prv[0].effective_signature(),
            Signature::of_exports(&["OPENACCT".into()])
        );
    }

    #[test]
    fn rejects_bad_source() {
        assert!(parse_binder_source("EXPORT SYMBOL('A')").is_err());
        assert!(
            parse_binder_source("STRPGMEXP PGMLVL(*PRV)\nEXPORT SYMBOL('A')\nENDPGMEXP").is_err()
        );
        assert!(parse_binder_source("STRPGMEXP\nEXPORT SYMBOL('A')").is_err());
        let hex = parse_binder_source(
            "STRPGMEXP SIGNATURE(X'000102030405060708090A0B0C0D0E0F')\nENDPGMEXP",
        )
        .unwrap();
        assert_eq!(hex.current().signature.unwrap().0[15], 15);
    }
}
