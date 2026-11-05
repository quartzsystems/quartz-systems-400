// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! JSON values: the parser (RFC 8259 text) and the serializer, compact
//! as IBM i writes it (`{"a":1,"b":[true,null]}`, `/` not escaped,
//! characters past ASCII kept, controls as `\n`, `\t` or `\u00XX`).

use std::fmt::Write as _;

/// A JSON value. Numbers keep their text; objects keep their members in
/// order, duplicates too.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Json {
    /// `null`.
    Null,
    /// `true` or `false`.
    Bool(bool),
    /// A number as written.
    Num(String),
    /// A string.
    Str(String),
    /// An array.
    Array(Vec<Json>),
    /// An object.
    Object(Vec<(String, Json)>),
    /// Text put in as it is (a `FORMAT JSON` value of a constructor).
    Raw(String),
}

/// Why text is not JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadJson {
    /// Where, in characters.
    pub at: usize,
}

struct P<'a> {
    s: &'a [char],
    at: usize,
    depth: u32,
}

/// The deepest nesting parsed.
const MAX_DEPTH: u32 = 512;

impl P<'_> {
    fn ws(&mut self) {
        while self.at < self.s.len() && matches!(self.s[self.at], ' ' | '\t' | '\n' | '\r') {
            self.at += 1;
        }
    }

    fn bad<T>(&self) -> Result<T, BadJson> {
        Err(BadJson { at: self.at })
    }

    fn eat(&mut self, word: &str) -> bool {
        let w: Vec<char> = word.chars().collect();
        if self.s[self.at..].starts_with(&w) {
            self.at += w.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Result<Json, BadJson> {
        self.ws();
        let Some(&c) = self.s.get(self.at) else { return self.bad() };
        match c {
            '{' => {
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return self.bad();
                }
                self.at += 1;
                let mut members = Vec::new();
                self.ws();
                if self.s.get(self.at) == Some(&'}') {
                    self.at += 1;
                    self.depth -= 1;
                    return Ok(Json::Object(members));
                }
                loop {
                    self.ws();
                    if self.s.get(self.at) != Some(&'"') {
                        return self.bad();
                    }
                    let k = self.string()?;
                    self.ws();
                    if self.s.get(self.at) != Some(&':') {
                        return self.bad();
                    }
                    self.at += 1;
                    members.push((k, self.value()?));
                    self.ws();
                    match self.s.get(self.at) {
                        Some(',') => self.at += 1,
                        Some('}') => {
                            self.at += 1;
                            self.depth -= 1;
                            return Ok(Json::Object(members));
                        }
                        _ => return self.bad(),
                    }
                }
            }
            '[' => {
                self.depth += 1;
                if self.depth > MAX_DEPTH {
                    return self.bad();
                }
                self.at += 1;
                let mut items = Vec::new();
                self.ws();
                if self.s.get(self.at) == Some(&']') {
                    self.at += 1;
                    self.depth -= 1;
                    return Ok(Json::Array(items));
                }
                loop {
                    items.push(self.value()?);
                    self.ws();
                    match self.s.get(self.at) {
                        Some(',') => self.at += 1,
                        Some(']') => {
                            self.at += 1;
                            self.depth -= 1;
                            return Ok(Json::Array(items));
                        }
                        _ => return self.bad(),
                    }
                }
            }
            '"' => Ok(Json::Str(self.string()?)),
            't' if self.eat("true") => Ok(Json::Bool(true)),
            'f' if self.eat("false") => Ok(Json::Bool(false)),
            'n' if self.eat("null") => Ok(Json::Null),
            '-' | '0'..='9' => self.number(),
            _ => self.bad(),
        }
    }

    fn digits(&mut self) -> usize {
        let start = self.at;
        while self.s.get(self.at).is_some_and(char::is_ascii_digit) {
            self.at += 1;
        }
        self.at - start
    }

    fn number(&mut self) -> Result<Json, BadJson> {
        let start = self.at;
        if self.s[self.at] == '-' {
            self.at += 1;
        }
        let lead = self.s.get(self.at).copied();
        let n = self.digits();
        if n == 0 || (lead == Some('0') && n > 1) {
            return self.bad();
        }
        if self.s.get(self.at) == Some(&'.') {
            self.at += 1;
            if self.digits() == 0 {
                return self.bad();
            }
        }
        if matches!(self.s.get(self.at), Some('e' | 'E')) {
            self.at += 1;
            if matches!(self.s.get(self.at), Some('+' | '-')) {
                self.at += 1;
            }
            if self.digits() == 0 {
                return self.bad();
            }
        }
        Ok(Json::Num(self.s[start..self.at].iter().collect()))
    }

    fn hex4(&mut self) -> Result<u32, BadJson> {
        let mut v = 0;
        for _ in 0..4 {
            let Some(d) = self.s.get(self.at).and_then(|c| c.to_digit(16)) else { return self.bad() };
            v = v * 16 + d;
            self.at += 1;
        }
        Ok(v)
    }

    fn string(&mut self) -> Result<String, BadJson> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let Some(&c) = self.s.get(self.at) else { return self.bad() };
            self.at += 1;
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let Some(&e) = self.s.get(self.at) else { return self.bad() };
                    self.at += 1;
                    out.push(match e {
                        '"' => '"',
                        '\\' => '\\',
                        '/' => '/',
                        'b' => '\u{8}',
                        'f' => '\u{c}',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xD800..0xDC00).contains(&hi) && self.eat("\\u") {
                                let lo = self.hex4()?;
                                0x10000 + ((hi - 0xD800) << 10) + (lo.wrapping_sub(0xDC00) & 0x3FF)
                            } else {
                                hi
                            };
                            char::from_u32(code).unwrap_or('\u{FFFD}')
                        }
                        _ => return self.bad(),
                    });
                }
                c if u32::from(c) < 0x20 => return self.bad(),
                c => out.push(c),
            }
        }
    }
}

