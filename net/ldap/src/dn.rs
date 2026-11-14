// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Distinguished names (RFC 4514). Parsing and printing are this
//! module's; normalization against matching rules is `dirsrv`'s.
//!
//! Parsing is lenient where real directories are: spaces around `,`,
//! `+` and `=` (IBM writes `cn=a, dc=b`), `;` as a separator, quoted
//! values (RFC 2253) and any character after `\`. Printing follows
//! RFC 4514 §2.4.

use std::fmt;
use std::str::FromStr;

/// An attribute value assertion in an RDN: `type=value`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ava {
    /// The attribute type as written: a name or a dotted OID.
    pub ty: String,
    /// The value's bytes (escapes and `#hex` decoded).
    pub value: Vec<u8>,
}

/// A relative distinguished name: one or more AVAs (`a=1+b=2`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rdn(pub Vec<Ava>);

/// A distinguished name, most specific RDN first, as written. The empty
/// DN (the root DSE) has no RDNs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Dn(pub Vec<Rdn>);

/// Why a string is not a DN.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid DN at offset {at}: {what}")]
pub struct DnError {
    /// The byte offset in the string.
    pub at: usize,
    /// What was wrong.
    pub what: &'static str,
}

impl Ava {
    /// An AVA of `ty` and the UTF-8 `value`.
    #[must_use]
    pub fn new(ty: &str, value: &str) -> Self {
        Self {
            ty: ty.to_string(),
            value: value.as_bytes().to_vec(),
        }
    }

    /// The value as text, if it is UTF-8.
    #[must_use]
    pub fn value_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.value).ok()
    }

    /// Same type (ASCII case ignored) and the same value bytes.
    #[must_use]
    pub fn raw_eq(&self, other: &Self) -> bool {
        self.ty.eq_ignore_ascii_case(&other.ty) && self.value == other.value
    }
}

impl Rdn {
    /// A single-valued RDN.
    #[must_use]
    pub fn single(ty: &str, value: &str) -> Self {
        Self(vec![Ava::new(ty, value)])
    }

    /// Parses one RDN (`cn=a+sn=b`).
    ///
    /// # Errors
    ///
    /// A [`DnError`] if `s` is not exactly one RDN.
    pub fn parse(s: &str) -> Result<Self, DnError> {
        let dn = Dn::parse(s)?;
        match <[Rdn; 1]>::try_from(dn.0) {
            Ok([r]) => Ok(r),
            Err(_) => Err(DnError {
                at: 0,
                what: "not a single RDN",
            }),
        }
    }

    /// The same AVAs in any order, compared as [`Ava::raw_eq`].
    #[must_use]
    pub fn raw_eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len() && self.0.iter().all(|a| other.0.iter().any(|b| a.raw_eq(b)))
    }
}

impl Dn {
    /// The root DSE's name.
    #[must_use]
    pub const fn root() -> Self {
        Self(Vec::new())
    }

    /// Whether this is the empty DN.
    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }

    /// Parses an RFC 4514 string.
    ///
    /// # Errors
    ///
    /// A [`DnError`] with the offset of the first problem.
    pub fn parse(s: &str) -> Result<Self, DnError> {
        Parser {
            s: s.as_bytes(),
            i: 0,
        }
        .dn()
    }

    /// The most specific RDN.
    #[must_use]
    pub fn rdn(&self) -> Option<&Rdn> {
        self.0.first()
    }

    /// The parent's name; none for the root.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        if self.0.is_empty() {
            None
        } else {
            Some(Self(self.0[1..].to_vec()))
        }
    }

    /// The name of a child: `rdn` before this DN.
    #[must_use]
    pub fn child(&self, rdn: Rdn) -> Self {
        let mut v = Vec::with_capacity(self.0.len() + 1);
        v.push(rdn);
        v.extend(self.0.iter().cloned());
        Self(v)
    }

    /// The number of RDNs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no RDNs (the root).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether this DN is strictly below `base`, on the raw form (types
    /// compared ignoring ASCII case, values byte for byte).
    #[must_use]
    pub fn is_descendant_of(&self, base: &Self) -> bool {
        self.0.len() > base.0.len() && self.ends_with(base)
    }

    /// Whether this DN is `base` or below it, on the raw form.
    #[must_use]
    pub fn is_within(&self, base: &Self) -> bool {
        self.0.len() >= base.0.len() && self.ends_with(base)
    }

    /// Equal on the raw form.
    #[must_use]
    pub fn raw_eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len() && self.ends_with(other)
    }

    fn ends_with(&self, base: &Self) -> bool {
        let skip = self.0.len() - base.0.len();
        self.0[skip..].iter().zip(&base.0).all(|(a, b)| a.raw_eq(b))
    }
}

