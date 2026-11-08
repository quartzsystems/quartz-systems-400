// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Message texts for diagnostics, read from a message file source
//! (`runtime/msgf/msgs/*.toml`: `[[msg]]` tables of `id`, `sev`,
//! `text`, `help` and `vars`), so a front end can fill a
//! [`Diagnostic`](crate::Diagnostic)'s text and help without the
//! message file object.

use std::collections::HashMap;

/// One message description.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Msg {
    /// The first-level text, `&1`... unfilled.
    pub text: String,
    /// The second-level text.
    pub help: String,
    /// The severity.
    pub severity: u8,
    /// The variables' types (`*CHAR 10`, `*DEC 5 0`).
    pub vars: Vec<String>,
}

/// The descriptions of one message file source, by identifier.
#[derive(Clone, Debug, Default)]
pub struct Table {
    msgs: HashMap<String, Msg>,
}

impl Table {
    /// The descriptions `source` holds; lines it does not understand
    /// are skipped.
    #[must_use]
    pub fn parse(source: &str) -> Self {
        let mut msgs = HashMap::new();
        let mut cur: Option<(String, Msg)> = None;
        for line in source.lines() {
            let line = line.trim();
            if line == "[[msg]]" {
                if let Some((id, m)) = cur.take() {
                    msgs.insert(id, m);
                }
                cur = Some((String::new(), Msg::default()));
                continue;
            }
            let Some((id, m)) = cur.as_mut() else {
                continue;
            };
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "id" => *id = string(value).unwrap_or_default(),
                "text" => m.text = string(value).unwrap_or_default(),
                "help" => m.help = string(value).unwrap_or_default(),
                "sev" => m.severity = value.parse().unwrap_or(0),
                "vars" => m.vars = strings(value),
                _ => {}
            }
        }
        if let Some((id, m)) = cur {
            msgs.insert(id, m);
        }
        msgs.remove("");
        Self { msgs }
    }

    /// The description of `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Msg> {
        self.msgs.get(id)
    }

    /// `id`'s text and help with `&1`, `&2`... replaced by `values`;
    /// `None` when the file has no such message.
    #[must_use]
    pub fn texts(&self, id: &str, values: &[String]) -> Option<(String, String)> {
        let m = self.get(id)?;
        Some((fill(&m.text, values), fill(&m.help, values)))
    }
}

/// `text` with `&n` replaced by `values[n - 1]` (a missing value
/// leaves an empty string).
#[must_use]
pub fn fill(text: &str, values: &[String]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices().peekable();
    while let Some((_, c)) = chars.next() {
        if c != '&' {
            out.push(c);
            continue;
        }
        let mut n = 0usize;
        let mut digits = 0;
        while let Some(&(_, d)) = chars.peek() {
            let Some(v) = d.to_digit(10).filter(|_| digits < 2) else {
                break;
            };
            n = n * 10 + v as usize;
            digits += 1;
            chars.next();
        }
        if digits == 0 {
            out.push('&');
        } else if let Some(v) = n.checked_sub(1).and_then(|i| values.get(i)) {
            out.push_str(v.trim_end());
        }
    }
    out
}

/// A TOML basic string.
fn string(v: &str) -> Option<String> {
    let inner = v.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// A TOML array of basic strings, on one line.
fn strings(v: &str) -> Vec<String> {
    let inner = v.trim().trim_start_matches('[').trim_end_matches(']');
    inner.split(',').filter_map(|p| string(p.trim())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
name = "QTEST"

[[msg]]
id = "CPD0727"
sev = 30
text = "Variable &1 is not declared."
help = "Declare &1 with \"DCL\"."
vars = ["*CHAR 11", "*DEC 5 0"]
"#;

    #[test]
    fn a_message_is_read_and_filled() {
        let t = Table::parse(SRC);
        let m = t.get("CPD0727").unwrap();
        assert_eq!(m.severity, 30);
        assert_eq!(m.vars, ["*CHAR 11", "*DEC 5 0"]);
        let (text, help) = t.texts("CPD0727", &["&X   ".into()]).unwrap();
        assert_eq!(text, "Variable &X is not declared.");
        assert_eq!(help, "Declare &X with \"DCL\".");
        assert!(t.texts("CPD0001", &[]).is_none());
        assert_eq!(fill("&2 & &", &[]), " & &");
    }
}
