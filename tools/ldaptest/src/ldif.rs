// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LDIF content records (RFC 2849), written and read by hand: what the
//! import and export scenarios generate and compare. Change records are not
//! needed here.

use std::collections::BTreeMap;

use crate::Error;

/// One entry: its DN and attributes in the order met, values as bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// The DN.
    pub dn: String,
    /// Attribute name and values.
    pub attrs: Vec<(String, Vec<Vec<u8>>)>,
}

impl Entry {
    /// An entry named `dn`.
    pub fn new(dn: impl Into<String>) -> Self {
        Entry { dn: dn.into(), attrs: Vec::new() }
    }

    /// Adds a value of `name`.
    pub fn add(&mut self, name: &str, value: impl Into<Vec<u8>>) -> &mut Self {
        let v = value.into();
        if let Some((_, vals)) = self.attrs.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            vals.push(v);
        } else {
            self.attrs.push((name.into(), vec![v]));
        }
        self
    }

    /// The attributes with lower-case names and sorted values: what two
    /// copies of an entry compare by.
    pub fn normal(&self) -> BTreeMap<String, Vec<Vec<u8>>> {
        let mut m: BTreeMap<String, Vec<Vec<u8>>> = BTreeMap::new();
        for (n, vals) in &self.attrs {
            m.entry(n.to_ascii_lowercase()).or_default().extend(vals.iter().cloned());
        }
        for v in m.values_mut() {
            v.sort();
        }
        m
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 of `data`.
pub fn b64_encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |a, (i, b)| a | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(B64[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Bytes of base64 `text` (white space ignored).
pub fn b64_decode(text: &str) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes().filter(|c| !c.is_ascii_whitespace()) {
        if c == b'=' {
            break;
        }
        let v = B64
            .iter()
            .position(|b| *b == c)
            .ok_or_else(|| Error::Decode(format!("base64 character {:?}", c as char)))?;
        acc = acc << 6 | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Ok(out)
}

/// RFC 2849's SAFE-STRING: no NUL, CR, LF, no leading space, colon or
/// less-than, no trailing space, ASCII only.
fn safe(v: &[u8]) -> bool {
    let bad_first = matches!(v.first(), Some(b' ' | b':' | b'<'));
    !bad_first
        && v.last() != Some(&b' ')
        && v.iter().all(|b| *b != 0 && *b != b'\r' && *b != b'\n' && *b < 0x80)
}

/// One `name: value` line, folded at 76 columns.
fn line(name: &str, v: &[u8], out: &mut String) {
    let text = if safe(v) {
        format!("{name}: {}", String::from_utf8_lossy(v))
    } else {
        format!("{name}:: {}", b64_encode(v))
    };
    let mut first = true;
    let mut rest = text.as_str();
    while !rest.is_empty() {
        let room = if first { 76 } else { 75 };
        let mut cut = rest.len().min(room);
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        if !first {
            out.push(' ');
        }
        out.push_str(&rest[..cut]);
        out.push('\n');
        rest = &rest[cut..];
        first = false;
    }
}

/// LDIF text of `entries`, with a `version: 1` line.
pub fn write(entries: &[Entry]) -> String {
    let mut out = String::from("version: 1\n");
    for e in entries {
        out.push('\n');
        line("dn", e.dn.as_bytes(), &mut out);
        for (n, vals) in &e.attrs {
            for v in vals {
                line(n, v, &mut out);
            }
        }
    }
    out
}

/// The entries of LDIF `text`.
pub fn read(text: &str) -> Result<Vec<Entry>, Error> {
    // Unfold first: a line starting with one space continues the previous.
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(cont) = raw.strip_prefix(' ') {
            match lines.last_mut() {
                Some(l) if !l.is_empty() => l.push_str(cont),
                _ => return Err(Error::Decode("continuation with no line before it".into())),
            }
        } else {
            lines.push(raw.to_owned());
        }
    }
    let mut out = Vec::new();
    let mut cur: Option<Entry> = None;
    for l in lines {
        if l.starts_with('#') {
            continue;
        }
        if l.is_empty() {
            out.extend(cur.take());
            continue;
        }
        let (name, value) = split(&l)?;
        if cur.is_none() {
            if name.eq_ignore_ascii_case("version") {
                continue;
            }
            if !name.eq_ignore_ascii_case("dn") {
                return Err(Error::Decode(format!("record starts with {name}, not dn")));
            }
            let dn = String::from_utf8(value).map_err(|_| Error::Decode("DN not UTF-8".into()))?;
            cur = Some(Entry::new(dn));
        } else if let Some(e) = cur.as_mut() {
            e.add(name, value);
        }
    }
    out.extend(cur);
    Ok(out)
}

fn split(l: &str) -> Result<(&str, Vec<u8>), Error> {
    let (name, rest) = l
        .split_once(':')
        .ok_or_else(|| Error::Decode(format!("line without a colon: {l}")))?;
    if let Some(b) = rest.strip_prefix(':') {
        Ok((name, b64_decode(b.trim_start())?))
    } else if rest.starts_with('<') {
        Err(Error::Decode("URL values are not read".into()))
    } else {
        Ok((name, rest.trim_start_matches(' ').as_bytes().to_vec()))
    }
}

/// `n` generated `inetOrgPerson` entries under `parent`, numbered from 0,
/// every tenth with a value that must be base64 (non-ASCII, leading space).
pub fn generate(parent: &str, n: usize) -> Vec<Entry> {
    (0..n)
        .map(|i| {
            let cn = format!("user{i:07}");
            let mut e = Entry::new(format!("cn={cn},{parent}"));
            e.add("objectClass", "top")
                .add("objectClass", "person")
                .add("objectClass", "organizationalPerson")
                .add("objectClass", "inetOrgPerson")
                .add("cn", cn.as_str())
                .add("sn", format!("Surname{}", i % 97))
                .add("uid", format!("u{i}"));
            if i % 10 == 0 {
                e.add("description", format!(" leading space and caf\u{e9} {i}"));
            } else {
                e.add("description", format!("entry {i}"));
            }
            e
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_vectors() {
        // RFC 4648 section 10.
        for (plain, enc) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(b64_encode(plain.as_bytes()), enc);
            assert_eq!(b64_decode(enc).unwrap(), plain.as_bytes());
        }
        assert!(b64_decode("Zm9v!").is_err());
    }

    #[test]
    fn round_trip_with_folding_and_base64() {
        let mut es = generate("ou=t,dc=example", 12);
        es[3].add("description", "x".repeat(200));
        es[4].add("description", vec![0u8, 1, 2, 0xff]);
        let text = write(&es);
        assert!(text.contains("description:: "));
        assert!(text.lines().all(|l| l.len() <= 76));
        let back = read(&text).unwrap();
        assert_eq!(back.len(), 12);
        for (a, b) in es.iter().zip(&back) {
            assert_eq!(a.dn, b.dn);
            assert_eq!(a.normal(), b.normal());
        }
    }

    #[test]
    fn reads_rfc2849_example_shapes() {
        let text = "version: 1\n# a comment\ndn: cn=Barbara Jensen, ou=Product Development, dc=airius, dc=com\nobjectclass: top\nobjectclass: person\ncn: Barbara Jensen\ndescription: A big sailing fan with a long\n  line\n\ndn:: b3U95Za25qWt6YOoLGRjPWV4YW1wbGU=\nou:: 5Za25qWt6YOo\n";
        let es = read(text).unwrap();
        assert_eq!(es.len(), 2);
        assert_eq!(es[0].attrs[1].1[0], b"Barbara Jensen");
        assert_eq!(es[0].attrs[2].1[0], b"A big sailing fan with a long line");
        assert_eq!(es[1].dn, "ou=\u{55b6}\u{696d}\u{90e8},dc=example");
        assert!(read("cn: x\n").is_err());
        assert!(read(" x\n").is_err());
    }
}
