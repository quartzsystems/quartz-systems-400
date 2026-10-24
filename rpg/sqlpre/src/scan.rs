// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A statement's text as tokens with their byte spans, enough to find
//! clauses, host variables and indicators; and the edits that turn the
//! text into what the SQL engine runs. Line breaks survive every edit,
//! so a position in the edited text is on the same line as before.

/// What a token is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A word or a number, upper case.
    Word(String),
    /// `'...'`, its value.
    Str(String),
    /// `"..."`.
    Delim(String),
    /// `:name` (`:ds.sub`), the name upper case.
    Host(String),
    /// Anything else, one character.
    Sym(char),
}

/// One token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tok {
    /// What it is.
    pub kind: Kind,
    /// Byte offsets in the text.
    pub start: usize,
    /// End, exclusive.
    pub end: usize,
}

impl Tok {
    /// Whether it is the word `w`.
    #[must_use]
    pub fn is(&self, w: &str) -> bool {
        matches!(&self.kind, Kind::Word(x) if x == w)
    }

    /// Whether it is the symbol `c`.
    #[must_use]
    pub fn sym(&self, c: char) -> bool {
        self.kind == Kind::Sym(c)
    }

    /// The word, if it is one.
    #[must_use]
    pub fn word(&self) -> Option<&str> {
        match &self.kind {
            Kind::Word(w) => Some(w),
            _ => None,
        }
    }
}

fn name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '#' | '$')
}

/// The tokens of a statement; `--` comments are dropped.
#[must_use]
pub fn tokens(text: &str) -> Vec<Tok> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let end_at = |i: usize| chars.get(i).map_or(text.len(), |c| c.0);
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (start, c) = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '-' && chars.get(i + 1).is_some_and(|x| x.1 == '-') {
            while i < chars.len() && chars[i].1 != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '\'' || c == '"' {
            let mut v = String::new();
            i += 1;
            while i < chars.len() {
                if chars[i].1 == c {
                    if chars.get(i + 1).is_some_and(|x| x.1 == c) {
                        v.push(c);
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                v.push(chars[i].1);
                i += 1;
            }
            let kind = if c == '\'' { Kind::Str(v) } else { Kind::Delim(v) };
            out.push(Tok { kind, start, end: end_at(i) });
            continue;
        }
        if c == ':' && chars.get(i + 1).is_some_and(|x| name_char(x.1) && !x.1.is_ascii_digit()) {
            i += 1;
            let mut v = String::new();
            while i < chars.len() && (name_char(chars[i].1) || chars[i].1 == '.' && chars.get(i + 1).is_some_and(|x| name_char(x.1))) {
                v.push(chars[i].1.to_ascii_uppercase());
                i += 1;
            }
            out.push(Tok {
                kind: Kind::Host(v),
                start,
                end: end_at(i),
            });
            continue;
        }
        if name_char(c) {
            let mut v = String::new();
            while i < chars.len() && name_char(chars[i].1) {
                v.push(chars[i].1.to_ascii_uppercase());
                i += 1;
            }
            out.push(Tok {
                kind: Kind::Word(v),
                start,
                end: end_at(i),
            });
            continue;
        }
        out.push(Tok {
            kind: Kind::Sym(c),
            start,
            end: end_at(i + 1),
        });
        i += 1;
    }
    out
}

/// A host variable and its indicator, where they are.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostTok {
    /// The variable, upper case.
    pub name: String,
    /// The indicator variable.
    pub ind: Option<String>,
    /// The first token's index.
    pub first: usize,
    /// One past the last token's index.
    pub next: usize,
}

