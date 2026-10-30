// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the replay compares between the oracle's answer to a command
//! and ours: the reply codes; the replies clients parse (`PWD`, `CWD`,
//! `SYST`, `PASV`); the data: bytes for a binary transfer, lines for a
//! text one, and a listing's lines field by field (`LISTFMT 0`'s
//! columns, `LISTFMT 1`'s words, `NLST`'s names), as sets.

use std::collections::BTreeMap;

use super::transcript::DataIn;

/// A listing line taken apart.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    /// `0`, `1` or `N` (`NLST`).
    form: char,
    owner: String,
    /// `*FILE`, `*MEM` ... in format 0; the mode's type letter in 1.
    kind: String,
    /// The whole mode string (format 1).
    mode: String,
    size: Option<u64>,
    /// The date as shown.
    date: String,
}

/// Whether a listing date is the day the sessions were recorded (what
/// a session made is that new; what was there before is older).
fn recent(date: &str) -> bool {
    date.starts_with("26-10-30") || date.starts_with("Oct 30") || date.starts_with("26-10-29") || date.starts_with("Oct 29")
}

/// One listing line as (name, entry).
fn parse_line(l: &str, nlst: bool) -> (String, Entry) {
    let none = |form: char| Entry {
        form,
        owner: String::new(),
        kind: String::new(),
        mode: String::new(),
        size: None,
        date: String::new(),
    };
    if nlst {
        return (l.to_string(), none('N'));
    }
    let b = l.as_bytes();
    let unix = b.len() > 10
        && matches!(b[0], b'd' | b'-' | b'l')
        && b[1..10].iter().all(|c| matches!(c, b'r' | b'w' | b'x' | b'-' | b's' | b'S' | b't' | b'T'));
    if unix {
        let w: Vec<&str> = l.split_whitespace().collect();
        if w.len() >= 9 {
            // The name is what follows the date's three words.
            let mut rest = l;
            for word in &w[..8] {
                if let Some(at) = rest.find(word) {
                    rest = &rest[at + word.len()..];
                }
            }
            let name = rest.trim_start().to_string();
            return (
                name,
                Entry {
                    form: '1',
                    owner: w[2].to_string(),
                    kind: w[0][..1].to_string(),
                    mode: w[0].to_string(),
                    size: w[4].parse().ok(),
                    date: format!("{} {} {}", w[5], w[6], w[7]),
                },
            );
        }
        return (l.to_string(), none('1'));
    }
    // Format 0 by words: owner, then size, date, time, type, name; a
    // member line has the type right after the owner.
    let w: Vec<&str> = l.split_whitespace().collect();
    let after = |n: usize| -> String {
        let mut rest = l;
        for word in &w[..n] {
            if let Some(at) = rest.find(word) {
                rest = &rest[at + word.len()..];
            }
        }
        rest.trim_start().to_string()
    };
    if w.len() >= 3 && w[1].starts_with('*') {
        return (
            after(2),
            Entry {
                form: '0',
                owner: w[0].to_string(),
                kind: w[1].to_string(),
                mode: String::new(),
                size: None,
                date: String::new(),
            },
        );
    }
    if w.len() >= 6 && w[4].starts_with('*') {
        return (
            after(5),
            Entry {
                form: '0',
                owner: w[0].to_string(),
                kind: w[4].to_string(),
                mode: String::new(),
                size: w[1].parse().ok(),
                date: format!("{} {}", w[2], w[3]),
            },
        );
    }
    (l.to_string(), none('0'))
}

/// A date's shape: digits as `9`, letters as `a`.
fn shape(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_digit() {
                '9'
            } else if c.is_ascii_alphabetic() {
                'a'
            } else {
                c
            }
        })
        .collect()
}

