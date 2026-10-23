// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The tag lexer. `:NAME attr=value.` starts a tag anywhere a colon
//! precedes a letter; the period closes it, quotes protect a value and
//! a tag may run over lines. Everything else is text, cut into one
//! [`Text`] token per line segment with the symbols resolved. `.*` at
//! the start of a line is a comment and `.IM` an imbed the source
//! already expanded.

pub mod symbol;

use crate::diag::{Diagnostics, ids};
use crate::source::Source;

/// One token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    /// A tag with its attributes.
    Tag(Tag),
    /// Text between tags.
    Text(Text),
    /// A `.*` line.
    Comment {
        /// The line.
        line: usize,
    },
    /// A `.IM` line; its member's lines follow.
    Imbed {
        /// The line.
        line: usize,
    },
}

/// A tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    /// Upper-case name without the colon.
    pub name: String,
    /// Attributes in source order.
    pub attrs: Vec<Attr>,
    /// The line the tag starts on.
    pub line: usize,
}

impl Tag {
    /// The last value of an attribute.
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&Attr> {
        self.attrs.iter().rev().find(|a| a.name == name)
    }
}

/// One attribute.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attr {
    /// Upper-case name.
    pub name: String,
    /// The value; `None` for a flag written without `=`.
    pub value: Option<String>,
    /// The value was quoted, so its case is as written.
    pub quoted: bool,
    /// The line the attribute is on.
    pub line: usize,
}

/// Text on one line, between tags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    /// The line.
    pub line: usize,
    /// Literal runs and variable references.
    pub pieces: Vec<Piece>,
}

impl Text {
    /// Only blanks.
    #[must_use]
    pub fn is_blank(&self) -> bool {
        self.pieces.iter().all(|p| match p {
            Piece::Text(t) => t.trim().is_empty(),
            Piece::Var(_) => false,
        })
    }
}

/// A run of text or a `&name.` variable reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// Literal text, symbols resolved.
    Text(String),
    /// A dialog variable, upper-case.
    Var(String),
}

/// Lex the whole source.
#[must_use]
pub fn lex(source: &Source) -> (Vec<Token>, Diagnostics) {
    let mut lx = Lexer {
        out: Vec::new(),
        diags: Diagnostics::new(),
        open: None,
    };
    for (line, l) in source.lines().iter().enumerate() {
        lx.line(line, &l.text);
    }
    if let Some(open) = lx.open.take() {
        lx.diags.push(ids::tag_not_closed(open.line, open.name()));
    }
    (lx.out, lx.diags)
}

/// A tag being gathered over lines.
struct OpenTag {
    line: usize,
    raw: String,
    in_quote: bool,
}

impl OpenTag {
    fn name(&self) -> &str {
        let end = self
            .raw
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(self.raw.len());
        &self.raw[..end]
    }
}

struct Lexer {
    out: Vec<Token>,
    diags: Diagnostics,
    open: Option<OpenTag>,
}

impl Lexer {
    fn line(&mut self, line: usize, text: &str) {
        let chars: Vec<char> = text.chars().collect();
        let mut at = 0;
        if self.open.is_none() {
            if text.starts_with(".*") {
                self.out.push(Token::Comment { line });
                return;
            }
            if text == ".IM" || text.starts_with(".IM ") {
                self.out.push(Token::Imbed { line });
                return;
            }
            if text.starts_with('.') && chars.get(1).is_some_and(char::is_ascii_alphabetic) {
                let word: String = text.split_whitespace().next().unwrap_or(".").to_string();
                self.diags.push(ids::control_word(line, &word));
                return;
            }
        } else if !self.gather(&chars, &mut at) {
            return;
        }
        self.text(line, &chars, at);
    }

