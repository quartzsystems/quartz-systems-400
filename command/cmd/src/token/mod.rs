// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The CL tokenizer: command strings, command-definition source and CL
//! source share one lexical form. Comments go, continuation lines are
//! joined, then the text becomes a tree of tokens: words, numbers,
//! quoted strings, hex strings, `&variables`, operators, `KEYWORD(args)`
//! and `%BUILTIN(args)` with their arguments, and bare `(lists)`.

mod lines;

pub use lines::{Joined, join};

use std::fmt::Write as _;

use crate::error::{CmdError, Diag};

/// A selective-prompt prefix on a keyword: `??KWD`, `?*KWD`, `?-KWD`,
/// `?<KWD`, `?&KWD`, or the `?` before a command name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sel {
    /// `??`: prompt with the value shown and changeable.
    Prompt,
    /// `?*`: show the value, protect it.
    Show,
    /// `?-`: leave the parameter off the prompt.
    Omit,
    /// `?<`: show the value as a default and allow a change.
    Default,
    /// `?&`: do not show the parameter, include its value.
    Hidden,
}

impl Sel {
    /// The prefix as typed.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::Prompt => "??",
            Self::Show => "?*",
            Self::Omit => "?-",
            Self::Default => "?<",
            Self::Hidden => "?&",
        }
    }
}

/// One token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// A bare word, upper case: a name, `*SPECIAL`, `LIB/NAME`.
    Word(String),
    /// A number as typed: digits, an optional sign and decimal point.
    Num(String),
    /// `'quoted'` text with `''` collapsed, case kept.
    Str(String),
    /// `X'..'` bytes.
    Hex(Vec<u8>),
    /// `&NAME`, upper case, without the `&`.
    Var(String),
    /// An operator: `+ - * /`, or `||`, `|<`, `|>` as `*CAT`, `*BCAT`,
    /// `*TCAT`.
    Op(&'static str),
    /// `NAME(args)`: a keyword with its arguments, and the selective
    /// prompt prefix before it, if any.
    Kw {
        /// The keyword, upper case.
        name: String,
        /// The prefix.
        pfx: Option<Sel>,
        /// The arguments.
        args: Vec<Tok>,
    },
    /// `%NAME(args)`: a built-in function.
    Builtin {
        /// The function, upper case, without the `%`.
        name: String,
        /// The arguments.
        args: Vec<Tok>,
    },
    /// `(args)`: a list or a nested expression.
    Paren(Vec<Tok>),
}

impl Tok {
    /// The word of a `Word` token.
    #[must_use]
    pub fn word(&self) -> Option<&str> {
        match self {
            Self::Word(w) => Some(w),
            _ => None,
        }
    }

    /// The text a token contributes to a command string.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Word(w) | Self::Num(w) => w.clone(),
            Self::Str(s) => format!("'{}'", s.replace('\'', "''")),
            Self::Hex(b) => {
                let mut s = String::from("X'");
                for x in b {
                    let _ = write!(s, "{x:02X}");
                }
                s.push('\'');
                s
            }
            Self::Var(v) => format!("&{v}"),
            Self::Op(o) => (*o).to_string(),
            Self::Kw { name, pfx, args } => {
                format!("{}{name}({})", pfx.map_or("", Sel::text), Self::join(args))
            }
            Self::Builtin { name, args } => format!("%{name}({})", Self::join(args)),
            Self::Paren(args) => format!("({})", Self::join(args)),
        }
    }

    /// Tokens as text, blank separated.
    #[must_use]
    pub fn join(toks: &[Tok]) -> String {
        toks.iter().map(Tok::text).collect::<Vec<_>>().join(" ")
    }
}

/// A tokenized command string.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tokens {
    /// The label before the command, without its colon.
    pub label: Option<String>,
    /// `?` before the command name: prompt the whole command.
    pub prompt: bool,
    /// The command name, upper case, and its library when qualified.
    pub name: String,
    /// The library, `None` for `*LIBL`.
    pub lib: Option<String>,
    /// The parameters: positional values, then `Kw` tokens.
    pub toks: Vec<Tok>,
}

/// How the tokenizer reads numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// The job's `DECFMT`: `' '`, `'I'` or `'J'`; `I` and `J` take `,`
    /// as the decimal point.
    pub decfmt: char,
}

impl Default for Options {
    fn default() -> Self {
        Self { decfmt: ' ' }
    }
}