/// Compare two listings; the differences found.
pub fn listing(theirs: &[String], more: usize, ours: &[String], nlst: bool) -> Vec<String> {
    let mut diffs = Vec::new();
    let t: BTreeMap<String, Entry> = theirs.iter().map(|l| parse_line(l, nlst)).collect();
    let o: BTreeMap<String, Entry> = ours.iter().map(|l| parse_line(l, nlst)).collect();
    for (name, te) in &t {
        let Some(oe) = o.get(name) else {
            diffs.push(format!("missing {name:?}"));
            continue;
        };
        if te.form != oe.form {
            diffs.push(format!("{name}: format {} against {}", oe.form, te.form));
            continue;
        }
        if te.owner != oe.owner {
            diffs.push(format!("{name}: owner {} against {}", oe.owner, te.owner));
        }
        if te.kind != oe.kind {
            diffs.push(format!("{name}: type {} against {}", oe.kind, te.kind));
        }
        let is_dir = te.kind == "d" || te.kind == "*DIR" || te.kind == "*FILE" || te.kind == "*LIB";
        if recent(&te.date) && !is_dir && te.size != oe.size {
            diffs.push(format!("{name}: size {:?} against {:?}", oe.size, te.size));
        }
        // What was there before has the fixture's dates; what the session
        // made is dated now on both sides.
        if recent(&te.date) && shape(&te.date) != shape(&oe.date) {
            diffs.push(format!("{name}: date {:?} against {:?}", oe.date, te.date));
        }
    }
    if more == 0 {
        for name in o.keys() {
            if !t.contains_key(name) {
                diffs.push(format!("extra {name:?}"));
            }
        }
    }
    diffs
}

/// Our data as the recorder writes text: Latin-1, split at line feeds,
/// carriage returns off, an empty last line dropped.
pub fn text_lines(data: &[u8]) -> Vec<String> {
    let s: String = data.iter().map(|&b| char::from(b)).collect();
    let mut rows: Vec<String> = s.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
    if rows.last().is_some_and(String::is_empty) {
        rows.pop();
    }
    rows
}

/// Compare what a transfer brought: the differences found.
pub fn data(
    theirs: &DataIn,
    ours: &[u8],
    is_listing: bool,
    nlst: bool,
    ebcdic: bool,
    mask: &dyn Fn(&str) -> String,
) -> Vec<String> {
    match theirs {
        DataIn::Hex(b) => {
            if b.as_slice() == ours {
                Vec::new()
            } else {
                vec![format!(
                    "bytes differ: {} against {}: ours {}",
                    ours.len(),
                    b.len(),
                    hex_head(ours)
                )]
            }
        }
        DataIn::Text { len, lines, more } => {
            let rows: Vec<String> = text_lines(ours).iter().map(|l| mask(l)).collect();
            if is_listing && ebcdic {
                // An EBCDIC listing: both sides decoded, a line at each NL.
                let decode = |rows: &[String]| -> Vec<String> {
                    let bytes: Vec<u8> = rows.join("\n").chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect();
                    let text = ccsid::conv::to_string(37, &bytes).unwrap_or_default();
                    text.split(['\u{85}', '\n']).filter(|l| !l.is_empty()).map(|l| mask(l.trim_end_matches('\r'))).collect()
                };
                return listing(&decode(lines), *more, &decode(&rows), nlst);
            }
            if is_listing {
                return listing(lines, *more, &rows, nlst);
            }
            let mut d = Vec::new();
            if *len != ours.len() {
                d.push(format!("length {} against {len}", ours.len()));
            }
            let n = lines.len().min(60);
            if rows.iter().take(n).ne(lines.iter().take(n)) {
                d.push(format!("text differs: ours {:?}", rows.iter().take(3).collect::<Vec<_>>()));
            }
            d
        }
    }
}

fn hex_head(b: &[u8]) -> String {
    b.iter().take(24).fold(String::new(), |mut s, x| {
        let _ = std::fmt::Write::write_fmt(&mut s, format_args!("{x:02x}"));
        s
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_lines() {
        let (n, e) = parse_line("QSYS           143360 26-10-10 00:10:31 *FILE      ARPA", false);
        assert_eq!((n.as_str(), e.kind.as_str(), e.size), ("ARPA", "*FILE", Some(143_360)));
        let (n, e) = parse_line("-rwx---r-x   1 QSYS     0          14022 Oct 03 2021  ARPA.INET", false);
        assert_eq!((n.as_str(), e.owner.as_str(), e.size), ("ARPA.INET", "QSYS", Some(14022)));
        let (n, _) = parse_line("lrwxrwxrwx   1 QSYS     0             37 Apr 23 2016  ls -> ../x", false);
        assert_eq!(n, "ls -> ../x");
        assert!(listing(&["a".into()], 0, &["a".into()], true).is_empty());
        assert_eq!(listing(&["a".into()], 0, &["b".into()], true).len(), 2);
    }
}