impl FromStr for Dn {
    type Err = DnError;

    fn from_str(s: &str) -> Result<Self, DnError> {
        Self::parse(s)
    }
}

impl fmt::Display for Ava {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.ty)?;
        f.write_str("=")?;
        f.write_str(&escape_value(&self.value))
    }
}

impl fmt::Display for Rdn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, a) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str("+")?;
            }
            write!(f, "{a}")?;
        }
        Ok(())
    }
}

impl fmt::Display for Dn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, r) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{r}")?;
        }
        Ok(())
    }
}

/// A value escaped as RFC 4514 §2.4 asks: the specials anywhere, a
/// leading space or `#`, a trailing space, NUL and control characters as
/// hex pairs, and bytes that are not UTF-8 as hex pairs.
#[must_use]
pub fn escape_value(v: &[u8]) -> String {
    let mut out = String::with_capacity(v.len());
    let text = String::from_utf8_lossy(v);
    let valid = matches!(text, std::borrow::Cow::Borrowed(_));
    if !valid {
        // Not UTF-8: every byte outside printable ASCII as a hex pair.
        for (i, &b) in v.iter().enumerate() {
            if b.is_ascii_graphic() || b == b' ' {
                push_ascii(&mut out, b as char, i == 0, i + 1 == v.len());
            } else {
                hex_pair(&mut out, b);
            }
        }
        return out;
    }
    let n = text.chars().count();
    for (i, c) in text.chars().enumerate() {
        if c.is_control() {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                hex_pair(&mut out, b);
            }
        } else {
            push_ascii(&mut out, c, i == 0, i + 1 == n);
        }
    }
    out
}

fn push_ascii(out: &mut String, c: char, first: bool, last: bool) {
    let special = matches!(c, '"' | '+' | ',' | ';' | '<' | '>' | '\\')
        || (first && (c == ' ' || c == '#'))
        || (last && c == ' ');
    if special {
        out.push('\\');
    }
    out.push(c);
}