impl Options {
    /// The decimal point.
    #[must_use]
    pub fn point(self) -> char {
        if self.decfmt == 'I' || self.decfmt == 'J' {
            ','
        } else {
            '.'
        }
    }
}

/// Tokenize one command string (comments and continuations already
/// joined by [`join`], or a one-line string).
pub fn tokenize(text: &str, opts: Options) -> Result<Tokens, CmdError> {
    let mut lx = Lexer {
        chars: text.chars().collect(),
        at: 0,
        opts,
    };
    let toks = lx.list(false)?;
    structure(toks)
}

/// A token list as a command: the label, the `?`, the name and its
/// library, then the parameters (what `tokenize` does after lexing).
pub fn structure(toks: Vec<Tok>) -> Result<Tokens, CmdError> {
    let mut toks = toks.into_iter().peekable();
    let mut out = Tokens::default();
    let Some(first) = toks.next() else {
        return Ok(out);
    };
    let mut first = first;
    if let Tok::Word(w) = &first
        && w.ends_with(':')
        && w.len() > 1
    {
        out.label = Some(w[..w.len() - 1].to_string());
        match toks.next() {
            Some(t) => first = t,
            None => return Ok(out),
        }
    }
    let name = match first {
        Tok::Word(w) => w,
        Tok::Kw { name, pfx, args } => {
            // `CALL(...)` is not a command; `CMD(`-style text is a keyword
            // without a command: report it as such.
            let _ = (pfx, args);
            return Err(Diag::new("CPD0030").data(&[(&name, 10)]).into());
        }
        other => {
            return Err(Diag::new("CPD0030").data(&[(&other.text(), 10)]).into());
        }
    };
    let name = if let Some(rest) = name.strip_prefix('?') {
        out.prompt = true;
        rest.to_string()
    } else {
        name
    };
    let (lib, name) = match name.split_once('/') {
        Some((l, n)) => (Some(l.to_string()), n.to_string()),
        None => (None, name),
    };
    out.name = name;
    out.lib = lib;
    out.toks = toks.collect();
    Ok(out)
}

/// Tokenize text that is not a command: the arguments of a keyword, an
/// expression, the parameter list of a definition statement.
pub fn tokenize_list(text: &str, opts: Options) -> Result<Vec<Tok>, CmdError> {
    let mut lx = Lexer {
        chars: text.chars().collect(),
        at: 0,
        opts,
    };
    lx.list(false)
}

struct Lexer {
    chars: Vec<char>,
    at: usize,
    opts: Options,
}

