// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! CL source as SEU prompts and checks it: a command spread over
//! records with `+` and `-`, its label and its comment; and a prompted
//! command laid back into records the way SEU writes them (the label
//! from column 2, the command at 14, its parameters from 25, `+` at
//! the end of each record that continues).

/// One command of CL source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Statement {
    /// First record's position.
    pub first: usize,
    /// Last record's position.
    pub last: usize,
    /// The label, without its colon.
    pub label: String,
    /// The command string.
    pub command: String,
    /// The comment's text, without `/*` and `*/`.
    pub comment: String,
}

/// A record's text and whether it continues (`+` or `-` as its last
/// character outside a comment): the text without the sign.
fn continued(line: &str) -> (String, Option<char>) {
    let t = line.trim_end();
    match t.chars().last() {
        Some(c @ ('+' | '-')) if !t.ends_with("*/") => (t[..t.len() - 1].to_string(), Some(c)),
        _ => (t.to_string(), None),
    }
}

/// Take the comments out of a command's text.
fn strip_comments(text: &str) -> (String, String) {
    let mut out = String::new();
    let mut comment = String::new();
    let mut rest = text;
    let mut quoted = false;
    while let Some(c) = rest.chars().next() {
        if c == '\'' {
            quoted = !quoted;
        }
        if !quoted && rest.starts_with("/*") {
            let end = rest[2..].find("*/").map_or(rest.len(), |e| e + 4);
            let inner = rest[2..end.min(rest.len()).saturating_sub(2).max(2)].trim();
            if !comment.is_empty() {
                comment.push(' ');
            }
            comment.push_str(inner);
            rest = &rest[end.min(rest.len())..];
            continue;
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    (out, comment)
}

/// The statement record `at` belongs to: back to the first record of
/// its command, on through its continuations.
#[must_use]
pub fn statement_at(records: &[String], at: usize) -> Option<Statement> {
    if at >= records.len() {
        return None;
    }
    let mut first = at;
    while first > 0 && continued(&records[first - 1]).1.is_some() {
        first -= 1;
    }
    let mut last = first;
    let mut text = String::new();
    let mut sign: Option<char> = None;
    loop {
        // After `+` the next record's leading blanks go; after `-` they stay.
        let raw = records[last].as_str();
        let src = if sign == Some('+') { raw.trim_start() } else { raw };
        let (t, s) = continued(src);
        text.push_str(&t);
        if s == Some('+') {
            let kept = text.trim_end().len();
            text.truncate(kept);
            text.push(' ');
        }
        sign = s;
        if sign.is_none() || last + 1 >= records.len() {
            break;
        }
        last += 1;
    }
    let (code, comment) = strip_comments(&text);
    let code = code.trim();
    let (label, command) = split_label(code);
    Some(Statement {
        first,
        last,
        label,
        command,
        comment,
    })
}

/// `LABEL: CMD ...` into its label and command.
fn split_label(code: &str) -> (String, String) {
    if let Some(i) = code.find(':') {
        let (l, rest) = code.split_at(i);
        let l = l.trim();
        if !l.is_empty() && !l.contains(' ') && !l.contains('(') && !l.contains('\'') {
            return (l.to_ascii_uppercase(), rest[1..].trim().to_string());
        }
    }
    (String::new(), code.to_string())
}

/// The command's words: its name, then each parameter, quoted strings
/// and parentheses kept whole.
fn words(command: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut quoted = false;
    for c in command.chars() {
        match c {
            '\'' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => depth -= 1,
            ' ' if !quoted && depth <= 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The records of a statement as SEU writes a prompted command: the
/// label from column 2, the command name at 14, parameters from 25,
/// `+` closing each record that goes on, the comment after the last.
#[must_use]
pub fn format(label: &str, command: &str, comment: &str, width: usize) -> Vec<String> {
    let width = width.clamp(40, 80);
    let ws = words(command.trim());
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    if !label.trim().is_empty() {
        cur.push(' ');
        cur.push_str(label.trim());
        cur.push(':');
    }
    let pad = |s: &mut String, col: usize| {
        let n = s.chars().count();
        if n + 1 < col {
            s.push_str(&" ".repeat(col - 1 - n));
        } else {
            s.push(' ');
        }
    };
    let mut it = ws.into_iter();
    if let Some(name) = it.next() {
        pad(&mut cur, 14);
        cur.push_str(&name);
    }
    let mut first_parm = true;
    for w in it {
        let at_25 = cur.chars().count() < 24 && first_parm;
        if at_25 {
            pad(&mut cur, 25);
        } else if cur.chars().count() + 1 + w.chars().count() + 2 > width {
            cur.push_str(" +");
            lines.push(std::mem::take(&mut cur));
            pad(&mut cur, 25);
        } else {
            cur.push(' ');
        }
        // A value longer than a record is cut with `-`.
        let mut rest: String = w;
        while cur.chars().count() + rest.chars().count() > width - 2 {
            let room = width - 2 - cur.chars().count();
            let head: String = rest.chars().take(room).collect();
            cur.push_str(&head);
            cur.push('-');
            lines.push(std::mem::take(&mut cur));
            rest = rest.chars().skip(room).collect();
        }
        cur.push_str(&rest);
        first_parm = false;
    }
    let comment = comment.trim();
    if !comment.is_empty() {
        let c = format!("/* {comment} */");
        if cur.chars().count() + 1 + c.chars().count() <= width {
            let col = (cur.chars().count() + 2).max(52.min(width - c.chars().count() + 1));
            pad(&mut cur, col);
            cur.push_str(&c);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = format!("             {c}");
        }
    }
    lines.push(cur);
    lines.into_iter().map(|l| l.trim_end().to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recs(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn one_line() {
        let r = recs(&["             DSPLIB     LIB(QGPL) /* look */"]);
        let s = statement_at(&r, 0).unwrap();
        assert_eq!((s.first, s.last), (0, 0));
        assert_eq!(s.command, "DSPLIB     LIB(QGPL)");
        assert_eq!(s.comment, "look");
        assert!(s.label.is_empty());
    }

    #[test]
    fn continued_with_a_label() {
        let r = recs(&[
            "             PGM",
            " LOOP:       SNDPGMMSG  MSG('one +",
            "                          two') +",
            "                          TOPGMQ(*EXT)",
            "             ENDPGM",
        ]);
        let s = statement_at(&r, 2).unwrap();
        assert_eq!((s.first, s.last), (1, 3));
        assert_eq!(s.label, "LOOP");
        assert!(s.command.starts_with("SNDPGMMSG"));
        assert!(s.command.contains("TOPGMQ(*EXT)"));
        assert_eq!(statement_at(&r, 4).unwrap().command, "ENDPGM");
    }

    #[test]
    fn formats_like_seu() {
        let l = format("", "DSPLIB LIB(QGPL) OUTPUT(*PRINT)", "", 80);
        assert_eq!(l, ["             DSPLIB     LIB(QGPL) OUTPUT(*PRINT)"]);
        let l = format("TOP", "GOTO CMDLBL(TOP)", "again", 80);
        assert_eq!(l.len(), 1);
        assert!(l[0].starts_with(" TOP:        GOTO       CMDLBL(TOP)"));
        assert!(l[0].ends_with("/* again */"));
    }

    #[test]
    fn long_commands_continue() {
        let cmd = format!("SNDPGMMSG MSG('{}') TOPGMQ(*EXT) MSGTYPE(*INFO)", "x".repeat(40));
        let l = format("", &cmd, "", 80);
        assert!(l.len() >= 2);
        assert!(l[0].ends_with('+'));
        assert!(l.iter().all(|x| x.chars().count() <= 80));
        let r: Vec<String> = l.clone();
        let s = statement_at(&r, 0).unwrap();
        assert_eq!(s.command.split_whitespace().next(), Some("SNDPGMMSG"));
        assert!(s.command.contains("MSGTYPE(*INFO)"));
    }
}
