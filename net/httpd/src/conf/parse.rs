// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading `httpd.conf`: lines (a trailing `\` continues one), comments,
//! directives with their arguments, `<Section args>` ... `</Section>`.
//! Every node keeps its source text so an unedited file prints back
//! byte for byte.

use super::rules;
use super::{Conf, ConfError, Directive, Node, Section};

/// Split a directive's argument text into words: double quotes group,
/// `\"` and `\\` escape inside them.
pub fn split_args(s: &str, line: u32, col0: u32) -> Result<Vec<String>, ConfError> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut quoted = false;
    let mut quote_col = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if quoted {
            match c {
                '\\' if matches!(it.peek(), Some((_, '"' | '\\'))) => {
                    cur.push(it.next().map_or('\\', |(_, c)| c));
                }
                '"' => quoted = false,
                c => cur.push(c),
            }
            continue;
        }
        match c {
            '"' => {
                quoted = true;
                in_word = true;
                quote_col = col0 + u32::try_from(i).unwrap_or(0);
            }
            c if c.is_whitespace() => {
                if in_word {
                    out.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                cur.push(c);
                in_word = true;
            }
        }
    }
    if quoted {
        return Err(ConfError::new(
            line,
            quote_col,
            "A quoted argument is not closed.",
        ));
    }
    if in_word {
        out.push(cur);
    }
    Ok(out)
}

/// One logical line: its first physical line number, its text with
/// continuations joined, and the physical text.
struct Logical {
    line: u32,
    text: String,
    raw: String,
}

fn logical_lines(src: &str) -> Vec<Logical> {
    let mut out = Vec::new();
    let mut pending: Option<Logical> = None;
    for (i, l) in src.split_inclusive('\n').enumerate() {
        let n = u32::try_from(i + 1).unwrap_or(u32::MAX);
        let body = l.trim_end_matches(['\n', '\r']);
        let mut cur = pending.take().unwrap_or(Logical {
            line: n,
            text: String::new(),
            raw: String::new(),
        });
        cur.raw.push_str(l);
        if let Some(stripped) = body.strip_suffix('\\') {
            cur.text.push_str(stripped);
            cur.text.push(' ');
            pending = Some(cur);
        } else {
            cur.text.push_str(body);
            out.push(cur);
        }
    }
    if let Some(p) = pending {
        out.push(p);
    }
    out
}

/// Parse `src`. Errors carry the line and column (1-based).
pub fn parse(src: &str) -> Result<Conf, ConfError> {
    let mut stack: Vec<Section> = Vec::new();
    let mut top: Vec<Node> = Vec::new();
    for l in logical_lines(src) {
        let indent = l.text.len() - l.text.trim_start().len();
        let col = u32::try_from(indent + 1).unwrap_or(1);
        let t = l.text.trim();
        let node = if t.is_empty() {
            Some(Node::Blank(l.raw))
        } else if t.starts_with('#') {
            Some(Node::Comment(l.raw))
        } else if let Some(rest) = t.strip_prefix("</") {
            let Some(name) = rest.strip_suffix('>') else {
                return Err(ConfError::new(
                    l.line,
                    col,
                    "A section end is missing its closing '>'.",
                ));
            };
            let name = name.trim();
            let Some(mut sec) = stack.pop() else {
                return Err(ConfError::new(
                    l.line,
                    col,
                    &format!("</{name}> without a matching <{name}>."),
                ));
            };
            if !sec.name.eq_ignore_ascii_case(name) {
                return Err(ConfError::new(
                    l.line,
                    col,
                    &format!(
                        "</{name}> closes <{}> opened on line {}.",
                        sec.name, sec.line
                    ),
                ));
            }
            sec.close_raw = Some(l.raw);
            Some(Node::Section(sec))
        } else if let Some(rest) = t.strip_prefix('<') {
            let Some(inner) = rest.strip_suffix('>') else {
                return Err(ConfError::new(
                    l.line,
                    col,
                    "A section start is missing its closing '>'.",
                ));
            };
            let (name, args) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
            let args_col = col + 1 + u32::try_from(name.len() + 1).unwrap_or(0);
            let args = split_args(args, l.line, args_col)?;
            rules::check_section(
                name,
                &args,
                l.line,
                col,
                stack.last().map(|s| s.name.as_str()),
            )?;
            stack.push(Section {
                name: name.to_string(),
                args,
                line: l.line,
                items: Vec::new(),
                raw: Some(l.raw),
                close_raw: None,
            });
            None
        } else {
            let (name, rest) = t.split_once(char::is_whitespace).unwrap_or((t, ""));
            let args_col = col + u32::try_from(t.len() - rest.len()).unwrap_or(0);
            let args = split_args(rest, l.line, args_col)?;
            rules::check_directive(
                name,
                &args,
                l.line,
                col,
                stack.last().map(|s| s.name.as_str()),
            )?;
            Some(Node::Directive(Directive {
                name: name.to_string(),
                args,
                line: l.line,
                raw: Some(l.raw),
            }))
        };
        // A closed section goes into its parent like any other node.
        if let Some(node) = node {
            match stack.last_mut() {
                Some(sec) => sec.items.push(node),
                None => top.push(node),
            }
        }
    }
    if let Some(open) = stack.pop() {
        return Err(ConfError::new(
            open.line,
            1,
            &format!("<{}> is not closed before the end of the file.", open.name),
        ));
    }
    Ok(Conf { items: top })
}
