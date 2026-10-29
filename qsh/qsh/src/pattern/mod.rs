// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Shell patterns (`*`, `?`, bracket expressions) for `case`, the trim
//! expansions and pathname expansion ([`path`]). Quoted characters
//! always match themselves.

pub mod path;

/// One pattern character and whether it was quoted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PChar {
    /// The character.
    pub c: char,
    /// Quoted: never special.
    pub quoted: bool,
}

impl PChar {
    /// An unquoted character.
    pub fn plain(c: char) -> Self {
        Self { c, quoted: false }
    }
}

/// Pattern characters from text, all unquoted.
pub fn plain(s: &str) -> Vec<PChar> {
    s.chars().map(PChar::plain).collect()
}

/// Pattern characters from text, all quoted.
pub fn literal(s: &str) -> Vec<PChar> {
    s.chars().map(|c| PChar { c, quoted: true }).collect()
}

/// Does the pattern contain an unquoted `*`, `?` or `[`?
pub fn has_glob(p: &[PChar]) -> bool {
    p.iter()
        .any(|c| !c.quoted && matches!(c.c, '*' | '?' | '['))
}

#[derive(Clone, Debug)]
enum Item {
    Range(char, char),
    Class(String),
    Char(char),
}

#[derive(Clone, Debug)]
enum Tok {
    Lit(char),
    Any,
    Star,
    Set { neg: bool, items: Vec<Item> },
}

fn compile(p: &[PChar]) -> Vec<Tok> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < p.len() {
        let pc = p[i];
        if pc.quoted {
            out.push(Tok::Lit(pc.c));
            i += 1;
            continue;
        }
        match pc.c {
            '*' => {
                if !matches!(out.last(), Some(Tok::Star)) {
                    out.push(Tok::Star);
                }
                i += 1;
            }
            '?' => {
                out.push(Tok::Any);
                i += 1;
            }
            '\\' if i + 1 < p.len() => {
                out.push(Tok::Lit(p[i + 1].c));
                i += 2;
            }
            '[' => {
                if let Some((t, next)) = bracket(p, i) {
                    out.push(t);
                    i = next;
                } else {
                    out.push(Tok::Lit('['));
                    i += 1;
                }
            }
            c => {
                out.push(Tok::Lit(c));
                i += 1;
            }
        }
    }
    out
}

/// A bracket expression starting at `p[start] == '['`: the token and
/// the index after `]`, or `None` when it does not close.
fn bracket(p: &[PChar], start: usize) -> Option<(Tok, usize)> {
    let mut i = start + 1;
    let mut neg = false;
    if i < p.len() && !p[i].quoted && matches!(p[i].c, '!' | '^') {
        neg = true;
        i += 1;
    }
    let mut items = Vec::new();
    let mut first = true;
    loop {
        let pc = *p.get(i)?;
        if pc.c == ']' && !pc.quoted && !first {
            return Some((Tok::Set { neg, items }, i + 1));
        }
        first = false;
        // [:class:]
        if pc.c == '[' && !pc.quoted && p.get(i + 1).is_some_and(|c| c.c == ':' && !c.quoted) {
            let rest: String = p[i + 2..].iter().map(|c| c.c).collect();
            if let Some(end) = rest.find(":]") {
                items.push(Item::Class(rest[..end].to_string()));
                i += 2 + rest[..end].chars().count() + 2;
                continue;
            }
        }
        let mut c = pc.c;
        if c == '\\' && !pc.quoted && i + 1 < p.len() {
            i += 1;
            c = p[i].c;
        }
        if p.get(i + 1).is_some_and(|d| d.c == '-' && !d.quoted)
            && let Some(hi) = p.get(i + 2)
            && (hi.c != ']' || hi.quoted)
        {
            items.push(Item::Range(c, hi.c));
            i += 3;
            continue;
        }
        items.push(Item::Char(c));
        i += 1;
    }
}

fn fold(c: char, icase: bool) -> char {
    if icase {
        c.to_lowercase().next().unwrap_or(c)
    } else {
        c
    }
}

fn class_has(class: &str, c: char) -> bool {
    match class {
        "alpha" => c.is_alphabetic(),
        "digit" => c.is_ascii_digit(),
        "alnum" => c.is_alphanumeric(),
        "upper" => c.is_uppercase(),
        "lower" => c.is_lowercase(),
        "space" => c.is_whitespace(),
        "blank" => c == ' ' || c == '\t',
        "punct" => c.is_ascii_punctuation(),
        "print" => !c.is_control(),
        "graph" => !c.is_control() && !c.is_whitespace(),
        "cntrl" => c.is_control(),
        "xdigit" => c.is_ascii_hexdigit(),
        _ => false,
    }
}

fn set_has(items: &[Item], c: char, icase: bool) -> bool {
    let fc = fold(c, icase);
    items.iter().any(|it| match it {
        Item::Char(x) => fold(*x, icase) == fc,
        Item::Range(a, b) => {
            (*a..=*b).contains(&c) || (icase && (fold(*a, true)..=fold(*b, true)).contains(&fc))
        }
        Item::Class(k) => class_has(k, c),
    })
}

fn tok_matches(t: &Tok, c: char, icase: bool) -> bool {
    match t {
        Tok::Lit(x) => fold(*x, icase) == fold(c, icase),
        Tok::Any => true,
        Tok::Set { neg, items } => set_has(items, c, icase) != *neg,
        Tok::Star => false,
    }
}

fn run(toks: &[Tok], text: &[char], icase: bool) -> bool {
    let (mut t, mut s) = (0usize, 0usize);
    let mut back: Option<(usize, usize)> = None;
    while s < text.len() {
        match toks.get(t) {
            Some(Tok::Star) => {
                back = Some((t, s));
                t += 1;
            }
            Some(tok) if tok_matches(tok, text[s], icase) => {
                t += 1;
                s += 1;
            }
            _ => match back {
                Some((bt, bs)) => {
                    t = bt + 1;
                    s = bs + 1;
                    back = Some((bt, bs + 1));
                }
                None => return false,
            },
        }
    }
    toks[t..].iter().all(|x| matches!(x, Tok::Star))
}

/// Does `text` match the whole pattern?
pub fn matches(p: &[PChar], text: &str, icase: bool) -> bool {
    let chars: Vec<char> = text.chars().collect();
    run(&compile(p), &chars, icase)
}

/// A compiled pattern for repeated matching (the trim expansions).
#[derive(Clone, Debug)]
pub struct Compiled(Vec<Tok>);

impl Compiled {
    /// Compile.
    pub fn new(p: &[PChar]) -> Self {
        Self(compile(p))
    }

    /// Does `text` match?
    pub fn matches(&self, text: &[char]) -> bool {
        run(&self.0, text, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, t: &str) -> bool {
        matches(&plain(p), t, false)
    }

    #[test]
    fn basics() {
        assert!(m("*", "abc"));
        assert!(m("a*c", "abbbc"));
        assert!(!m("a*c", "abcd"));
        assert!(m("a?c", "abc"));
        assert!(m("[a-c]x", "bx"));
        assert!(m("[!a-c]x", "dx"));
        assert!(m("[]]", "]"));
        assert!(m("[[:digit:]]*", "9z"));
        assert!(m("a[", "a["));
        assert!(m("\\*", "*"));
        assert!(!m("\\*", "x"));
        assert!(matches(&literal("*"), "*", false));
        assert!(!matches(&literal("*"), "x", false));
        assert!(matches(&plain("ABC*"), "abcd", true));
    }
}
