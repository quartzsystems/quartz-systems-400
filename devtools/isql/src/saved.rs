// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A saved session: the attributes, the history and the input lines,
//! as the text the driver stores for the user in the
//! `QUSRSYS/QSQLSESS` user space and reads back at the next `STRSQL`.

use std::fmt::Write as _;

use crate::IsqlError;
use crate::attrs::Attrs;
use crate::history::Entry;

/// The first line of a saved session.
const MAGIC: &str = "QSQLSESS 1";

/// What a saved session holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Saved {
    /// The attributes.
    pub attrs: Attrs,
    /// The history.
    pub entries: Vec<Entry>,
    /// The input lines as left.
    pub input: Vec<String>,
}

/// The session as text: `A` attribute lines, `S` a statement's first
/// line and `C` the next ones, `M` messages, `I` input lines.
#[must_use]
pub fn to_text(s: &Saved) -> String {
    let mut out = format!("{MAGIC}\n");
    for line in s.attrs.to_text().lines() {
        let _ = writeln!(out, "A {line}");
    }
    for e in &s.entries {
        match e {
            Entry::Statement(lines) => {
                if lines.is_empty() {
                    let _ = writeln!(out, "S ");
                }
                for (i, l) in lines.iter().enumerate() {
                    let _ = writeln!(out, "{} {l}", if i == 0 { 'S' } else { 'C' });
                }
            }
            Entry::Message { id, text } => {
                let id = if id.is_empty() { "-" } else { id.as_str() };
                let _ = writeln!(out, "M {id} {text}");
            }
        }
    }
    let mut input = s.input.clone();
    while input.last().is_some_and(|l| l.trim().is_empty()) {
        input.pop();
    }
    for l in input {
        let _ = writeln!(out, "I {l}");
    }
    out
}

/// A session from its text.
///
/// # Errors
/// The text is not a saved session.
pub fn from_text(text: &str) -> Result<Saved, IsqlError> {
    let mut lines = text.lines();
    if lines.next() != Some(MAGIC) {
        return Err(IsqlError::Damaged("no session header".into()));
    }
    let mut attrs = String::new();
    let mut s = Saved::default();
    for l in lines {
        let (tag, rest) = l.split_at(l.len().min(2));
        match tag {
            "A " => {
                attrs.push_str(rest);
                attrs.push('\n');
            }
            "S " => s.entries.push(Entry::Statement(vec![rest.to_string()])),
            "C " => match s.entries.last_mut() {
                Some(Entry::Statement(v)) => v.push(rest.to_string()),
                _ => {
                    return Err(IsqlError::Damaged(
                        "a continuation without a statement".into(),
                    ));
                }
            },
            "M " => {
                let (id, text) = rest.split_once(' ').unwrap_or((rest, ""));
                s.entries.push(Entry::Message {
                    id: if id == "-" {
                        String::new()
                    } else {
                        id.to_string()
                    },
                    text: text.to_string(),
                });
            }
            "I " => s.input.push(rest.to_string()),
            _ if l.is_empty() => {}
            _ => return Err(IsqlError::Damaged(format!("line {l:?}"))),
        }
    }
    s.attrs = Attrs::from_text(&attrs);
    Ok(s)
}

/// The statements of a session as source lines (option 4: save
/// session in source file), each statement's lines with the last
/// ending in `;`.
#[must_use]
pub fn source_lines(entries: &[Entry]) -> Vec<String> {
    let mut out = Vec::new();
    for e in entries {
        if let Entry::Statement(lines) = e {
            let lines: Vec<&String> = lines.iter().filter(|l| !l.trim().is_empty()).collect();
            let n = lines.len();
            for (i, l) in lines.into_iter().enumerate() {
                if i + 1 == n {
                    out.push(format!("{};", l.trim_end().trim_end_matches(';')));
                } else {
                    out.push(l.trim_end().to_string());
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut a = Attrs::default();
        a.set("NAMING", "*SQL").unwrap();
        let s = Saved {
            attrs: a,
            entries: vec![
                Entry::Statement(vec![String::new(), "select *".into(), "from t".into()]),
                Entry::Message {
                    id: "SQL0204".into(),
                    text: "T in QGPL not found.".into(),
                },
                Entry::Message {
                    id: String::new(),
                    text: "Session started.".into(),
                },
            ],
            input: vec!["select 1".into()],
        };
        let back = from_text(&to_text(&s)).unwrap();
        assert_eq!(back, s);
        assert!(from_text("junk").is_err());
    }

    #[test]
    fn source() {
        let e = vec![Entry::Statement(vec![
            String::new(),
            "select 1".into(),
            "from sysibm.sysdummy1".into(),
        ])];
        assert_eq!(
            source_lines(&e),
            vec!["select 1".to_string(), "from sysibm.sysdummy1;".to_string()]
        );
    }
}
