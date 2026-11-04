// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Our command titles: the `PROMPT('...')` of the `CMD` statement in
//! each `command/sys-cpps/cmds/*.cmd` definition source.

use std::collections::BTreeMap;
use std::path::Path;

use crate::{Error, Result};

/// Drop `/* ... */` comments.
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => return out,
        }
    }
    out.push_str(rest);
    out
}

/// Statements: lines joined over their `+` and `-` continuations.
fn statements(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        let t = line.trim_end();
        if let Some(head) = t.strip_suffix('+') {
            cur.push_str(head.trim());
            cur.push(' ');
        } else if let Some(head) = t.strip_suffix('-') {
            cur.push_str(head);
        } else {
            cur.push_str(t.trim());
            if !cur.trim().is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            cur.clear();
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

/// The quoted value after `PROMPT(`; `''` is one quote.
fn prompt_value(stmt: &str) -> Option<String> {
    let at = stmt.find("PROMPT(")?;
    let rest = stmt[at + 7..].trim_start();
    let rest = rest.strip_prefix('\'')?;
    let mut out = String::new();
    let mut chars = rest.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if chars.peek() == Some(&'\'') {
                chars.next();
                out.push('\'');
            } else {
                return Some(out);
            }
        } else {
            out.push(c);
        }
    }
    None
}

/// The title of a definition source: the `CMD` statement's `PROMPT`.
#[must_use]
pub fn title_of(source: &str) -> Option<String> {
    let text = strip_comments(source);
    statements(&text)
        .iter()
        .find(|s| s.split_whitespace().next() == Some("CMD"))
        .and_then(|s| prompt_value(s))
}

/// The value of `KWD(` on the `CMD` statement, without its library.
fn cmd_value(source: &str, kwd: &str) -> Option<String> {
    let text = strip_comments(source);
    let stmts = statements(&text);
    let cmd = stmts.iter().find(|s| s.split_whitespace().next() == Some("CMD"))?;
    let at = cmd.find(&format!("{kwd}("))?;
    let rest = &cmd[at + kwd.len() + 1..];
    let v = rest[..rest.find(')')?].trim();
    Some(v.rsplit('/').next().unwrap_or(v).to_string())
}

/// Every `NAME.cmd` under `dir` with its help panel group and module
/// (`HLPPNLGRP`, `HLPID`), for the menus' item help.
pub fn read_help(dir: &Path) -> Result<BTreeMap<String, (String, String)>> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_none_or(|x| x != "cmd") {
            continue;
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_uppercase())
            .unwrap_or_default();
        let text = std::fs::read_to_string(&path)?;
        if let Some(g) = cmd_value(&text, "HLPPNLGRP") {
            let id = cmd_value(&text, "HLPID").unwrap_or_else(|| name.clone());
            out.insert(name, (g, id));
        }
    }
    Ok(out)
}

/// Every `NAME.cmd` under `dir` with its title.
pub fn read_titles(dir: &Path) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().is_none_or(|x| x != "cmd") {
            continue;
        }
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_uppercase())
            .unwrap_or_default();
        let text = std::fs::read_to_string(&path)?;
        let title = title_of(&text).ok_or_else(|| {
            Error::Input(format!(
                "{}: no PROMPT on the CMD statement",
                path.display()
            ))
        })?;
        out.insert(name, title);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_of_the_cmd_statement() {
        let src = "/* ADDJOBSCDE: add an entry. */\n             CMD        PROMPT('Add Job Schedule Entry') MAXPOS(1) +\n                          ALLOW(*ALL)\n             PARM       KWD(JOB) TYPE(*NAME) PROMPT('Job name')\n";
        assert_eq!(title_of(src).as_deref(), Some("Add Job Schedule Entry"));
        let quoted = "CMD PROMPT('Change User''s Profile')";
        assert_eq!(title_of(quoted).as_deref(), Some("Change User's Profile"));
        let split = "             CMD        MAXPOS(1) +\n                          PROMPT('Over +\n                          Lines')";
        assert_eq!(title_of(split).as_deref(), Some("Over Lines"));
        assert_eq!(title_of("PARM KWD(X) PROMPT('No CMD')"), None);
    }
}
