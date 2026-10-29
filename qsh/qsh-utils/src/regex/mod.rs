// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! POSIX regular expressions, basic and extended, over bytes: anchors,
//! bracket expressions with classes, groups, back-references, `*`,
//! `+`, `?`, intervals and alternation. A pattern compiles to a small
//! program run by a backtracking matcher that explores every path and
//! keeps the leftmost-longest match; without back-references a visited
//! bitmap keeps the work linear in the text and the program.

pub mod parse;

use parse::{DUP_MAX, Node, Set, set_has};

/// Which grammar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Syntax {
    /// Basic regular expressions (`grep`, `sed`).
    Basic,
    /// Extended regular expressions (`grep -E`).
    Extended,
}

/// Why a pattern is not a regular expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    /// A bracket or parenthesis is not closed or matched.
    #[error("The pattern has an unmatched {0}.")]
    Unmatched(char),
    /// A pattern ends in a backslash.
    #[error("The pattern ends in a backslash.")]
    TrailingBackslash,
    /// `\{` without a valid interval.
    #[error("The interval expression is not valid.")]
    BadInterval,
    /// A back-reference to a group not yet closed.
    #[error("The back-reference is not valid.")]
    BadBackref,
    /// An unknown class name or a bad collating element.
    #[error("The character class is not valid.")]
    BadClass,
    /// A range whose end is before its start.
    #[error("The range in the bracket expression is not valid.")]
    BadRange,
    /// A repetition of an anchor.
    #[error("The repetition is not valid here.")]
    BadRepeat,
}

#[derive(Clone, Debug)]
enum Inst {
    Byte(u8),
    Any,
    Set(usize),
    Bol,
    Eol,
    Split(usize, usize),
    Jmp(usize),
    Save(usize),
    Backref(usize),
    Mark(usize),
    Check(usize),
    Match,
}

/// A compiled regular expression.
#[derive(Clone, Debug)]
pub struct Regex {
    prog: Vec<Inst>,
    sets: Vec<Set>,
    groups: usize,
    marks: usize,
    backrefs: bool,
    icase: bool,
}

/// Where a pattern matched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    /// Start offset.
    pub start: usize,
    /// End offset (exclusive).
    pub end: usize,
    /// Each group's extent, `groups[0]` the whole match.
    pub groups: Vec<Option<(usize, usize)>>,
}

impl Match {
    /// Group `n`'s bytes of `text`.
    pub fn group<'t>(&self, text: &'t [u8], n: usize) -> Option<&'t [u8]> {
        self.groups
            .get(n)
            .copied()
            .flatten()
            .map(|(s, e)| &text[s..e])
    }
}

struct Compiler {
    prog: Vec<Inst>,
    sets: Vec<Set>,
    marks: usize,
    backrefs: bool,
}

impl Compiler {
    fn emit(&mut self, i: Inst) -> usize {
        self.prog.push(i);
        self.prog.len() - 1
    }

    fn node(&mut self, n: &Node) {
        match n {
            Node::Empty => {}
            Node::Byte(b) => {
                self.emit(Inst::Byte(*b));
            }
            Node::Any => {
                self.emit(Inst::Any);
            }
            Node::Set(s) => {
                let idx = match self.sets.iter().position(|x| x == s) {
                    Some(i) => i,
                    None => {
                        self.sets.push(*s);
                        self.sets.len() - 1
                    }
                };
                self.emit(Inst::Set(idx));
            }
            Node::Bol => {
                self.emit(Inst::Bol);
            }
            Node::Eol => {
                self.emit(Inst::Eol);
            }
            Node::Group(g, inner) => {
                self.emit(Inst::Save(2 * g));
                self.node(inner);
                self.emit(Inst::Save(2 * g + 1));
            }
            Node::Backref(g) => {
                self.backrefs = true;
                self.emit(Inst::Backref(*g));
            }
            Node::Concat(items) => {
                for i in items {
                    self.node(i);
                }
            }
            Node::Alt(alts) => {
                let mut jumps = Vec::new();
                for (k, a) in alts.iter().enumerate() {
                    if k + 1 < alts.len() {
                        let split = self.emit(Inst::Split(0, 0));
                        self.node(a);
                        jumps.push(self.emit(Inst::Jmp(0)));
                        let next = self.prog.len();
                        self.prog[split] = Inst::Split(split + 1, next);
                    } else {
                        self.node(a);
                    }
                }
                let end = self.prog.len();
                for j in jumps {
                    self.prog[j] = Inst::Jmp(end);
                }
            }
            Node::Repeat(inner, min, max) => {
                for _ in 0..*min {
                    self.node(inner);
                }
                match max {
                    None => {
                        let mark = self.marks;
                        self.marks += 1;
                        let split = self.emit(Inst::Split(0, 0));
                        self.emit(Inst::Mark(mark));
                        self.node(inner);
                        self.emit(Inst::Check(mark));
                        self.emit(Inst::Jmp(split));
                        let end = self.prog.len();
                        self.prog[split] = Inst::Split(split + 1, end);
                    }
                    Some(max) => {
                        let optional = max.saturating_sub(*min).min(DUP_MAX);
                        let mut splits = Vec::new();
                        for _ in 0..optional {
                            splits.push(self.emit(Inst::Split(0, 0)));
                            self.node(inner);
                        }
                        let end = self.prog.len();
                        for s in splits {
                            self.prog[s] = Inst::Split(s + 1, end);
                        }
                    }
                }
            }
        }
    }
}

