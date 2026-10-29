// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Parsing POSIX basic and extended regular expressions into a tree.

use super::{Error, Syntax};

/// Bytes a bracket expression matches.
pub type Set = [u64; 4];

/// Is a byte in a set?
pub fn set_has(s: &Set, b: u8) -> bool {
    s[(b >> 6) as usize] & (1u64 << (b & 63)) != 0
}

/// Add a byte to a set.
pub fn set_add(s: &mut Set, b: u8) {
    s[(b >> 6) as usize] |= 1u64 << (b & 63);
}

/// The tree of a regular expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// Matches the empty string.
    Empty,
    /// One byte.
    Byte(u8),
    /// Any byte.
    Any,
    /// A bracket expression.
    Set(Set),
    /// Start of the text.
    Bol,
    /// End of the text.
    Eol,
    /// A capturing group.
    Group(usize, Box<Node>),
    /// `\n`: what group `n` matched.
    Backref(usize),
    /// Repetition, `None` is unbounded.
    Repeat(Box<Node>, u32, Option<u32>),
    /// A sequence.
    Concat(Vec<Node>),
    /// Alternatives.
    Alt(Vec<Node>),
}

/// The largest repetition count.
pub const DUP_MAX: u32 = 255;

struct Parser<'a> {
    p: &'a [u8],
    i: usize,
    syntax: Syntax,
    icase: bool,
    groups: usize,
    closed: Vec<bool>,
}

