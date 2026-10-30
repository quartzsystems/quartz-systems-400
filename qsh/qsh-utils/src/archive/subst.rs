// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `pax -s /old/new/[gp]`: rename members with basic regular
//! expressions; the first expression that matches renames.

use crate::regex::{Regex, Syntax};
use crate::text::sed::parse::{Rep, replacement};

/// One `-s` expression.
pub struct Subst {
    re: Regex,
    rep: Vec<Rep>,
    global: bool,
    /// Report each rename on standard error.
    pub print: bool,
}

/// Parse `/old/new/[gp]` with any delimiter.
pub fn parse(s: &str) -> Result<Subst, String> {
    let bad = || format!("The -s argument {s} is not valid.");
    let b = s.as_bytes();
    let d = *b.first().ok_or_else(bad)?;
    if d == b'\\' || d == b'\n' {
        return Err(bad());
    }
    let mut parts: Vec<Vec<u8>> = vec![Vec::new()];
    let mut i = 1;
    while i < b.len() {
        let c = b[i];
        if c == b'\\' && i + 1 < b.len() && b[i + 1] == d {
            parts.last_mut().expect("part").push(d);
            i += 2;
            continue;
        }
        if c == b'\\' && i + 1 < b.len() {
            let p = parts.last_mut().expect("part");
            p.push(c);
            p.push(b[i + 1]);
            i += 2;
            continue;
        }
        if c == d && parts.len() < 3 {
            parts.push(Vec::new());
        } else {
            parts.last_mut().expect("part").push(c);
        }
        i += 1;
    }
    if parts.len() < 3 {
        return Err(bad());
    }
    let flags = &parts[2];
    let mut global = false;
    let mut print = false;
    for &f in flags {
        match f {
            b'g' => global = true,
            b'p' => print = true,
            _ => return Err(bad()),
        }
    }
    let re = Regex::new(&parts[0], Syntax::Basic, false).map_err(|e| format!("{s}: {e}"))?;
    Ok(Subst {
        re,
        rep: replacement(&parts[1]),
        global,
        print,
    })
}

impl Subst {
    /// The renamed text, or `None` when the expression does not match.
    pub fn apply(&self, text: &str) -> Option<String> {
        let t = text.as_bytes();
        let mut out = Vec::new();
        let mut pos = 0;
        let mut copied = 0;
        let mut done = false;
        while pos <= t.len() {
            let Some(m) = self.re.find_at(t, pos) else {
                break;
            };
            out.extend_from_slice(&t[copied..m.start]);
            for r in &self.rep {
                match r {
                    Rep::Lit(l) => out.extend_from_slice(l),
                    Rep::Group(g) => {
                        if let Some(s) = m.group(t, *g) {
                            out.extend_from_slice(s);
                        }
                    }
                }
            }
            copied = m.end;
            done = true;
            if !self.global {
                break;
            }
            if m.end == m.start {
                if m.end < t.len() {
                    out.push(t[m.end]);
                }
                copied = m.end + 1;
                pos = m.end + 1;
            } else {
                pos = m.end;
            }
        }
        if !done {
            return None;
        }
        if copied < t.len() {
            out.extend_from_slice(&t[copied..]);
        }
        Some(String::from_utf8_lossy(&out).into_owned())
    }
}

/// Rename through a list: the first match wins; no match keeps the
/// name. The second value is the `p` report when one was asked for.
pub fn rename(list: &[Subst], name: &str) -> (String, Option<String>) {
    for s in list {
        if let Some(n) = s.apply(name) {
            let note = s.print.then(|| format!("{name} >> {n}\n"));
            return (n, note);
        }
    }
    (name.to_string(), None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions() {
        let s = parse("/^data/out/").unwrap();
        assert_eq!(s.apply("data/x").as_deref(), Some("out/x"));
        assert_eq!(s.apply("x/data"), None);
        let s = parse(",a,[&],g").unwrap();
        assert_eq!(s.apply("banana").as_deref(), Some("b[a]n[a]n[a]"));
        let s = parse("/\\(.*\\)\\.txt/\\1.bak/").unwrap();
        assert_eq!(s.apply("f.txt").as_deref(), Some("f.bak"));
        assert!(parse("/a/b").is_err());
        assert!(parse("/a/b/q").is_err());
        let s = parse("|/|_|g").unwrap();
        assert_eq!(s.apply("a/b/c").as_deref(), Some("a_b_c"));
    }
}
