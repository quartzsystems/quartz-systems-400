// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Search filters (RFC 4511 §4.5.1.7, strings RFC 4515): the string
//! form both ways, the BER form both ways, nesting limited to
//! [`MAX_DEPTH`].

use std::fmt;

use crate::ber::{BerError, Reader, Writer, tag};

/// A search filter. Attribute descriptions are kept as written (type
/// and options); values as bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// `(&...)`; empty is absolute true (RFC 4526).
    And(Vec<Filter>),
    /// `(|...)`; empty is absolute false.
    Or(Vec<Filter>),
    /// `(!...)`.
    Not(Box<Filter>),
    /// `(attr=value)`.
    Equality(String, Vec<u8>),
    /// `(attr=ini*any*fin)`.
    Substrings(String, Substrings),
    /// `(attr>=value)`.
    GreaterOrEqual(String, Vec<u8>),
    /// `(attr<=value)`.
    LessOrEqual(String, Vec<u8>),
    /// `(attr=*)`.
    Present(String),
    /// `(attr~=value)`.
    Approx(String, Vec<u8>),
    /// `(attr:dn:rule:=value)`.
    Extensible(Extensible),
}

/// The parts of a substring assertion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Substrings {
    /// The initial part, if any.
    pub initial: Option<Vec<u8>>,
    /// The middle parts, in order.
    pub any: Vec<Vec<u8>>,
    /// The final part, if any.
    pub fin: Option<Vec<u8>>,
}

/// An extensible match assertion.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Extensible {
    /// The matching rule, a name or OID.
    pub rule: Option<String>,
    /// The attribute description.
    pub attr: Option<String>,
    /// The assertion value.
    pub value: Vec<u8>,
    /// Whether the DN's attributes take part (`:dn`).
    pub dn_attributes: bool,
}

/// The deepest nesting of `&`, `|` and `!` a filter may have.
pub const MAX_DEPTH: usize = 64;