    /// Continue an open tag over `chars`; true when it closed on this line.
    fn gather(&mut self, chars: &[char], at: &mut usize) -> bool {
        let open = self.open.as_mut().expect("open tag");
        if !open.raw.is_empty() {
            open.raw.push(' ');
        }
        let mut closed = false;
        while *at < chars.len() {
            let c = chars[*at];
            *at += 1;
            if open.in_quote {
                open.raw.push(c);
                if c == '\'' {
                    open.in_quote = false;
                }
                continue;
            }
            match c {
                '\'' => {
                    open.in_quote = true;
                    open.raw.push(c);
                }
                '.' => {
                    closed = true;
                    break;
                }
                _ => open.raw.push(c),
            }
        }
        if closed {
            let open = self.open.take().expect("open tag");
            let tag = parse_tag(&open, &mut self.diags);
            self.out.push(Token::Tag(tag));
        }
        closed
    }

    /// Text from `at`, opening tags as they come. An empty line is an
    /// empty text token, so `:XMP.` and `:LINES.` keep their blank lines.
    fn text(&mut self, line: usize, chars: &[char], mut at: usize) {
        if chars.is_empty() {
            self.out.push(Token::Text(Text {
                line,
                pieces: vec![Piece::Text(String::new())],
            }));
            return;
        }
        let mut pieces = Vec::new();
        let mut run = String::new();
        while at < chars.len() {
            let c = chars[at];
            if c == ':' && chars.get(at + 1).is_some_and(char::is_ascii_alphabetic) {
                flush(&mut run, &mut pieces);
                if !pieces.is_empty() {
                    self.out.push(Token::Text(Text {
                        line,
                        pieces: std::mem::take(&mut pieces),
                    }));
                }
                self.open = Some(OpenTag {
                    line,
                    raw: String::new(),
                    in_quote: false,
                });
                at += 1;
                if self.gather(chars, &mut at) {
                    continue;
                }
                return;
            }
            if c == '&'
                && let Some((name, end)) = symbol_at(chars, at)
            {
                at = end;
                if let Some(ch) = symbol::builtin(&name) {
                    run.push(ch);
                } else {
                    flush(&mut run, &mut pieces);
                    pieces.push(Piece::Var(name.to_ascii_uppercase()));
                }
                continue;
            }
            run.push(c);
            at += 1;
        }
        flush(&mut run, &mut pieces);
        if !pieces.is_empty() {
            self.out.push(Token::Text(Text { line, pieces }));
        }
    }
}

fn flush(run: &mut String, pieces: &mut Vec<Piece>) {
    if !run.is_empty() {
        pieces.push(Piece::Text(std::mem::take(run)));
    }
}

/// `&name.` at `at`: the name and the index after the period.
fn symbol_at(chars: &[char], at: usize) -> Option<(String, usize)> {
    let mut i = at + 1;
    if !chars.get(i).is_some_and(char::is_ascii_alphabetic) {
        return None;
    }
    let mut name = String::new();
    while let Some(&c) = chars.get(i) {
        if c.is_ascii_alphanumeric() || c == '_' {
            name.push(c);
            i += 1;
        } else {
            break;
        }
    }
    if chars.get(i) == Some(&'.') {
        Some((name, i + 1))
    } else {
        None
    }
}