/// Parse JSON text: one value, blanks around it.
pub fn parse(text: &str) -> Result<Json, BadJson> {
    let s: Vec<char> = text.chars().collect();
    let mut p = P { s: &s, at: 0, depth: 0 };
    let v = p.value()?;
    p.ws();
    if p.at != s.len() {
        return p.bad();
    }
    Ok(v)
}

/// A string as JSON: quoted, escaped.
pub fn quote(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl Json {
    /// Compact text.
    #[must_use]
    pub fn text(&self) -> String {
        let mut s = String::new();
        self.write(&mut s, false);
        s
    }

    /// Compact text, with an array's nulls as `BSON_TO_JSON` writes them
    /// (`[true, null ]`) when `bson`.
    pub fn write(&self, out: &mut String, bson: bool) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Num(n) | Self::Raw(n) => out.push_str(n),
            Self::Str(s) => quote(s, out),
            Self::Array(items) => {
                out.push('[');
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    if bson && *v == Self::Null {
                        out.push_str(" null ");
                    } else {
                        v.write(out, bson);
                    }
                }
                out.push(']');
            }
            Self::Object(members) => {
                out.push('{');
                for (i, (k, v)) in members.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    quote(k, out);
                    out.push(':');
                    v.write(out, bson);
                }
                out.push('}');
            }
        }
    }

    /// Whether an object, at any depth, repeats a key.
    #[must_use]
    pub fn has_duplicate_keys(&self) -> bool {
        match self {
            Self::Object(m) => {
                let mut seen = std::collections::HashSet::new();
                m.iter().any(|(k, _)| !seen.insert(k.as_str())) || m.iter().any(|(_, v)| v.has_duplicate_keys())
            }
            Self::Array(items) => items.iter().any(Self::has_duplicate_keys),
            _ => false,
        }
    }

    /// Whether a scalar: not an array or object.
    #[must_use]
    pub fn is_scalar(&self) -> bool {
        !matches!(self, Self::Array(_) | Self::Object(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_writes_compact() {
        let j = parse(r#"{"a" : [ 1 , {"b" :  2} ] }"#).unwrap();
        assert_eq!(j.text(), r#"{"a":[1,{"b":2}]}"#);
        assert_eq!(parse(r#""xéy""#).unwrap(), Json::Str("xéy".into()));
        assert_eq!(parse("1.5e2").unwrap(), Json::Num("1.5e2".into()));
        for bad in ["x", "[1,2", "{\"a\"}", "01", "1.", "\"a\nb\"", "[1] 2", ""] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        let mut s = String::new();
        quote("he said \"hi\" \\ / \n\té€", &mut s);
        assert_eq!(s, r#""he said \"hi\" \\ / \n\té€""#);
        let mut b = String::new();
        parse("[true,null]").unwrap().write(&mut b, true);
        assert_eq!(b, "[true, null ]");
        assert!(parse(r#"{"a":1,"a":2}"#).unwrap().has_duplicate_keys());
    }
}