/// Why a filter does not parse or decode.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FilterError {
    /// A string filter's syntax.
    #[error("invalid filter at offset {at}: {what}")]
    Syntax {
        /// The byte offset.
        at: usize,
        /// What was wrong.
        what: &'static str,
    },
    /// Nesting past [`MAX_DEPTH`].
    #[error("filter is nested too deeply")]
    TooDeep,
    /// A BER filter that is not well formed.
    #[error("bad BER filter: {0}")]
    Ber(#[from] BerError),
    /// A BER filter with an unknown choice or bad parts.
    #[error("bad BER filter: {0}")]
    Bad(&'static str),
}

mod t {
    use crate::ber::tag::{context, context_constructed};

    pub const AND: u8 = context_constructed(0);
    pub const OR: u8 = context_constructed(1);
    pub const NOT: u8 = context_constructed(2);
    pub const EQ: u8 = context_constructed(3);
    pub const SUB: u8 = context_constructed(4);
    pub const GE: u8 = context_constructed(5);
    pub const LE: u8 = context_constructed(6);
    pub const PRESENT: u8 = context(7);
    pub const APPROX: u8 = context_constructed(8);
    pub const EXT: u8 = context_constructed(9);
}

impl Filter {
    /// `(objectClass=*)`, the filter that matches every entry.
    #[must_use]
    pub fn any() -> Self {
        Self::Present("objectClass".into())
    }

    /// Parses an RFC 4515 string. A filter without the outer parentheses
    /// (`cn=a`) is taken as tools take it.
    ///
    /// # Errors
    ///
    /// A [`FilterError`].
    pub fn parse(s: &str) -> Result<Self, FilterError> {
        let trimmed = s.trim();
        if trimmed.starts_with('(') {
            let lead = s.len() - s.trim_start().len();
            let mut p = Parser {
                s: trimmed.as_bytes(),
                i: 0,
                shift: lead,
            };
            let f = p.filter(0)?;
            if p.i != p.s.len() {
                return Err(p.err("text after the filter"));
            }
            Ok(f)
        } else {
            let wrapped = format!("({trimmed})");
            let mut p = Parser {
                s: wrapped.as_bytes(),
                i: 0,
                shift: 0,
            };
            let f = p.filter(0)?;
            if p.i != p.s.len() {
                return Err(p.err("text after the filter"));
            }
            Ok(f)
        }
    }

    /// Writes the BER form.
    pub fn encode(&self, w: &mut Writer) {
        match self {
            Self::And(v) => w.constructed(t::AND, |w| v.iter().for_each(|f| f.encode(w))),
            Self::Or(v) => w.constructed(t::OR, |w| v.iter().for_each(|f| f.encode(w))),
            Self::Not(f) => w.constructed(t::NOT, |w| f.encode(w)),
            Self::Equality(a, v) => ava(w, t::EQ, a, v),
            Self::GreaterOrEqual(a, v) => ava(w, t::GE, a, v),
            Self::LessOrEqual(a, v) => ava(w, t::LE, a, v),
            Self::Approx(a, v) => ava(w, t::APPROX, a, v),
            Self::Present(a) => w.string(t::PRESENT, a),
            Self::Substrings(a, s) => w.constructed(t::SUB, |w| {
                w.string(tag::OCTET_STRING, a);
                w.constructed(tag::SEQUENCE, |w| {
                    if let Some(i) = &s.initial {
                        w.octets(tag::context(0), i);
                    }
                    for m in &s.any {
                        w.octets(tag::context(1), m);
                    }
                    if let Some(f) = &s.fin {
                        w.octets(tag::context(2), f);
                    }
                });
            }),
            Self::Extensible(e) => w.constructed(t::EXT, |w| {
                if let Some(r) = &e.rule {
                    w.string(tag::context(1), r);
                }
                if let Some(a) = &e.attr {
                    w.string(tag::context(2), a);
                }
                w.octets(tag::context(3), &e.value);
                if e.dn_attributes {
                    w.boolean(tag::context(4), true);
                }
            }),
        }
    }

    /// The BER form.
    #[must_use]
    pub fn to_ber(&self) -> Vec<u8> {
        let mut w = Writer::new();
        self.encode(&mut w);
        w.into_bytes()
    }

    /// Reads one filter from `r`. Its nesting is limited by
    /// [`MAX_DEPTH`], not by the reader's depth.
    ///
    /// # Errors
    ///
    /// A [`FilterError`].
    pub fn decode(r: &mut Reader<'_>) -> Result<Self, FilterError> {
        let (_, _, whole) = r.element()?;
        let mut inner = Reader::with_limits(whole, usize::MAX, MAX_DEPTH + 4);
        let f = decode(&mut inner, 0)?;
        inner.finish()?;
        Ok(f)
    }

    /// Decodes exactly one BER filter.
    ///
    /// # Errors
    ///
    /// A [`FilterError`].
    pub fn from_ber(b: &[u8]) -> Result<Self, FilterError> {
        let mut r = Reader::new(b);
        let f = Self::decode(&mut r)?;
        r.finish()?;
        Ok(f)
    }
}

fn ava(w: &mut Writer, t: u8, a: &str, v: &[u8]) {
    w.constructed(t, |w| {
        w.string(tag::OCTET_STRING, a);
        w.octets(tag::OCTET_STRING, v);
    });
}

fn text(b: &[u8]) -> Result<String, FilterError> {
    String::from_utf8(b.to_vec()).map_err(|_| FilterError::Bad("attribute is not UTF-8"))
}

fn decode_ava(r: &mut Reader<'_>, t: u8) -> Result<(String, Vec<u8>), FilterError> {
    let mut s = r.sequence(t)?;
    let a = text(s.octets(tag::OCTET_STRING)?)?;
    let v = s.octets(tag::OCTET_STRING)?.to_vec();
    s.finish()?;
    Ok((a, v))
}

fn decode(r: &mut Reader<'_>, depth: usize) -> Result<Filter, FilterError> {
    let Some(tg) = r.peek_tag() else {
        return Err(BerError::Missing.into());
    };
    let list = |r: &mut Reader<'_>, t: u8| -> Result<Vec<Filter>, FilterError> {
        if depth >= MAX_DEPTH {
            return Err(FilterError::TooDeep);
        }
        let mut s = r.sequence(t)?;
        let mut v = Vec::new();
        while !s.is_empty() {
            v.push(decode(&mut s, depth + 1)?);
        }
        Ok(v)
    };
    Ok(match tg {
        t::AND => Filter::And(list(r, t::AND)?),
        t::OR => Filter::Or(list(r, t::OR)?),
        t::NOT => {
            let mut v = list(r, t::NOT)?;
            if v.len() != 1 {
                return Err(FilterError::Bad("not takes one filter"));
            }
            Filter::Not(Box::new(v.remove(0)))
        }
        t::EQ => {
            let (a, v) = decode_ava(r, t::EQ)?;
            Filter::Equality(a, v)
        }
        t::GE => {
            let (a, v) = decode_ava(r, t::GE)?;
            Filter::GreaterOrEqual(a, v)
        }
        t::LE => {
            let (a, v) = decode_ava(r, t::LE)?;
            Filter::LessOrEqual(a, v)
        }
        t::APPROX => {
            let (a, v) = decode_ava(r, t::APPROX)?;
            Filter::Approx(a, v)
        }
        t::PRESENT => Filter::Present(text(r.octets(t::PRESENT)?)?),
        t::SUB => {
            let mut s = r.sequence(t::SUB)?;
            let a = text(s.octets(tag::OCTET_STRING)?)?;
            let mut parts = s.sequence(tag::SEQUENCE)?;
            s.finish()?;
            let mut sub = Substrings::default();
            let mut n = 0;
            while !parts.is_empty() {
                let (pt, c, _) = parts.element()?;
                match pt {
                    0x80 if n == 0 => sub.initial = Some(c.to_vec()),
                    0x81 if sub.fin.is_none() => sub.any.push(c.to_vec()),
                    0x82 if sub.fin.is_none() => sub.fin = Some(c.to_vec()),
                    _ => return Err(FilterError::Bad("substrings out of order")),
                }
                n += 1;
            }
            if n == 0 {
                return Err(FilterError::Bad("substrings without parts"));
            }
            Filter::Substrings(a, sub)
        }
        t::EXT => {
            let mut s = r.sequence(t::EXT)?;
            let rule = s.opt_octets(tag::context(1))?.map(text).transpose()?;
            let attr = s.opt_octets(tag::context(2))?.map(text).transpose()?;
            let value = s.octets(tag::context(3))?.to_vec();
            let dn_attributes = if s.at(tag::context(4)) {
                s.boolean(tag::context(4))?
            } else {
                false
            };
            s.finish()?;
            if rule.is_none() && attr.is_none() {
                return Err(FilterError::Bad("extensible match without a rule or type"));
            }
            Filter::Extensible(Extensible {
                rule,
                attr,
                value,
                dn_attributes,
            })
        }
        _ => return Err(FilterError::Bad("unknown filter choice")),
    })
}