enum Job {
    Try(usize, usize),
    Cap(usize, Option<usize>),
    Mark(usize, usize),
}

impl Regex {
    /// Compile a pattern.
    pub fn new(pat: &[u8], syntax: Syntax, icase: bool) -> Result<Self, Error> {
        let (node, groups) = parse::parse(pat, syntax, icase)?;
        let mut c = Compiler {
            prog: Vec::new(),
            sets: Vec::new(),
            marks: 0,
            backrefs: false,
        };
        c.emit(Inst::Save(0));
        c.node(&node);
        c.emit(Inst::Save(1));
        c.emit(Inst::Match);
        Ok(Self {
            prog: c.prog,
            sets: c.sets,
            groups,
            marks: c.marks,
            backrefs: c.backrefs,
            icase,
        })
    }

    /// A fixed string as a pattern (`grep -F`).
    pub fn literal(s: &[u8], icase: bool) -> Self {
        let mut c = Compiler {
            prog: Vec::new(),
            sets: Vec::new(),
            marks: 0,
            backrefs: false,
        };
        c.emit(Inst::Save(0));
        for &b in s {
            if icase && b.is_ascii_alphabetic() {
                let mut set = Set::default();
                parse::set_add(&mut set, b.to_ascii_lowercase());
                parse::set_add(&mut set, b.to_ascii_uppercase());
                c.node(&Node::Set(set));
            } else {
                c.emit(Inst::Byte(b));
            }
        }
        c.emit(Inst::Save(1));
        c.emit(Inst::Match);
        Self {
            prog: c.prog,
            sets: c.sets,
            groups: 0,
            marks: 0,
            backrefs: false,
            icase,
        }
    }

    /// The number of groups.
    pub fn groups(&self) -> usize {
        self.groups
    }

    /// Does the pattern match anywhere in the text?
    pub fn is_match(&self, text: &[u8]) -> bool {
        self.find_at(text, 0).is_some()
    }

    /// The leftmost-longest match starting at or after `from`. `^`
    /// matches only at offset 0 and `$` only at the end.
    pub fn find_at(&self, text: &[u8], from: usize) -> Option<Match> {
        let mut visited = if self.backrefs {
            Vec::new()
        } else {
            // Positions before `from` are never visited.
            vec![0u64; (self.prog.len() * (text.len().saturating_sub(from) + 1)).div_ceil(64)]
        };
        for start in from..=text.len() {
            if let Some(m) = self.run(text, start, from, &mut visited) {
                return Some(m);
            }
            if self.anchored() {
                break;
            }
        }
        None
    }

    /// Every non-overlapping match, an empty match advancing by one.
    pub fn find_all(&self, text: &[u8]) -> Vec<Match> {
        let mut out = Vec::new();
        let mut pos = 0;
        while pos <= text.len() {
            let Some(m) = self.find_at(text, pos) else {
                break;
            };
            pos = if m.end == m.start { m.end + 1 } else { m.end };
            out.push(m);
        }
        out
    }

    fn anchored(&self) -> bool {
        matches!(self.prog.get(1), Some(Inst::Bol))
    }

    fn eq_byte(&self, a: u8, b: u8) -> bool {
        if self.icase {
            a.eq_ignore_ascii_case(&b)
        } else {
            a == b
        }
    }