impl Lexer {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.at).copied()
    }

    fn peek2(&self) -> Option<char> {
        self.chars.get(self.at + 1).copied()
    }

    /// Tokens up to the end, or to the `)` closing a nested list.
    fn list(&mut self, nested: bool) -> Result<Vec<Tok>, CmdError> {
        let mut out = Vec::new();
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.at += 1;
            }
            let Some(c) = self.peek() else {
                if nested {
                    return Err(Diag::new("CPD0014").into());
                }
                return Ok(out);
            };
            match c {
                ')' => {
                    if nested {
                        self.at += 1;
                        return Ok(out);
                    }
                    return Err(Diag::new("CPD0014").into());
                }
                '(' => {
                    self.at += 1;
                    let inner = self.list(true)?;
                    out.push(Tok::Paren(inner));
                }
                '\'' => out.push(Tok::Str(self.quoted()?)),
                '&' if self.peek2().is_some_and(is_name_char) => {
                    self.at += 1;
                    let w = self.bare();
                    out.push(Tok::Var(w.to_ascii_uppercase()));
                }
                '%' if self.peek2().is_some_and(is_name_char) => {
                    self.at += 1;
                    let w = self.bare().to_ascii_uppercase();
                    if self.peek() == Some('(') {
                        self.at += 1;
                        let args = self.list(true)?;
                        out.push(Tok::Builtin { name: w, args });
                    } else {
                        out.push(Tok::Word(format!("%{w}")));
                    }
                }
                '|' => {
                    self.at += 1;
                    let op = match self.peek() {
                        Some('|') => "*CAT",
                        Some('<') => "*BCAT",
                        Some('>') => "*TCAT",
                        _ => return Err(Diag::new("CPD0019").data(&[("|", 1)]).into()),
                    };
                    self.at += 1;
                    out.push(Tok::Op(op));
                }
                '+' | '-' | '*' | '/' if self.standalone() => {
                    self.at += 1;
                    out.push(Tok::Op(match c {
                        '+' => "+",
                        '-' => "-",
                        '*' => "*",
                        _ => "/",
                    }));
                }
                '-' | '+'
                    if self
                        .peek2()
                        .is_some_and(|d| d.is_ascii_digit() || d == self.opts.point()) =>
                {
                    let w = self.bare();
                    out.push(self.number_or_word(w));
                }
                _ => {
                    if (c == 'X' || c == 'x') && self.peek2() == Some('\'') {
                        self.at += 1;
                        let text = self.quoted()?;
                        out.push(Tok::Hex(hex_bytes(&text)?));
                        continue;
                    }
                    let w = self.bare();
                    if w.is_empty() {
                        // A character no token starts with.
                        self.at += 1;
                        return Err(Diag::new("CPD0019").data(&[(&c.to_string(), 1)]).into());
                    }
                    if self.peek() == Some('(') {
                        self.at += 1;
                        let args = self.list(true)?;
                        let (pfx, name) = split_prefix(&w);
                        out.push(Tok::Kw {
                            name: name.to_ascii_uppercase(),
                            pfx,
                            args,
                        });
                    } else {
                        out.push(self.number_or_word(w));
                    }
                }
            }
        }
    }

    /// Whether the operator character at `at` stands alone: followed by
    /// a blank, a parenthesis or the end.
    fn standalone(&self) -> bool {
        match self.peek2() {
            None => true,
            Some(c) => c.is_whitespace() || c == '(' || c == ')',
        }
    }

    /// A bare word: up to a blank, a parenthesis or a quote.
    fn bare(&mut self) -> String {
        let start = self.at;
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == '(' || c == ')' || c == '\'' {
                break;
            }
            self.at += 1;
        }
        self.chars[start..self.at].iter().collect()
    }

    fn number_or_word(&self, w: String) -> Tok {
        if is_number(&w, self.opts.point()) {
            Tok::Num(w)
        } else {
            Tok::Word(w.to_ascii_uppercase())
        }
    }

    /// The text of a quoted string at `at`, `''` collapsed.
    fn quoted(&mut self) -> Result<String, CmdError> {
        debug_assert_eq!(self.peek(), Some('\''));
        self.at += 1;
        let mut s = String::new();
        loop {
            match self.peek() {
                None => return Err(Diag::new("CPD0013").into()),
                Some('\'') => {
                    self.at += 1;
                    if self.peek() == Some('\'') {
                        s.push('\'');
                        self.at += 1;
                    } else {
                        return Ok(s);
                    }
                }
                Some(c) => {
                    s.push(c);
                    self.at += 1;
                }
            }
        }
    }
}

/// A character of a name after its first.
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '#' | '@' | '.')
}

/// A `?x` selective-prompt prefix before a keyword.
fn split_prefix(w: &str) -> (Option<Sel>, &str) {
    let b = w.as_bytes();
    if b.len() >= 2 && b[0] == b'?' {
        let sel = match b[1] {
            b'?' => Some(Sel::Prompt),
            b'*' => Some(Sel::Show),
            b'-' => Some(Sel::Omit),
            b'<' => Some(Sel::Default),
            b'&' => Some(Sel::Hidden),
            _ => None,
        };
        if sel.is_some() {
            return (sel, &w[2..]);
        }
    }
    (None, w)
}

/// Whether a word is a number: an optional sign, digits, at most one
/// decimal point, at least one digit.
#[must_use]
pub fn is_number(w: &str, point: char) -> bool {
    let w = w.strip_prefix(['+', '-']).unwrap_or(w);
    let mut digits = 0;
    let mut points = 0;
    for c in w.chars() {
        if c.is_ascii_digit() {
            digits += 1;
        } else if c == point {
            points += 1;
        } else {
            return false;
        }
    }
    digits > 0 && points <= 1
}