/// A value escaped for a string filter: `*`, `(`, `)`, `\`, NUL, other
/// control characters and bytes that are not UTF-8 as `\XX`.
#[must_use]
pub fn escape(v: &[u8]) -> String {
    let mut out = String::with_capacity(v.len());
    let utf8 = std::str::from_utf8(v).ok();
    match utf8 {
        Some(s) => {
            for c in s.chars() {
                if matches!(c, '*' | '(' | ')' | '\\') || c.is_control() {
                    let mut buf = [0u8; 4];
                    for b in c.encode_utf8(&mut buf).bytes() {
                        push_hex(&mut out, b);
                    }
                } else {
                    out.push(c);
                }
            }
        }
        None => {
            for &b in v {
                if (b.is_ascii_graphic() && !matches!(b, b'*' | b'(' | b')' | b'\\')) || b == b' ' {
                    out.push(b as char);
                } else {
                    push_hex(&mut out, b);
                }
            }
        }
    }
    out
}

fn push_hex(out: &mut String, b: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push('\\');
    out.push(HEX[usize::from(b >> 4)] as char);
    out.push(HEX[usize::from(b & 15)] as char);
}

impl fmt::Display for Filter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::And(v) | Self::Or(v) => {
                f.write_str(if matches!(self, Self::And(_)) {
                    "(&"
                } else {
                    "(|"
                })?;
                for x in v {
                    write!(f, "{x}")?;
                }
                f.write_str(")")
            }
            Self::Not(x) => write!(f, "(!{x})"),
            Self::Equality(a, v) => write!(f, "({a}={})", escape(v)),
            Self::GreaterOrEqual(a, v) => write!(f, "({a}>={})", escape(v)),
            Self::LessOrEqual(a, v) => write!(f, "({a}<={})", escape(v)),
            Self::Approx(a, v) => write!(f, "({a}~={})", escape(v)),
            Self::Present(a) => write!(f, "({a}=*)"),
            Self::Substrings(a, s) => {
                write!(f, "({a}=")?;
                if let Some(i) = &s.initial {
                    f.write_str(&escape(i))?;
                }
                f.write_str("*")?;
                for m in &s.any {
                    write!(f, "{}*", escape(m))?;
                }
                if let Some(x) = &s.fin {
                    f.write_str(&escape(x))?;
                }
                f.write_str(")")
            }
            Self::Extensible(e) => {
                f.write_str("(")?;
                if let Some(a) = &e.attr {
                    f.write_str(a)?;
                }
                if e.dn_attributes {
                    f.write_str(":dn")?;
                }
                if let Some(r) = &e.rule {
                    write!(f, ":{r}")?;
                }
                write!(f, ":={})", escape(&e.value))
            }
        }
    }
}