/// Parse a pattern: the tree and the number of groups.
pub fn parse(pat: &[u8], syntax: Syntax, icase: bool) -> Result<(Node, usize), Error> {
    let mut p = Parser {
        p: pat,
        i: 0,
        syntax,
        icase,
        groups: 0,
        closed: Vec::new(),
    };
    let node = p.alternation(0)?;
    if p.i < p.p.len() {
        return Err(Error::Unmatched(')'));
    }
    Ok((node, p.groups))
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.p.get(self.i).copied()
    }

    fn peek2(&self) -> Option<u8> {
        self.p.get(self.i + 1).copied()
    }

    fn ere(&self) -> bool {
        self.syntax == Syntax::Extended
    }

    fn at_group_close(&self) -> bool {
        match self.syntax {
            Syntax::Extended => self.peek() == Some(b')'),
            Syntax::Basic => self.peek() == Some(b'\\') && self.peek2() == Some(b')'),
        }
    }

    fn at_alt(&self) -> bool {
        match self.syntax {
            Syntax::Extended => self.peek() == Some(b'|'),
            Syntax::Basic => self.peek() == Some(b'\\') && self.peek2() == Some(b'|'),
        }
    }

    fn alternation(&mut self, depth: usize) -> Result<Node, Error> {
        let mut alts = vec![self.concat(depth)?];
        while self.at_alt() {
            self.i += if self.ere() { 1 } else { 2 };
            alts.push(self.concat(depth)?);
        }
        Ok(if alts.len() == 1 {
            alts.pop().unwrap_or(Node::Empty)
        } else {
            Node::Alt(alts)
        })
    }

    fn concat(&mut self, depth: usize) -> Result<Node, Error> {
        let mut items: Vec<Node> = Vec::new();
        let start = self.i;
        loop {
            if self.i >= self.p.len() || self.at_alt() {
                break;
            }
            if self.at_group_close() {
                if depth == 0 && self.ere() {
                    return Err(Error::Unmatched(')'));
                }
                if depth == 0 {
                    return Err(Error::Unmatched(')'));
                }
                break;
            }
            let at_start = self.i == start;
            // In a BRE a `*` after a leading `^` is literal too.
            let star_lit = at_start || (!self.ere() && items.len() == 1 && items[0] == Node::Bol);
            let atom = self.atom(at_start, star_lit, depth)?;
            let atom = self.quantifiers(atom)?;
            items.push(atom);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap_or(Node::Empty),
            _ => Node::Concat(items),
        })
    }

    fn lit(&self, b: u8) -> Node {
        if self.icase && b.is_ascii_alphabetic() {
            let mut s = Set::default();
            set_add(&mut s, b.to_ascii_lowercase());
            set_add(&mut s, b.to_ascii_uppercase());
            Node::Set(s)
        } else {
            Node::Byte(b)
        }
    }

    fn atom(&mut self, at_start: bool, star_lit: bool, depth: usize) -> Result<Node, Error> {
        let c = self.peek().ok_or(Error::TrailingBackslash)?;
        self.i += 1;
        match c {
            b'.' => Ok(Node::Any),
            b'[' => self.bracket(),
            b'^' => {
                if self.ere() || at_start {
                    Ok(Node::Bol)
                } else {
                    Ok(self.lit(b'^'))
                }
            }
            b'$' => {
                let at_end = self.i >= self.p.len() || self.at_group_close() || self.at_alt();
                if self.ere() || at_end {
                    Ok(Node::Eol)
                } else {
                    Ok(self.lit(b'$'))
                }
            }
            b'*' if star_lit => Ok(self.lit(b'*')),
            b'*' | b'+' | b'?' if self.ere() && at_start => Ok(self.lit(c)),
            b'(' if self.ere() => self.group(depth),
            b')' if self.ere() => Err(Error::Unmatched(')')),
            b'{' if self.ere() && at_start => Ok(self.lit(b'{')),
            b'\\' => {
                let e = self.peek().ok_or(Error::TrailingBackslash)?;
                self.i += 1;
                match e {
                    b'(' if !self.ere() => self.group(depth),
                    b')' if !self.ere() => Err(Error::Unmatched(')')),
                    b'{' if !self.ere() => Err(Error::BadInterval),
                    b'1'..=b'9' => {
                        let n = usize::from(e - b'0');
                        if n > self.groups || !self.closed.get(n - 1).copied().unwrap_or(false) {
                            return Err(Error::BadBackref);
                        }
                        Ok(Node::Backref(n))
                    }
                    b'n' => Ok(Node::Byte(b'\n')),
                    b't' => Ok(Node::Byte(b'\t')),
                    other => Ok(self.lit(other)),
                }
            }
            other => Ok(self.lit(other)),
        }
    }

    fn group(&mut self, depth: usize) -> Result<Node, Error> {
        self.groups += 1;
        let n = self.groups;
        self.closed.push(false);
        let inner = self.alternation(depth + 1)?;
        if !self.at_group_close() {
            return Err(Error::Unmatched('('));
        }
        self.i += if self.ere() { 1 } else { 2 };
        self.closed[n - 1] = true;
        Ok(Node::Group(n, Box::new(inner)))
    }

    fn quantifiers(&mut self, mut atom: Node) -> Result<Node, Error> {
        if !self.ere() && atom == Node::Bol {
            return Ok(atom);
        }
        loop {
            let Some(c) = self.peek() else {
                return Ok(atom);
            };
            let (min, max) = match c {
                b'*' => {
                    self.i += 1;
                    (0, None)
                }
                b'+' if self.ere() => {
                    self.i += 1;
                    (1, None)
                }
                b'?' if self.ere() => {
                    self.i += 1;
                    (0, Some(1))
                }
                b'{' if self.ere() => match self.interval(self.i + 1) {
                    Some((min, max, next)) => {
                        self.i = next;
                        (min, max)
                    }
                    None => return Ok(atom),
                },
                b'\\' if !self.ere() && self.peek2() == Some(b'{') => {
                    let (min, max, next) = self.interval(self.i + 2).ok_or(Error::BadInterval)?;
                    self.i = next;
                    (min, max)
                }
                _ => return Ok(atom),
            };
            if matches!(atom, Node::Bol | Node::Eol) {
                return Err(Error::BadRepeat);
            }
            atom = Node::Repeat(Box::new(atom), min, max);
        }
    }

    /// `m}`, `m,}`, `m,n}` starting at `i`: (min, max, index after).
    fn interval(&self, mut i: usize) -> Option<(u32, Option<u32>, usize)> {
        let num = |i: &mut usize| -> Option<u32> {
            let s = *i;
            while *i < self.p.len() && self.p[*i].is_ascii_digit() {
                *i += 1;
            }
            if s == *i {
                return None;
            }
            std::str::from_utf8(&self.p[s..*i]).ok()?.parse().ok()
        };
        let min = num(&mut i)?;
        let max = if self.p.get(i) == Some(&b',') {
            i += 1;
            if self.p.get(i) == Some(&b'}') || self.p.get(i) == Some(&b'\\') {
                None
            } else {
                Some(num(&mut i)?)
            }
        } else {
            Some(min)
        };
        if self.ere() {
            if self.p.get(i) != Some(&b'}') {
                return None;
            }
            i += 1;
        } else {
            if self.p.get(i) != Some(&b'\\') || self.p.get(i + 1) != Some(&b'}') {
                return None;
            }
            i += 2;
        }
        if min > DUP_MAX || max.is_some_and(|m| m > DUP_MAX || m < min) {
            return None;
        }
        Some((min, max, i))
    }

    fn bracket(&mut self) -> Result<Node, Error> {
        let mut set = Set::default();
        let mut neg = false;
        if self.peek() == Some(b'^') {
            neg = true;
            self.i += 1;
        }
        let mut first = true;
        let mut prev: Option<u8> = None;
        loop {
            let c = self.peek().ok_or(Error::Unmatched('['))?;
            if c == b']' && !first {
                self.i += 1;
                break;
            }
            first = false;
            if c == b'[' && matches!(self.peek2(), Some(b':' | b'.' | b'=')) {
                let kind = self.p[self.i + 1];
                let start = self.i + 2;
                let mut j = start;
                loop {
                    if j + 1 >= self.p.len() {
                        return Err(Error::Unmatched('['));
                    }
                    if self.p[j] == kind && self.p[j + 1] == b']' {
                        break;
                    }
                    j += 1;
                }
                let name = &self.p[start..j];
                self.i = j + 2;
                match kind {
                    b':' => {
                        class(name, &mut set).ok_or(Error::BadClass)?;
                        prev = None;
                    }
                    _ => {
                        if name.len() != 1 {
                            return Err(Error::BadClass);
                        }
                        set_add(&mut set, name[0]);
                        prev = Some(name[0]);
                    }
                }
                continue;
            }
            if c == b'-'
                && let Some(lo) = prev
                && self.peek2().is_some_and(|n| n != b']')
            {
                self.i += 1;
                let hi = self.range_end()?;
                if hi < lo {
                    return Err(Error::BadRange);
                }
                for b in lo..=hi {
                    set_add(&mut set, b);
                }
                prev = None;
                continue;
            }
            self.i += 1;
            set_add(&mut set, c);
            prev = Some(c);
        }
        if self.icase {
            for b in b'a'..=b'z' {
                if set_has(&set, b) || set_has(&set, b.to_ascii_uppercase()) {
                    set_add(&mut set, b);
                    set_add(&mut set, b.to_ascii_uppercase());
                }
            }
        }
        if neg {
            for w in &mut set {
                *w = !*w;
            }
            // A newline never matches a non-matching list in grep and
            // sed usage; keep POSIX: it does match unless REG_NEWLINE.
        }
        Ok(Node::Set(set))
    }

    fn range_end(&mut self) -> Result<u8, Error> {
        let c = self.peek().ok_or(Error::Unmatched('['))?;
        if c == b'[' && matches!(self.peek2(), Some(b'.' | b'=')) {
            let kind = self.p[self.i + 1];
            let b = *self.p.get(self.i + 2).ok_or(Error::Unmatched('['))?;
            if self.p.get(self.i + 3) != Some(&kind) || self.p.get(self.i + 4) != Some(&b']') {
                return Err(Error::BadClass);
            }
            self.i += 5;
            return Ok(b);
        }
        self.i += 1;
        Ok(c)
    }
}

/// Add a character class to a set.
fn class(name: &[u8], set: &mut Set) -> Option<()> {
    let f: fn(u8) -> bool = match name {
        b"alpha" => |b| b.is_ascii_alphabetic(),
        b"digit" => |b| b.is_ascii_digit(),
        b"alnum" => |b| b.is_ascii_alphanumeric(),
        b"upper" => |b| b.is_ascii_uppercase(),
        b"lower" => |b| b.is_ascii_lowercase(),
        b"space" => |b| matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c),
        b"blank" => |b| b == b' ' || b == b'\t',
        b"punct" => |b| b.is_ascii_punctuation(),
        b"print" => |b| (0x20..0x7f).contains(&b),
        b"graph" => |b| (0x21..0x7f).contains(&b),
        b"cntrl" => |b| b < 0x20 || b == 0x7f,
        b"xdigit" => |b| b.is_ascii_hexdigit(),
        _ => return None,
    };
    for b in 0..=255u8 {
        if f(b) {
            set_add(set, b);
        }
    }
    Some(())
}