    fn run(&self, text: &[u8], start: usize, from: usize, visited: &mut [u64]) -> Option<Match> {
        let ncap = 2 * (self.groups + 1);
        let mut caps: Vec<Option<usize>> = vec![None; ncap];
        let mut marks: Vec<usize> = vec![usize::MAX; self.marks];
        let mut best: Option<Match> = None;
        let mut stack = vec![Job::Try(0, start)];
        let width = text.len() - from + 1;
        while let Some(job) = stack.pop() {
            let (mut pc, mut pos) = match job {
                Job::Try(pc, pos) => (pc, pos),
                Job::Cap(i, old) => {
                    caps[i] = old;
                    continue;
                }
                Job::Mark(i, old) => {
                    marks[i] = old;
                    continue;
                }
            };
            loop {
                if !self.backrefs {
                    let bit = pc * width + (pos - from);
                    let (w, b) = (bit / 64, bit % 64);
                    if visited[w] & (1 << b) != 0 {
                        break;
                    }
                    visited[w] |= 1 << b;
                }
                match &self.prog[pc] {
                    Inst::Byte(b) => {
                        if pos < text.len() && self.eq_byte(text[pos], *b) {
                            pos += 1;
                            pc += 1;
                        } else {
                            break;
                        }
                    }
                    Inst::Any => {
                        if pos < text.len() {
                            pos += 1;
                            pc += 1;
                        } else {
                            break;
                        }
                    }
                    Inst::Set(i) => {
                        if pos < text.len() && set_has(&self.sets[*i], text[pos]) {
                            pos += 1;
                            pc += 1;
                        } else {
                            break;
                        }
                    }
                    Inst::Bol => {
                        if pos == 0 {
                            pc += 1;
                        } else {
                            break;
                        }
                    }
                    Inst::Eol => {
                        if pos == text.len() {
                            pc += 1;
                        } else {
                            break;
                        }
                    }
                    Inst::Split(a, b) => {
                        stack.push(Job::Try(*b, pos));
                        pc = *a;
                    }
                    Inst::Jmp(t) => pc = *t,
                    Inst::Save(i) => {
                        stack.push(Job::Cap(*i, caps[*i]));
                        caps[*i] = Some(pos);
                        pc += 1;
                    }
                    Inst::Backref(g) => {
                        let (Some(s), Some(e)) = (caps[2 * g], caps[2 * g + 1]) else {
                            break;
                        };
                        let len = e - s;
                        if pos + len <= text.len()
                            && (0..len).all(|k| self.eq_byte(text[s + k], text[pos + k]))
                        {
                            pos += len;
                            pc += 1;
                        } else {
                            break;
                        }
                    }
                    Inst::Mark(i) => {
                        if self.backrefs {
                            stack.push(Job::Mark(*i, marks[*i]));
                            marks[*i] = pos;
                        }
                        pc += 1;
                    }
                    Inst::Check(i) => {
                        if self.backrefs && marks[*i] == pos {
                            break;
                        }
                        pc += 1;
                    }
                    Inst::Match => {
                        if best.as_ref().is_none_or(|b| pos > b.end) {
                            let groups = (0..=self.groups)
                                .map(|g| match (caps[2 * g], caps[2 * g + 1]) {
                                    (Some(s), Some(e)) if s <= e => Some((s, e)),
                                    _ => None,
                                })
                                .collect();
                            best = Some(Match {
                                start,
                                end: pos,
                                groups,
                            });
                            if pos == text.len() && !self.backrefs {
                                return best;
                            }
                        }
                        break;
                    }
                }
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(pat: &str, text: &str, syntax: Syntax) -> Option<(usize, usize)> {
        Regex::new(pat.as_bytes(), syntax, false)
            .unwrap()
            .find_at(text.as_bytes(), 0)
            .map(|m| (m.start, m.end))
    }

    #[test]
    fn basics() {
        assert_eq!(find("b", "abc", Syntax::Basic), Some((1, 2)));
        assert_eq!(find("^b", "abc", Syntax::Basic), None);
        assert_eq!(find("c$", "abc", Syntax::Basic), Some((2, 3)));
        assert_eq!(find("a.c", "abc", Syntax::Basic), Some((0, 3)));
        assert_eq!(find("ab*", "abbbc", Syntax::Basic), Some((0, 4)));
        assert_eq!(find("*a", "x*a", Syntax::Basic), Some((1, 3)));
        assert_eq!(find("a\\{2\\}", "aaa", Syntax::Basic), Some((0, 2)));
        assert_eq!(find("\\(ab\\)\\1", "xababy", Syntax::Basic), Some((1, 5)));
        assert_eq!(find("[[:digit:]][[:digit:]]*", "ab12", Syntax::Basic), Some((2, 4)));
    }

    #[test]
    fn extended_and_longest() {
        assert_eq!(find("ab|abcd", "abcd", Syntax::Extended), Some((0, 4)));
        assert_eq!(find("(a|ab)(c|bcd)", "abcd", Syntax::Extended), Some((0, 4)));
        assert_eq!(find("a+", "baaa", Syntax::Extended), Some((1, 4)));
        assert_eq!(find("x?", "abc", Syntax::Extended), Some((0, 0)));
        assert_eq!(find("a{2,3}", "aaaa", Syntax::Extended), Some((0, 3)));
        assert_eq!(find("(a*)*b", "aaab", Syntax::Extended), Some((0, 4)));
        assert_eq!(find("(a*)*", "aaa", Syntax::Extended), Some((0, 3)));
        assert_eq!(find("()", "x", Syntax::Extended), Some((0, 0)));
        assert_eq!(find("^$", "", Syntax::Extended), Some((0, 0)));
        assert_eq!(find("a{,2}", "a{,2}", Syntax::Extended), Some((0, 5)));
    }

    #[test]
    fn groups_and_errors() {
        let r = Regex::new(b"(a+)(b+)?", Syntax::Extended, false).unwrap();
        let m = r.find_at(b"xaab", 0).unwrap();
        assert_eq!(m.group(b"xaab", 1), Some(&b"aa"[..]));
        assert_eq!(m.group(b"xaab", 2), Some(&b"b"[..]));
        let m = r.find_at(b"xaa", 0).unwrap();
        assert_eq!(m.group(b"xaa", 2), None);
        assert_eq!(Regex::new(b"(", Syntax::Extended, false).unwrap_err(), Error::Unmatched('('));
        assert_eq!(Regex::new(b"[a", Syntax::Basic, false).unwrap_err(), Error::Unmatched('['));
        assert_eq!(Regex::new(b"\\1", Syntax::Basic, false).unwrap_err(), Error::BadBackref);
        assert_eq!(Regex::new(b"a\\", Syntax::Basic, false).unwrap_err(), Error::TrailingBackslash);
        assert_eq!(Regex::new(b"[[:nope:]]", Syntax::Basic, false).unwrap_err(), Error::BadClass);
        assert_eq!(Regex::new(b"[z-a]", Syntax::Basic, false).unwrap_err(), Error::BadRange);
    }

    #[test]
    fn brackets_and_case() {
        assert_eq!(find("[]a]", "x]", Syntax::Basic), Some((1, 2)));
        assert_eq!(find("[^a-c]", "abcd", Syntax::Basic), Some((3, 4)));
        assert_eq!(find("[a-]", "-", Syntax::Basic), Some((0, 1)));
        assert_eq!(find("[[.-.]]", "-", Syntax::Basic), Some((0, 1)));
        let r = Regex::new(b"abc", Syntax::Basic, true).unwrap();
        assert!(r.is_match(b"xABCx"));
        let r = Regex::new(b"[a-c]+", Syntax::Extended, true).unwrap();
        assert_eq!(r.find_at(b"xABCx", 0).map(|m| (m.start, m.end)), Some((1, 4)));
        let r = Regex::literal(b"a.b", false);
        assert!(r.is_match(b"xa.b") && !r.is_match(b"axb"));
    }

    #[test]
    fn find_all_advances() {
        let r = Regex::new(b"x*", Syntax::Basic, false).unwrap();
        let v: Vec<(usize, usize)> = r
            .find_all(b"axxb")
            .into_iter()
            .map(|m| (m.start, m.end))
            .collect();
        assert_eq!(v, vec![(0, 0), (1, 3), (3, 3), (4, 4)]);
    }

    #[test]
    fn pathological_is_fast() {
        let pat = "(a*)*(a*)*(a*)*(a*)*b";
        let text = "a".repeat(2000);
        assert_eq!(find(pat, &text, Syntax::Extended), None);
        let pat = "\\(a*\\)*\\1b";
        let text = "a".repeat(10);
        assert_eq!(find(pat, &text, Syntax::Basic), None);
    }
}
