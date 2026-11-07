// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `application/x-www-form-urlencoded` fields and query strings.

use std::fmt::Write as _;

/// Decoded name/value pairs, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Form {
    pairs: Vec<(String, String)>,
}

impl Form {
    /// From decoded pairs.
    pub fn new(pairs: Vec<(String, String)>) -> Self {
        Self { pairs }
    }

    /// The first value of `name`.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The first value of `name`, or "".
    pub fn str(&self, name: &str) -> &str {
        self.get(name).unwrap_or("")
    }

    /// Every value of `name`.
    pub fn all(&self, name: &str) -> Vec<&str> {
        self.pairs
            .iter()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// Whether `name` was sent (a pressed button, a checked box).
    pub fn has(&self, name: &str) -> bool {
        self.pairs.iter().any(|(k, _)| k == name)
    }

    /// Every pair.
    pub fn pairs(&self) -> &[(String, String)] {
        &self.pairs
    }
}

/// Split and decode `a=1&b=x+y`.
pub fn parse_pairs(s: &str) -> Vec<(String, String)> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (decode(k, true), decode(v, true))
        })
        .collect()
}

/// Percent-decode `s`; `plus` turns `+` into a space (forms, not paths).
#[allow(clippy::many_single_char_names)]
pub fn decode(s: &str, plus: bool) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => match (hex(b[i + 1]), hex(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h << 4 | l);
                    i += 3;
                    continue;
                }
                _ => out.push(b'%'),
            },
            b'+' if plus => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Percent-encode a query value.
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' | b'*' => {
                out.push(b as char);
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_and_decoding() {
        let f = Form::new(parse_pairs("name=WEB+STORE&x=%2Fwww%2F&x=2&flag&bad=%zz%"));
        assert_eq!(f.get("name"), Some("WEB STORE"));
        assert_eq!(f.all("x"), vec!["/www/", "2"]);
        assert!(f.has("flag"));
        assert_eq!(f.str("bad"), "%zz%");
        assert_eq!(decode("a+b%20c", false), "a+b c");
        assert_eq!(encode("/www/a b&*SYSTEM"), "/www/a%20b%26*SYSTEM");
    }
}