/// The bytes of a hex string's digits (blanks allowed between pairs).
pub fn hex_bytes(text: &str) -> Result<Vec<u8>, CmdError> {
    let clean: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    if !clean.len().is_multiple_of(2) || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Diag::new("CPD0017").into());
    }
    Ok((0..clean.len() / 2)
        .map(|i| u8::from_str_radix(&clean[2 * i..2 * i + 2], 16).expect("hex"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Tokens {
        tokenize(s, Options::default()).unwrap()
    }

    #[test]
    fn command_with_keywords() {
        let c = t("CRTPF FILE(QGPL/F) SRCFILE(QGPL/QDDSSRC) TEXT('it''s (x)') SIZE(100 10 3)");
        assert_eq!(c.name, "CRTPF");
        assert_eq!(c.lib, None);
        assert_eq!(c.toks.len(), 4);
        match &c.toks[2] {
            Tok::Kw { name, args, .. } => {
                assert_eq!(name, "TEXT");
                assert_eq!(args, &vec![Tok::Str("it's (x)".into())]);
            }
            other => panic!("{other:?}"),
        }
        match &c.toks[3] {
            Tok::Kw { args, .. } => assert_eq!(args.len(), 3),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn positional_qualified_and_label() {
        let c = t("LOOP:  qsys/dltlib testlib *n x'c1c2' -3.5 +7");
        assert_eq!(c.label.as_deref(), Some("LOOP"));
        assert_eq!(c.lib.as_deref(), Some("QSYS"));
        assert_eq!(c.name, "DLTLIB");
        assert_eq!(c.toks[0], Tok::Word("TESTLIB".into()));
        assert_eq!(c.toks[1], Tok::Word("*N".into()));
        assert_eq!(c.toks[2], Tok::Hex(vec![0xC1, 0xC2]));
        assert_eq!(c.toks[3], Tok::Num("-3.5".into()));
        assert_eq!(c.toks[4], Tok::Num("+7".into()));
    }

    #[test]
    fn selective_prompting() {
        let c = t("?CRTLIB ??LIB(X) ?*TYPE(*PROD) ?-TEXT ?<AUT(*ALL) ?&CRTAUT(*CHANGE)");
        assert!(c.prompt);
        assert_eq!(c.name, "CRTLIB");
        let pfx: Vec<Option<Sel>> = c
            .toks
            .iter()
            .map(|t| match t {
                Tok::Kw { pfx, .. } => *pfx,
                Tok::Word(w) if w.starts_with("?-") => Some(Sel::Omit),
                _ => None,
            })
            .collect();
        assert_eq!(
            pfx,
            vec![
                Some(Sel::Prompt),
                Some(Sel::Show),
                Some(Sel::Omit),
                Some(Sel::Default),
                Some(Sel::Hidden)
            ]
        );
    }

    #[test]
    fn expressions_and_builtins() {
        let c = t("CHGVAR VAR(&A) VALUE(&B *CAT 'x' || %SST(&C 1 2) + (3 * 4) / 2 - -1)");
        let Tok::Kw { args, .. } = &c.toks[1] else {
            panic!()
        };
        assert_eq!(args[0], Tok::Var("B".into()));
        assert_eq!(args[1], Tok::Word("*CAT".into()));
        assert_eq!(args[3], Tok::Op("*CAT"));
        assert!(
            matches!(&args[4], Tok::Builtin { name, args } if name == "SST" && args.len() == 3)
        );
        assert_eq!(args[5], Tok::Op("+"));
        assert_eq!(
            args[6],
            Tok::Paren(vec![
                Tok::Num("3".into()),
                Tok::Op("*"),
                Tok::Num("4".into())
            ])
        );
        assert_eq!(args[7], Tok::Op("/"));
        assert_eq!(args[9], Tok::Op("-"));
        assert_eq!(args[10], Tok::Num("-1".into()));
    }

    #[test]
    fn errors() {
        let e = tokenize("CRTLIB LIB(X", Options::default()).unwrap_err();
        assert_eq!(e.msgid(), "CPD0014");
        let e = tokenize("CRTLIB TEXT('x)", Options::default()).unwrap_err();
        assert_eq!(e.msgid(), "CPD0013");
        let e = tokenize("CRTLIB X'1'", Options::default()).unwrap_err();
        assert_eq!(e.msgid(), "CPD0017");
        assert_eq!(t("").name, "");
    }

    #[test]
    fn text_round_trip() {
        let c = t("CPYF FROMFILE(A/B) TOFILE(*PRINT) INCREL((*IF F1 *EQ 'x''y') (*AND F2 *GT 5))");
        assert_eq!(
            Tok::join(&c.toks),
            "FROMFILE(A/B) TOFILE(*PRINT) INCREL((*IF F1 *EQ 'x''y') (*AND F2 *GT 5))"
        );
    }

    #[test]
    fn comma_decimal_point() {
        let o = Options { decfmt: 'I' };
        let c = tokenize("X 1,5", o).unwrap();
        assert_eq!(c.toks[0], Tok::Num("1,5".into()));
    }
}
