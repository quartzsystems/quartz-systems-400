// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! CL commands as the sample system runs them: a command string's name
//! and parameters, and the messages it answers with.

use std::collections::BTreeMap;

use qsweb::msg::{Kind, Message};

use super::text;

/// Message `id` with `data` from the fixture's message texts.
pub fn msg(kind: Kind, id: &str, data: &[&str]) -> Message {
    let t = text::second_level(id, data).map(|s| s.text).unwrap_or_default();
    Message::new(kind, id, &t)
}

/// A command's completion message.
#[allow(clippy::unnecessary_wraps)]
pub fn done(id: &str, data: &[&str]) -> Result<Message, Message> {
    Ok(msg(Kind::Done, id, data))
}

/// A command's escape message.
pub fn fail(id: &str, data: &[&str]) -> Result<Message, Message> {
    Err(msg(Kind::Error, id, data))
}

/// A CL command's name and its `KEYWORD(value)` parameters, values with
/// their parentheses' contents as typed.
pub fn parse(cmd: &str) -> (String, BTreeMap<String, String>) {
    let cmd = cmd.trim();
    let (name, rest) = cmd.split_once(' ').unwrap_or((cmd, ""));
    let mut parms = BTreeMap::new();
    let b: Vec<char> = rest.chars().collect();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && b[i] == ' ' {
            i += 1;
        }
        let start = i;
        while i < b.len() && b[i] != '(' && b[i] != ' ' {
            i += 1;
        }
        let key: String = b[start..i].iter().collect();
        if i < b.len() && b[i] == '(' {
            let mut depth = 0;
            let mut quoted = false;
            let vs = i + 1;
            while i < b.len() {
                match b[i] {
                    '\'' => quoted = !quoted,
                    '(' if !quoted => depth += 1,
                    ')' if !quoted => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
            let v: String = b[vs..i.min(b.len())].iter().collect();
            parms.insert(key.to_ascii_uppercase(), v);
            i += 1;
        } else if !key.is_empty() {
            parms.insert(key.to_ascii_uppercase(), String::new());
        }
    }
    (name.to_ascii_uppercase(), parms)
}

/// A quoted CL string's text.
pub fn unquote(s: &str) -> String {
    let s = s.trim();
    match s.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        Some(t) => t.replace("''", "'"),
        None => s.to_string(),
    }
}

/// `lib/name` split; a bare name gets `lib`.
pub fn split<'a>(q: &'a str, lib: &'a str) -> (&'a str, &'a str) {
    q.split_once('/').unwrap_or((lib, q))
}