/// The host variable at token `i`, with an indicator after it
/// (`:v :i`, `:v:i`, `:v INDICATOR :i`).
#[must_use]
pub fn host_at(toks: &[Tok], i: usize) -> Option<HostTok> {
    let Kind::Host(name) = &toks.get(i)?.kind else {
        return None;
    };
    let mut h = HostTok {
        name: name.clone(),
        ind: None,
        first: i,
        next: i + 1,
    };
    match toks.get(i + 1).map(|t| &t.kind) {
        Some(Kind::Host(ind)) => {
            h.ind = Some(ind.clone());
            h.next = i + 2;
        }
        Some(Kind::Word(w)) if w == "INDICATOR" => {
            if let Some(Kind::Host(ind)) = toks.get(i + 2).map(|t| &t.kind) {
                h.ind = Some(ind.clone());
                h.next = i + 3;
            }
        }
        _ => {}
    }
    Some(h)
}

/// The host variables of a list `:a, :b :ib, ...` from token `i` up to
/// `end`.
#[must_use]
pub fn host_list(toks: &[Tok], mut i: usize, end: usize) -> Vec<HostTok> {
    let mut out = Vec::new();
    while i < end {
        if let Some(h) = host_at(toks, i) {
            i = h.next;
            out.push(h);
        } else {
            i += 1;
        }
    }
    out
}

/// The first token at parenthesis depth 0 from `from` that `pred`
/// accepts.
pub fn find_top(toks: &[Tok], from: usize, pred: impl Fn(&Tok) -> bool) -> Option<usize> {
    let mut depth = 0i32;
    for (i, t) in toks.iter().enumerate().skip(from) {
        if t.sym('(') {
            depth += 1;
        } else if t.sym(')') {
            depth -= 1;
        } else if depth == 0 && pred(t) {
            return Some(i);
        }
    }
    None
}

/// Edits to a text: byte ranges replaced, the line breaks they held
/// kept.
#[derive(Clone, Debug, Default)]
pub struct Edits {
    list: Vec<(usize, usize, String)>,
}

impl Edits {
    /// Replace `start..end`.
    pub fn replace(&mut self, start: usize, end: usize, with: &str) {
        self.list.push((start, end, with.to_string()));
    }

    /// The edited text.
    #[must_use]
    pub fn apply(&self, text: &str) -> String {
        self.apply_range(text, 0, text.len())
    }

    /// The edited text of `start..end`, with the edits inside it.
    #[must_use]
    pub fn apply_range(&self, text: &str, start: usize, end: usize) -> String {
        let mut list: Vec<(usize, usize, String)> =
            self.list.iter().filter(|e| e.0 >= start && e.1 <= end).cloned().collect();
        list.sort_by_key(|e| e.0);
        let text = &text[..end];
        let mut out = String::new();
        let mut at = start;
        for (s, e, w) in list {
            if s < at {
                continue;
            }
            out.push_str(&text[at..s]);
            out.push_str(&w);
            for _ in text[s..e].matches('\n') {
                out.push('\n');
            }
            at = e;
        }
        out.push_str(&text[at..]);
        out
    }
}

/// Line breaks in `text` before byte `at`.
#[must_use]
pub fn line_of(text: &str, at: usize) -> u32 {
    u32::try_from(text[..at.min(text.len())].matches('\n').count()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_host_variables_and_indicators() {
        let t = tokens("SELECT A INTO :x :i, :ds.sub INDICATOR :j FROM T WHERE B = 'x:y'");
        let into = find_top(&t, 0, |t| t.is("INTO")).unwrap();
        let from = find_top(&t, into, |t| t.is("FROM")).unwrap();
        let l = host_list(&t, into + 1, from);
        assert_eq!(l.len(), 2);
        assert_eq!((l[0].name.as_str(), l[0].ind.as_deref()), ("X", Some("I")));
        assert_eq!((l[1].name.as_str(), l[1].ind.as_deref()), ("DS.SUB", Some("J")));
        assert!(t.iter().any(|x| x.kind == Kind::Str("x:y".into())));
    }

    #[test]
    fn edits_keep_line_breaks() {
        let mut e = Edits::default();
        e.replace(2, 7, "?");
        assert_eq!(e.apply("a :x\n y b"), "a ?\n b");
    }
}