/// Parse the gathered text of a tag (without colon and period).
fn parse_tag(open: &OpenTag, diags: &mut Diagnostics) -> Tag {
    let chars: Vec<char> = open.raw.chars().collect();
    let mut at = 0;
    let name = word(&chars, &mut at).to_ascii_uppercase();
    let mut attrs = Vec::new();
    loop {
        while at < chars.len() && chars[at].is_whitespace() {
            at += 1;
        }
        if at >= chars.len() {
            break;
        }
        if !chars[at].is_ascii_alphabetic() {
            diags.push(ids::stray_in_tag(open.line, &name, chars[at]));
            at += 1;
            continue;
        }
        let aname = word(&chars, &mut at).to_ascii_uppercase();
        let mut value = None;
        let mut quoted = false;
        if chars.get(at) == Some(&'=') {
            at += 1;
            if chars.get(at) == Some(&'\'') {
                quoted = true;
                at += 1;
                let mut v = String::new();
                let mut closed = false;
                while at < chars.len() {
                    let c = chars[at];
                    at += 1;
                    if c == '\'' {
                        if chars.get(at) == Some(&'\'') {
                            v.push('\'');
                            at += 1;
                        } else {
                            closed = true;
                            break;
                        }
                    } else {
                        v.push(c);
                    }
                }
                if !closed {
                    diags.push(ids::quote_not_closed(open.line, &name, &aname));
                }
                value = Some(v);
            } else {
                let start = at;
                while at < chars.len() && !chars[at].is_whitespace() {
                    at += 1;
                }
                let v: String = chars[start..at].iter().collect();
                value = Some(v.to_ascii_uppercase());
            }
        }
        attrs.push(Attr {
            name: aname,
            value,
            quoted,
            line: open.line,
        });
    }
    Tag {
        name,
        attrs,
        line: open.line,
    }
}

fn word(chars: &[char], at: &mut usize) -> String {
    let start = *at;
    while *at < chars.len() && (chars[*at].is_ascii_alphanumeric() || chars[*at] == '_') {
        *at += 1;
    }
    chars[start..*at].iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::NoImbeds;

    fn toks(text: &str) -> Vec<Token> {
        let (s, _) = Source::read(text, &NoImbeds);
        lex(&s).0
    }

    #[test]
    fn tags_text_and_symbols() {
        let t = toks(":PANEL NAME=main help='a/b' .Title &amp. :HP2.x:EHP2. &var.\n.* note\nmore");
        let Token::Tag(tag) = &t[0] else {
            panic!("{t:?}")
        };
        assert_eq!(tag.name, "PANEL");
        assert_eq!(tag.attr("NAME").unwrap().value.as_deref(), Some("MAIN"));
        assert_eq!(tag.attr("HELP").unwrap().value.as_deref(), Some("a/b"));
        assert!(tag.attr("HELP").unwrap().quoted);
        let Token::Text(x) = &t[1] else {
            panic!("{t:?}")
        };
        assert_eq!(x.pieces, [Piece::Text("Title & ".into())]);
        assert!(matches!(&t[2], Token::Tag(g) if g.name == "HP2"));
        assert!(matches!(&t[4], Token::Tag(g) if g.name == "EHP2"));
        let Token::Text(x) = &t[5] else {
            panic!("{t:?}")
        };
        assert_eq!(
            x.pieces,
            [Piece::Text(" ".into()), Piece::Var("VAR".into())]
        );
        assert_eq!(t[6], Token::Comment { line: 1 });
        assert!(matches!(&t[7], Token::Text(x) if x.line == 2));
    }

    #[test]
    fn tags_span_lines_and_quotes_hold_periods() {
        let t = toks(":CLASS NAME=c\n   BASETYPE='CHAR 10'\n   WIDTH=10.\n:ECLASS.");
        let Token::Tag(tag) = &t[0] else {
            panic!("{t:?}")
        };
        assert_eq!(tag.attrs.len(), 3);
        assert_eq!(
            tag.attr("BASETYPE").unwrap().value.as_deref(),
            Some("CHAR 10")
        );
        assert!(matches!(&t[1], Token::Tag(g) if g.name == "ECLASS"));
        let t = toks(":COND NAME=x EXPR='&a. EQ ''y.z'''.");
        let Token::Tag(tag) = &t[0] else {
            panic!("{t:?}")
        };
        assert_eq!(
            tag.attr("EXPR").unwrap().value.as_deref(),
            Some("&a. EQ 'y.z'")
        );
    }

    #[test]
    fn open_tag_at_end_is_reported() {
        let (s, _) = Source::read(":PANEL NAME=x", &NoImbeds);
        let (_, d) = lex(&s);
        assert_eq!(d.items()[0].id, "UIM0103");
    }
}