impl std::str::FromStr for Filter {
    type Err = FilterError;

    fn from_str(s: &str) -> Result<Self, FilterError> {
        Self::parse(s)
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    /// Added to offsets in errors (leading spaces trimmed).
    shift: usize,
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn attr_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b';' | b'_')
}

impl Parser<'_> {
    fn err(&self, what: &'static str) -> FilterError {
        FilterError::Syntax {
            at: self.i + self.shift,
            what,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn eat(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn spaces(&mut self) {
        while self.peek() == Some(b' ') {
            self.i += 1;
        }
    }

    fn filter(&mut self, depth: usize) -> Result<Filter, FilterError> {
        if !self.eat(b'(') {
            return Err(self.err("expected '('"));
        }
        let f = match self.peek() {
            Some(b'&') => {
                self.i += 1;
                Filter::And(self.list(depth)?)
            }
            Some(b'|') => {
                self.i += 1;
                Filter::Or(self.list(depth)?)
            }
            Some(b'!') => {
                self.i += 1;
                if depth >= MAX_DEPTH {
                    return Err(FilterError::TooDeep);
                }
                self.spaces();
                let f = self.filter(depth + 1)?;
                self.spaces();
                Filter::Not(Box::new(f))
            }
            _ => self.item()?,
        };
        if !self.eat(b')') {
            return Err(self.err("expected ')'"));
        }
        Ok(f)
    }

    fn list(&mut self, depth: usize) -> Result<Vec<Filter>, FilterError> {
        if depth >= MAX_DEPTH {
            return Err(FilterError::TooDeep);
        }
        let mut v = Vec::new();
        loop {
            self.spaces();
            if self.peek() != Some(b'(') {
                return Ok(v);
            }
            v.push(self.filter(depth + 1)?);
        }
    }

    fn attr(&mut self) -> String {
        let start = self.i;
        while self.peek().is_some_and(attr_char) {
            self.i += 1;
        }
        String::from_utf8_lossy(&self.s[start..self.i]).into_owned()
    }

    /// A value up to the closing `)`, split at unescaped `*`.
    fn value_parts(&mut self) -> Result<Vec<Vec<u8>>, FilterError> {
        let mut parts = vec![Vec::new()];
        loop {
            match self.peek() {
                None => return Err(self.err("expected ')'")),
                Some(b')') => return Ok(parts),
                Some(b'(') => return Err(self.err("'(' in a value must be escaped")),
                Some(b'*') => {
                    self.i += 1;
                    parts.push(Vec::new());
                }
                Some(b'\\') => {
                    self.i += 1;
                    let Some(c) = self.peek() else {
                        return Err(self.err("'\\' at the end"));
                    };
                    let last = parts.last_mut().unwrap_or_else(|| unreachable!());
                    if let Some(h) = hex(c) {
                        let Some(l) = self.s.get(self.i + 1).copied().and_then(hex) else {
                            return Err(self.err("bad escape"));
                        };
                        last.push((h << 4) | l);
                        self.i += 2;
                    } else {
                        // RFC 1960's escapes: the character itself.
                        last.push(c);
                        self.i += 1;
                    }
                }
                Some(c) => {
                    parts.last_mut().unwrap_or_else(|| unreachable!()).push(c);
                    self.i += 1;
                }
            }
        }
    }

    fn single(&mut self) -> Result<Vec<u8>, FilterError> {
        let at = self.i;
        let mut parts = self.value_parts()?;
        if parts.len() != 1 {
            self.i = at;
            return Err(self.err("'*' in a value must be escaped"));
        }
        Ok(parts.remove(0))
    }

    fn item(&mut self) -> Result<Filter, FilterError> {
        let attr = self.attr();
        match self.peek() {
            Some(b':') => return self.extensible(attr),
            _ if attr.is_empty() => return Err(self.err("expected an attribute")),
            Some(b'=') => {}
            Some(b'~' | b'>' | b'<') => {
                let op = self.s[self.i];
                self.i += 1;
                if !self.eat(b'=') {
                    return Err(self.err("expected '='"));
                }
                let v = self.single()?;
                return Ok(match op {
                    b'~' => Filter::Approx(attr, v),
                    b'>' => Filter::GreaterOrEqual(attr, v),
                    _ => Filter::LessOrEqual(attr, v),
                });
            }
            _ => return Err(self.err("expected a filter type")),
        }
        self.i += 1;
        let mut parts = self.value_parts()?;
        if parts.len() == 1 {
            return Ok(Filter::Equality(attr, parts.remove(0)));
        }
        if parts.len() == 2 && parts[0].is_empty() && parts[1].is_empty() {
            return Ok(Filter::Present(attr));
        }
        let fin = parts.pop().filter(|p| !p.is_empty());
        let initial = Some(parts.remove(0)).filter(|p| !p.is_empty());
        if parts.iter().any(Vec::is_empty) {
            return Err(self.err("empty part between '*'s"));
        }
        Ok(Filter::Substrings(
            attr,
            Substrings {
                initial,
                any: parts,
                fin,
            },
        ))
    }

    fn extensible(&mut self, attr: String) -> Result<Filter, FilterError> {
        let mut e = Extensible {
            attr: (!attr.is_empty()).then_some(attr),
            ..Extensible::default()
        };
        loop {
            if !self.eat(b':') {
                return Err(self.err("expected ':'"));
            }
            if self.eat(b'=') {
                break;
            }
            let start = self.i;
            let word = self.attr();
            if word.is_empty() {
                return Err(self.err("expected 'dn' or a matching rule"));
            }
            if word.eq_ignore_ascii_case("dn") && !e.dn_attributes && e.rule.is_none() {
                e.dn_attributes = true;
            } else if e.rule.is_none() {
                e.rule = Some(word);
            } else {
                self.i = start;
                return Err(self.err("expected ':='"));
            }
        }
        if e.attr.is_none() && e.rule.is_none() {
            return Err(self.err("extensible match needs a type or a rule"));
        }
        e.value = self.single()?;
        Ok(Filter::Extensible(e))
    }
}

#[cfg(test)]
mod tests {
    use super::{Extensible, Filter, FilterError, MAX_DEPTH, Substrings};

    fn p(s: &str) -> Filter {
        Filter::parse(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    fn eq(a: &str, v: &[u8]) -> Filter {
        Filter::Equality(a.into(), v.to_vec())
    }

    #[test]
    fn rfc4515_examples() {
        assert_eq!(p("(cn=Babs Jensen)"), eq("cn", b"Babs Jensen"));
        assert_eq!(
            p("(!(cn=Tim Howes))"),
            Filter::Not(Box::new(eq("cn", b"Tim Howes")))
        );
        let f = p("(&(objectClass=Person)(|(sn=Jensen)(cn=Babs J*)))");
        let Filter::And(v) = &f else { panic!() };
        assert_eq!(v[0], eq("objectClass", b"Person"));
        let Filter::Or(o) = &v[1] else { panic!() };
        assert_eq!(
            o[1],
            Filter::Substrings(
                "cn".into(),
                Substrings {
                    initial: Some(b"Babs J".to_vec()),
                    any: vec![],
                    fin: None
                }
            )
        );
        assert_eq!(
            p("(o=univ*of*mich*)"),
            Filter::Substrings(
                "o".into(),
                Substrings {
                    initial: Some(b"univ".to_vec()),
                    any: vec![b"of".to_vec(), b"mich".to_vec()],
                    fin: None
                }
            )
        );
        assert_eq!(p("(seeAlso=)"), eq("seeAlso", b""));
        assert_eq!(
            p("(cn:caseExactMatch:=Fred Flintstone)"),
            Filter::Extensible(Extensible {
                rule: Some("caseExactMatch".into()),
                attr: Some("cn".into()),
                value: b"Fred Flintstone".to_vec(),
                dn_attributes: false
            })
        );
        assert!(matches!(
            p("(cn:=Betty Rubble)"),
            Filter::Extensible(Extensible { rule: None, .. })
        ));
        let Filter::Extensible(e) = p("(sn:dn:2.4.6.8.10:=Barney Rubble)") else {
            panic!()
        };
        assert!(e.dn_attributes);
        assert_eq!(e.rule.as_deref(), Some("2.4.6.8.10"));
        let Filter::Extensible(e) = p("(o:dn:=Ace Industry)") else {
            panic!()
        };
        assert!(e.dn_attributes && e.rule.is_none());
        let Filter::Extensible(e) = p("(:1.2.3:=Wilma Flintstone)") else {
            panic!()
        };
        assert!(e.attr.is_none());
        let Filter::Extensible(e) = p("(:DN:2.4.6.8.10:=Dino)") else {
            panic!()
        };
        assert!(e.dn_attributes && e.attr.is_none());
        assert_eq!(
            p("(o=Parens R Us \\28for all your parenthetical needs\\29)"),
            eq("o", b"Parens R Us (for all your parenthetical needs)")
        );
        assert_eq!(
            p("(cn=*\\2A*)"),
            Filter::Substrings(
                "cn".into(),
                Substrings {
                    initial: None,
                    any: vec![b"*".to_vec()],
                    fin: None
                }
            )
        );
        assert_eq!(p("(filename=C:\\5cMyFile)"), eq("filename", b"C:\\MyFile"));
        assert_eq!(p("(bin=\\00\\00\\00\\04)"), eq("bin", &[0, 0, 0, 4]));
        assert_eq!(p("(sn=Lu\\c4\\8di\\c4\\87)"), eq("sn", "Lučić".as_bytes()));
        assert_eq!(
            p("(1.3.6.1.4.1.1466.0=\\04\\02\\48\\69)"),
            eq("1.3.6.1.4.1.1466.0", &[4, 2, 0x48, 0x69])
        );
    }

    #[test]
    fn strings_round_trip() {
        for s in [
            "(cn=Babs Jensen)",
            "(!(cn=Tim Howes))",
            "(&(objectClass=Person)(|(sn=Jensen)(cn=Babs J*)))",
            "(o=univ*of*mich*)",
            "(cn=*a*b)",
            "(cn=*)",
            "(&)",
            "(|)",
            "(age>=21)",
            "(age<=65)",
            "(sn~=jensen)",
            "(cn:dn:caseExactMatch:=x)",
            "(o=\\28\\29\\2a\\5c\\00)",
            "(cn;lang-en=x)",
        ] {
            let f = p(s);
            assert_eq!(f.to_string(), s);
            assert_eq!(Filter::from_ber(&f.to_ber()).unwrap(), f);
        }
        assert_eq!(p("cn=bare").to_string(), "(cn=bare)");
        assert_eq!(p("(& (a=1) (b=2) )").to_string(), "(&(a=1)(b=2))");
        assert_eq!(p("(x=\\*)"), eq("x", b"*"));
    }

    #[test]
    fn ber_by_hand() {
        // (&(cn=a)(sn=*)) from the ASN.1: [0]{ [3]{ "cn" "a" } [7]"sn" }
        let b = [
            0xA0, 0x0D, 0xA3, 0x07, 0x04, 0x02, b'c', b'n', 0x04, 0x01, b'a', 0x87, 0x02, b's',
            b'n',
        ];
        let f = Filter::from_ber(&b).unwrap();
        assert_eq!(
            f,
            Filter::And(vec![eq("cn", b"a"), Filter::Present("sn".into())])
        );
        assert_eq!(f.to_ber(), b);
        // (cn=a*b) : [4]{ "cn" SEQUENCE{ [0]"a" [2]"b" } }
        let b = [
            0xA4, 0x0C, 0x04, 0x02, b'c', b'n', 0x30, 0x06, 0x80, 0x01, b'a', 0x82, 0x01, b'b',
        ];
        assert_eq!(Filter::from_ber(&b).unwrap(), p("(cn=a*b)"));
        assert_eq!(p("(cn=a*b)").to_ber(), b);
        // (cn:dn:=x) : [9]{ [2]"cn" [3]"x" [4]TRUE }
        let b = [
            0xA9, 0x0A, 0x82, 0x02, b'c', b'n', 0x83, 0x01, b'x', 0x84, 0x01, 0xFF,
        ];
        assert_eq!(Filter::from_ber(&b).unwrap(), p("(cn:dn:=x)"));
        assert_eq!(p("(cn:dn:=x)").to_ber(), b);
        // Bad: final before any, no parts, unknown choice, empty not.
        for bad in [
            &[
                0xA4, 0x0C, 0x04, 0x02, b'c', b'n', 0x30, 0x06, 0x82, 0x01, b'a', 0x81, 0x01, b'b',
            ][..],
            &[0xA4, 0x06, 0x04, 0x02, b'c', b'n', 0x30, 0x00],
            &[0xAA, 0x00],
            &[0xA2, 0x00],
            &[0xA9, 0x03, 0x83, 0x01, b'x'],
        ] {
            assert!(Filter::from_ber(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn depth_limit() {
        let deep = |n: usize| format!("{}(a=b){}", "(!".repeat(n), ")".repeat(n));
        assert!(Filter::parse(&deep(MAX_DEPTH)).is_ok());
        assert_eq!(
            Filter::parse(&deep(MAX_DEPTH + 1)),
            Err(FilterError::TooDeep)
        );
        let ok = Filter::parse(&deep(MAX_DEPTH)).unwrap();
        assert_eq!(Filter::from_ber(&ok.to_ber()).unwrap(), ok);
        let mut b = ok.to_ber();
        for _ in 0..1 {
            let mut w = crate::ber::Writer::new();
            w.octets(0xA2, &b);
            b = w.into_bytes();
        }
        assert_eq!(Filter::from_ber(&b), Err(FilterError::TooDeep));
    }

    #[test]
    fn errors() {
        for bad in [
            "",
            "(",
            "()",
            "(cn)",
            "(cn=a",
            "(cn=a))",
            "(=a)",
            "(cn~a)",
            "(cn>=a*)",
            "(cn=a(b)",
            "(cn=a**b)",
            "(cn=\\4)",
            "(cn:=)x",
            "(:=a)",
            "(cn:a:b:=c)",
            "(!(a=b)(c=d))",
            "(&(a=b)x)",
        ] {
            assert!(Filter::parse(bad).is_err(), "{bad} parsed");
        }
    }
}