fn hex_pair(out: &mut String, b: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push('\\');
    out.push(HEX[usize::from(b >> 4)] as char);
    out.push(HEX[usize::from(b & 15)] as char);
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// The string forms of a `#hex` value: the BER string types decode to
/// their content; anything else stays as the encoded bytes.
fn ber_string(v: Vec<u8>) -> Vec<u8> {
    // UTF8String, NumericString, PrintableString, TeletexString,
    // IA5String, VisibleString, OCTET STRING.
    const TAGS: [u8; 7] = [0x0C, 0x12, 0x13, 0x14, 0x16, 0x1A, 0x04];
    if v.len() < 2 || !TAGS.contains(&v[0]) {
        return v;
    }
    let (len, hdr) = if v[1] < 0x80 {
        (usize::from(v[1]), 2)
    } else {
        let n = usize::from(v[1] & 0x7F);
        if n == 0 || n > 4 || v.len() < 2 + n {
            return v;
        }
        let len = v[2..2 + n]
            .iter()
            .fold(0usize, |a, &b| (a << 8) | usize::from(b));
        (len, 2 + n)
    };
    if hdr + len == v.len() {
        v[hdr..].to_vec()
    } else {
        v
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn err(&self, what: &'static str) -> DnError {
        DnError { at: self.i, what }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn spaces(&mut self) {
        while self.peek() == Some(b' ') {
            self.i += 1;
        }
    }

    fn dn(mut self) -> Result<Dn, DnError> {
        let mut rdns = Vec::new();
        self.spaces();
        if self.peek().is_none() {
            return Ok(Dn::root());
        }
        loop {
            let mut avas = vec![self.ava()?];
            loop {
                match self.peek() {
                    Some(b'+') => {
                        self.i += 1;
                        avas.push(self.ava()?);
                    }
                    Some(b',' | b';') => {
                        self.i += 1;
                        break;
                    }
                    None => {
                        rdns.push(Rdn(avas));
                        return Ok(Dn(rdns));
                    }
                    Some(_) => return Err(self.err("expected ',' or '+'")),
                }
            }
            rdns.push(Rdn(avas));
        }
    }

    fn ava(&mut self) -> Result<Ava, DnError> {
        self.spaces();
        let ty = self.ty()?;
        self.spaces();
        if self.peek() != Some(b'=') {
            return Err(self.err("expected '='"));
        }
        self.i += 1;
        self.spaces();
        let value = match self.peek() {
            Some(b'#') => {
                self.i += 1;
                self.hex_value()?
            }
            Some(b'"') => {
                self.i += 1;
                self.quoted()?
            }
            _ => self.string()?,
        };
        self.spaces();
        Ok(Ava { ty, value })
    }

    fn ty(&mut self) -> Result<String, DnError> {
        let start = self.i;
        let first = self
            .peek()
            .ok_or_else(|| self.err("expected an attribute type"))?;
        if first.is_ascii_digit() {
            // numericoid: number 1*( "." number )
            let mut dots = 0;
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    self.i += 1;
                } else if c == b'.' {
                    if self.s.get(self.i + 1).is_none_or(|d| !d.is_ascii_digit()) {
                        return Err(self.err("bad numeric OID"));
                    }
                    dots += 1;
                    self.i += 1;
                } else {
                    break;
                }
            }
            if dots == 0 {
                return Err(self.err("bad numeric OID"));
            }
        } else if first.is_ascii_alphabetic() {
            while let Some(c) = self.peek() {
                if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' {
                    self.i += 1;
                } else if c == b'.' && self.s[start..self.i].eq_ignore_ascii_case(b"oid") {
                    // RFC 2253's "OID.1.2.3": keep the OID only.
                    self.i += 1;
                    let mut sub = Parser {
                        s: self.s,
                        i: self.i,
                    };
                    let oid = sub.ty()?;
                    self.i = sub.i;
                    return Ok(oid);
                } else {
                    break;
                }
            }
        } else {
            return Err(self.err("expected an attribute type"));
        }
        Ok(String::from_utf8_lossy(&self.s[start..self.i]).into_owned())
    }

    fn hex_value(&mut self) -> Result<Vec<u8>, DnError> {
        let mut v = Vec::new();
        while let Some(h) = self.peek().and_then(hex_digit) {
            let l = self
                .s
                .get(self.i + 1)
                .copied()
                .and_then(hex_digit)
                .ok_or_else(|| DnError {
                    at: self.i + 1,
                    what: "odd hex value",
                })?;
            v.push((h << 4) | l);
            self.i += 2;
        }
        if v.is_empty() {
            return Err(self.err("empty hex value"));
        }
        Ok(ber_string(v))
    }

    fn escape(&mut self, out: &mut Vec<u8>) -> Result<(), DnError> {
        // At the byte after '\'.
        let c = self.peek().ok_or_else(|| self.err("'\\' at the end"))?;
        if let Some(h) = hex_digit(c) {
            let l = self
                .s
                .get(self.i + 1)
                .copied()
                .and_then(hex_digit)
                .ok_or_else(|| self.err("bad hex pair"))?;
            out.push((h << 4) | l);
            self.i += 2;
        } else {
            // Only the specials may be escaped (RFC 4514 section 3:
            // `pair`); `\z` is not a DN, as IBM's server says too.
            if !matches!(
                c,
                b'\\' | b'"' | b'+' | b',' | b';' | b'<' | b'>' | b' ' | b'#' | b'='
            ) {
                return Err(self.err("only a special or a hex pair may follow '\\'"));
            }
            out.push(c);
            self.i += 1;
        }
        Ok(())
    }

    fn string(&mut self) -> Result<Vec<u8>, DnError> {
        let mut out = Vec::new();
        // Length of `out` up to the last byte that was escaped or not a
        // space: unescaped trailing spaces are dropped.
        let mut keep = 0;
        while let Some(c) = self.peek() {
            match c {
                b',' | b';' | b'+' => break,
                b'\\' => {
                    self.i += 1;
                    self.escape(&mut out)?;
                    keep = out.len();
                }
                0 => return Err(self.err("NUL in a value")),
                _ => {
                    out.push(c);
                    self.i += 1;
                    if c != b' ' {
                        keep = out.len();
                    }
                }
            }
        }
        out.truncate(keep);
        Ok(out)
    }

    fn quoted(&mut self) -> Result<Vec<u8>, DnError> {
        let mut out = Vec::new();
        loop {
            match self.peek() {
                None => return Err(self.err("unterminated quoted value")),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    self.escape(&mut out)?;
                }
                Some(c) => {
                    out.push(c);
                    self.i += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Ava, Dn, Rdn};

    fn p(s: &str) -> Dn {
        Dn::parse(s).unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn rfc4514_examples() {
        let d = p("UID=jsmith,DC=example,DC=net");
        assert_eq!(d.len(), 3);
        assert_eq!(d.0[0].0[0].ty, "UID");
        assert_eq!(d.to_string(), "UID=jsmith,DC=example,DC=net");

        let d = p("OU=Sales+CN=J.  Smith,DC=example,DC=net");
        assert_eq!(d.0[0].0.len(), 2);
        assert_eq!(d.0[0].0[1].value, b"J.  Smith");

        let d = p("CN=James \\\"Jim\\\" Smith\\, III,DC=example,DC=net");
        assert_eq!(d.0[0].0[0].value, b"James \"Jim\" Smith, III");
        assert_eq!(
            d.to_string(),
            "CN=James \\\"Jim\\\" Smith\\, III,DC=example,DC=net"
        );

        let d = p("CN=Before\\0dAfter,DC=example,DC=net");
        assert_eq!(d.0[0].0[0].value, b"Before\rAfter");
        assert_eq!(d.to_string(), "CN=Before\\0DAfter,DC=example,DC=net");

        let d = p("1.3.6.1.4.1.1466.0=#04024869");
        assert_eq!(d.0[0].0[0].ty, "1.3.6.1.4.1.1466.0");
        assert_eq!(d.0[0].0[0].value, b"Hi");

        let d = p("CN=Lu\\C4\\8Di\\C4\\87");
        assert_eq!(d.0[0].0[0].value_str(), Some("Lučić"));
        assert_eq!(d.to_string(), "CN=Lučić");
    }

    #[test]
    fn ibm_spacing_and_case() {
        let d = p("cn=Connection Management, cn=Front End, cn=Configuration");
        assert_eq!(d.len(), 3);
        assert_eq!(d.0[1].0[0].value, b"Front End");
        assert_eq!(
            d.to_string(),
            "cn=Connection Management,cn=Front End,cn=Configuration"
        );
        let d = p("OS400-SYS=SYSTEM");
        assert_eq!(d.0[0].0[0].ty, "OS400-SYS");
        let d = p("  cn = a ,  dc = b  ; dc=c ");
        assert_eq!(d.to_string(), "cn=a,dc=b,dc=c");
        assert_eq!(p("cn=a\\ ").0[0].0[0].value, b"a ");
        assert_eq!(p("cn=\"a, b\"").0[0].0[0].value, b"a, b");
        assert_eq!(p("OID.2.5.4.3=x").0[0].0[0].ty, "2.5.4.3");
    }

    #[test]
    fn only_specials_and_hex_pairs_are_escaped() {
        assert!(Dn::parse(r"cn=\zz").is_err());
        assert_eq!(Dn::parse(r"cn=a\,b").unwrap().to_string(), r"cn=a\,b");
        assert_eq!(Dn::parse(r"cn=\61").unwrap().to_string(), "cn=a");
    }

    #[test]
    fn printing_escapes() {
        let d = Dn(vec![Rdn(vec![Ava::new("cn", " #a+b<c>;\\ ")])]);
        let s = d.to_string();
        assert_eq!(s, "cn=\\ #a\\+b\\<c\\>\\;\\\\\\ ");
        assert_eq!(p(&s), d);
        let bin = Dn(vec![Rdn(vec![Ava {
            ty: "x".into(),
            value: vec![0xFF, b'a', 0, b' '],
        }])]);
        let s = bin.to_string();
        assert_eq!(s, "x=\\FFa\\00\\ ");
        assert_eq!(p(&s), bin);
    }

    #[test]
    fn errors() {
        for bad in [
            "cn",
            "=a",
            "cn=a,",
            "cn=a,,dc=b",
            "1.=a",
            "1=a",
            "cn=#",
            "cn=#0",
            "cn=\\",
            "cn=\\4",
            "cn=\"a",
            "cn=a\"x\"b,=",
            "cn=a+",
        ] {
            assert!(Dn::parse(bad).is_err(), "{bad} parsed");
        }
        assert!(p("").is_root());
        assert!(p("   ").is_root());
    }

    #[test]
    fn helpers() {
        let d = p("cn=a,ou=b,dc=c");
        let base = p("OU=b, DC=c");
        assert!(d.is_descendant_of(&base));
        assert!(d.is_within(&base));
        assert!(!base.is_descendant_of(&base));
        assert!(base.is_within(&base));
        assert!(!d.is_descendant_of(&p("ou=B,dc=c")));
        assert!(d.is_descendant_of(&Dn::root()));
        assert_eq!(d.parent().unwrap().to_string(), "ou=b,dc=c");
        assert_eq!(Dn::root().parent(), None);
        assert_eq!(d.rdn().unwrap().to_string(), "cn=a");
        assert!(p("cn=a+sn=b").0[0].raw_eq(&Rdn::parse("SN=b+CN=a").unwrap()));
        assert!(Rdn::parse("cn=a,dc=b").is_err());
        assert_eq!(
            base.child(Rdn::single("cn", "x")).to_string(),
            "cn=x,OU=b,DC=c"
        );
        assert!(p("CN=X,dc=c").raw_eq(&p("cn=X, DC=c")));
    }
}
