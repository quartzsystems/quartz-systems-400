// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Changes to an LDIF file's text that keep everything else as it was:
//! the entries' order, the attributes' order, comments, folding. An
//! attribute's lines are replaced where the first of them was; a new
//! attribute goes after the entry's last line.

use ldap::ldif::{self, AttrValue, Body, Record, Value};

use super::dn_key;

/// One logical line: its physical lines' range and its text unfolded.
struct Logical {
    first: usize,
    end: usize,
    text: String,
}

/// The entries of `lines`: the logical lines of each paragraph that
/// has a `dn:`, comments left out.
fn entries(lines: &[&str]) -> Vec<Vec<Logical>> {
    let mut out = Vec::new();
    let mut cur: Vec<Logical> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        if l.trim().is_empty() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            i += 1;
            continue;
        }
        let first = i;
        let mut text = l.to_string();
        i += 1;
        while i < lines.len() && lines[i].starts_with(' ') {
            text.push_str(&lines[i][1..]);
            i += 1;
        }
        if !text.starts_with('#') {
            cur.push(Logical {
                first,
                end: i,
                text,
            });
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.retain(|e| {
        e.first()
            .is_some_and(|l| attr_of(&l.text).0.eq_ignore_ascii_case("dn"))
    });
    out
}

/// A logical line's attribute name and its value (decoded when
/// base64), as text.
fn attr_of(text: &str) -> (String, String) {
    let Some(colon) = text.find(':') else {
        return (String::new(), String::new());
    };
    let name = text[..colon].trim().to_string();
    let rest = &text[colon + 1..];
    let value = if let Some(b) = rest.strip_prefix(':') {
        ldif::base64_decode(b.trim().as_bytes())
            .map(|v| String::from_utf8_lossy(&v).into_owned())
            .unwrap_or_default()
    } else {
        rest.trim_start().to_string()
    };
    (name, value)
}

/// The lines of `attr: value` for each value, encoded and folded as
/// `ldap::ldif` writes them.
fn value_lines(attr: &str, values: &[&str]) -> Vec<String> {
    let r = Record {
        line: 0,
        dn: String::new(),
        controls: Vec::new(),
        body: Body::Content(
            values
                .iter()
                .map(|v| AttrValue {
                    ty: attr.to_string(),
                    value: Value::Data(v.as_bytes().to_vec()),
                })
                .collect(),
        ),
    };
    ldif::write_one(&r)
        .lines()
        .skip(1)
        .map(str::to_string)
        .collect()
}

/// `text` with attribute `attr` of entry `entry` set to `values` (none
/// removes it). `None` when the file has no such entry.
#[must_use]
pub fn set(text: &str, entry: &str, attr: &str, values: &[&str]) -> Option<String> {
    let crlf = text.contains("\r\n");
    let lines: Vec<&str> = text.lines().collect();
    let key = dn_key(entry);
    let found = entries(&lines)
        .into_iter()
        .find(|e| dn_key(&attr_of(&e[0].text).1) == key)?;
    let mine: Vec<&Logical> = found
        .iter()
        .skip(1)
        .filter(|l| attr_of(&l.text).0.eq_ignore_ascii_case(attr))
        .collect();
    let new = value_lines(attr, values);
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + new.len());
    let (at, drop): (usize, Vec<(usize, usize)>) = match mine.first() {
        Some(f) => (f.first, mine.iter().map(|l| (l.first, l.end)).collect()),
        None => (found.last().map_or(0, |l| l.end), Vec::new()),
    };
    let mut i = 0;
    while i <= lines.len() {
        if i == at {
            out.extend(new.iter().cloned());
        }
        if i == lines.len() {
            break;
        }
        if let Some((_, end)) = drop.iter().find(|(s, _)| *s == i) {
            i = *end;
            continue;
        }
        out.push(lines[i].to_string());
        i += 1;
    }
    let nl = if crlf { "\r\n" } else { "\n" };
    let mut s = out.join(nl);
    if text.ends_with('\n') {
        s.push_str(nl);
    }
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "# a comment\nversion: 1\n\ndn: cn=Configuration\ncn: Configuration\n# kept\nibm-slapdAdminDN: cn=Administrator\nibm-slapdPort: 389\n\ndn: cn=SSL, cn=Configuration\ncn: SSL\nibm-slapdSecurePort: 636\n";

    #[test]
    fn replace_add_remove() {
        let t = set(FILE, "CN=CONFIGURATION", "ibm-slapdPort", &["3389"]).unwrap();
        assert!(t.contains(
            "# kept\nibm-slapdAdminDN: cn=Administrator\nibm-slapdPort: 3389\n\ndn: cn=SSL"
        ));
        let t = set(&t, "cn=Configuration", "ibm-slapdAdminPW", &["{SSHA512}x"]).unwrap();
        assert!(t.contains("ibm-slapdPort: 3389\nibm-slapdAdminPW: {SSHA512}x\n\ndn: cn=SSL"));
        let t = set(&t, "cn=SSL,cn=Configuration", "ibm-slapdSecurity", &["TLS"]).unwrap();
        assert!(t.ends_with("ibm-slapdSecurePort: 636\nibm-slapdSecurity: TLS\n"));
        let t = set(&t, "cn=Configuration", "ibm-slapdAdminPW", &[]).unwrap();
        assert!(!t.contains("AdminPW"));
        assert!(t.starts_with("# a comment\nversion: 1\n"));
        assert!(set(FILE, "cn=nowhere", "a", &["b"]).is_none());
        // A long value is folded, and read back whole.
        let long = "x".repeat(150);
        let t = set(
            FILE,
            "cn=Configuration",
            "ibm-slapdAdminPW",
            &[long.as_str()],
        )
        .unwrap();
        let back = ldap::ldif::parse(t.as_bytes()).unwrap();
        let Body::Content(a) = &back.records[0].body else {
            panic!()
        };
        assert!(
            a.iter()
                .any(|v| v.value == Value::Data(long.as_bytes().to_vec()))
        );
    }
}
