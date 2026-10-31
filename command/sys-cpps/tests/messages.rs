// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a processing program sends is a message: every identifier the
//! job's and the service's processing programs end a command with
//! (`Fail::new`, `msgid:`, `escape(`) is a literal message ID that a
//! shipped message file describes, and the catch-alls `CPF9898` and
//! `CPF9899` are only sent through the function check
//! (`sys_cpps::check`). A panel's message line never shows an error
//! value's own text (`e.to_string()`, `{e}`): that goes to the job log
//! as `CPD9898`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Every message ID the shipped message files describe.
fn described() -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let dir = root().join("runtime/msgf/msgs");
    for e in std::fs::read_dir(&dir).expect("msgs").flatten() {
        let text = std::fs::read_to_string(e.path()).unwrap_or_default();
        for line in text.lines() {
            if let Some(rest) = line.trim().strip_prefix("id = \"") {
                ids.insert(rest.trim_end_matches('"').to_string());
            }
        }
    }
    ids
}

fn is_msgid(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7 && b[..3].iter().all(u8::is_ascii_uppercase) && b[3..].iter().all(u8::is_ascii_hexdigit)
}

/// The string literal right after `at` in `line`, if one starts there.
fn literal_at(line: &str, at: usize) -> Option<&str> {
    let rest = line[at..].trim_start().strip_prefix('"')?;
    rest.split('"').next()
}

/// The directories whose code ends commands.
const DIRS: &[&str] = &[
    "work/qsjob/src/command",
    "work/qsjob/src/panels",
    "work/qsjob/src/devtools",
    "lic/slic/src/cpp",
];

/// The sends found: where each is, and its literal.
fn sends() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for d in DIRS {
        let mut files = Vec::new();
        sources(&root().join(d), &mut files);
        for f in files {
            let text = std::fs::read_to_string(&f).unwrap_or_default();
            let name = f.strip_prefix(root()).unwrap_or(&f).display().to_string();
            // Comments out, so a call may span lines.
            let code: String = text
                .lines()
                .map(|l| if l.trim_start().starts_with("//") { "" } else { l })
                .collect::<Vec<_>>()
                .join("\n");
            for pat in ["Fail::new(", "msgid: ", "escape(", "refused("] {
                let mut from = 0;
                while let Some(i) = code[from..].find(pat) {
                    let at = from + i + pat.len();
                    if let Some(lit) = literal_at(&code, at) {
                        let line = code[..at].matches('\n').count() + 1;
                        // A text form: the identifier, then the detail.
                        let lit = lit.split(' ').next().unwrap_or(lit);
                        out.push((format!("{name}:{line}"), lit.to_string()));
                    }
                    from = at;
                }
            }
        }
    }
    out
}

#[test]
fn sent_identifiers_are_described_messages() {
    let ids = described();
    let mut wrong = Vec::new();
    for (at, lit) in sends() {
        if !is_msgid(&lit) {
            wrong.push(format!("{at}: literal {lit:?} where a message ID goes"));
            continue;
        }
        if !ids.contains(&lit) {
            wrong.push(format!("{at}: {lit} is in no shipped message file"));
        }
        if lit == "CPF9898" || lit == "CPF9899" {
            wrong.push(format!("{at}: {lit} sent directly; use the function check"));
        }
    }
    assert!(wrong.is_empty(), "{} sends:\n  {}", wrong.len(), wrong.join("\n  "));
}

#[test]
fn message_lines_show_no_error_text() {
    let mut files = Vec::new();
    sources(&root().join("work/qsjob/src/panels"), &mut files);
    let raw = ["e.to_string()", "{e}", "{e:?}", "e.text,", "e.text)", "(e, true)", "(e, false)"];
    let mut wrong = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        for (n, line) in lines.iter().enumerate() {
            if !line.contains("message = Some((") {
                continue;
            }
            // The message's expression: this line and the next two.
            let expr: String = lines[n..(n + 3).min(lines.len())].join(" ");
            let expr = &expr[..expr.find("));").unwrap_or(expr.len())];
            if raw.iter().any(|r| expr.contains(r)) {
                let name = f.strip_prefix(root()).unwrap_or(&f).display().to_string();
                wrong.push(format!("{name}:{}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(wrong.is_empty(), "message lines with an error's text:\n  {}", wrong.join("\n  "));
}

/// Where the message lines' words are written: the job's displays
/// (panels, menus, System Request, the developer tools' drivers), the
/// UIM dialog, the UIM APIs and the developer tools.
const LINE_DIRS: &[&str] = &[
    "work/qsjob/src",
    "ui/uim/src/engine",
    "runtime/sysapi/src/uiapi",
    "devtools",
];

/// Files allowed a literal on a message line, each with why. (The
/// LIC's own screens, `lic/`, are not scanned: DST and the IPL console
/// run before there are message files, as IBM's do.)
const ALLOWED: &[(&str, &str)] = &[];

/// A file's code: comment lines blank, test modules cut.
fn code_of(text: &str) -> String {
    let text = text.find("#[cfg(test)]").map_or(text, |i| &text[..i]);
    text.lines()
        .map(|l| if l.trim_start().starts_with("//") { "" } else { l })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The identifier ending at `end` (letters, digits, `_`, `.`, `*`).
fn ident_before(code: &str, end: usize) -> &str {
    let head = code[..end].trim_end();
    let start = head
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '*'))
        .map_or(0, |i| i + 1);
    &head[start..]
}

/// Whether an expression starts with text of its own: a literal or a
/// `format!`, under any `Some(` and `(`.
fn starts_literal(rhs: &str) -> bool {
    let mut r = rhs.trim_start();
    loop {
        if let Some(x) = r.strip_prefix("Some(") {
            r = x.trim_start();
        } else if let Some(x) = r.strip_prefix('(') {
            r = x.trim_start();
        } else {
            break;
        }
    }
    r.starts_with('"') || r.starts_with("format!(") || r.starts_with("String::from(\"")
}

/// Every message line's text in the scanned files: what is written
/// to one that is not a message ID (`wrong`), and the IDs written.
fn message_lines() -> (Vec<String>, Vec<(String, String)>) {
    let mut wrong = Vec::new();
    let mut ids = Vec::new();
    for d in LINE_DIRS {
        let mut files = Vec::new();
        sources(&root().join(d), &mut files);
        for f in files {
            let name = f.strip_prefix(root()).unwrap_or(&f).display().to_string().replace('\\', "/");
            if name.contains("/tests/") || ALLOWED.iter().any(|(a, _)| *a == name) {
                continue;
            }
            let code = code_of(&std::fs::read_to_string(&f).unwrap_or_default());
            let at = |i: usize| format!("{name}:{}", code[..i].matches('\n').count() + 1);
            // Assignments to a message: `message = "..."`, `d.message =
            // Some((format!(..), true))`, `msg = ...`.
            for (i, _) in code.match_indices('=') {
                let b = code.as_bytes();
                let prev = if i > 0 { b[i - 1] } else { b' ' };
                let next = b.get(i + 1).copied().unwrap_or(b' ');
                if matches!(prev, b'=' | b'!' | b'<' | b'>' | b'+' | b'-') || matches!(next, b'=' | b'>') {
                    continue;
                }
                let id = ident_before(&code, i).to_ascii_lowercase();
                let last = id.rsplit('.').next().unwrap_or("").trim_start_matches('*');
                if (last.contains("message") || last == "msg") && starts_literal(&code[i + 1..]) {
                    wrong.push(format!("{}: text assigned to {id}", at(i)));
                }
            }
            // Text handed straight to a message line.
            for pat in [".set_message(\"", ".error_line(\"", "Act::Say(\"", "Refused(\""] {
                for (i, _) in code.match_indices(pat) {
                    wrong.push(format!("{}: text given to {}", at(i), pat.trim_end_matches('"')));
                }
            }
            // Messages by identifier: the literal must be one.
            for pat in [".error(\"", ".info(\"", "LineMsg::error(\"", "LineMsg::info(\"", ".msg(\""] {
                for (i, _) in code.match_indices(pat) {
                    if let Some(lit) = literal_at(&code, i + pat.len() - 1) {
                        ids.push((at(i), lit.to_string()));
                    }
                }
            }
            // The identifiers a module names as constants.
            if name.ends_with("/msgid.rs") {
                for (i, _) in code.match_indices(": &str = \"") {
                    if let Some(lit) = literal_at(&code, i + 9) {
                        ids.push((at(i), lit.to_string()));
                    }
                }
            }
            let said = code.match_indices("msgline::text(").chain(code.match_indices("msgline::say("));
            for (i, _) in said {
                // The identifier is the second argument: after the host.
                let window = &code[i..(i + 120).min(code.len())];
                if let Some(c) = window.find(',')
                    && let Some(lit) = literal_at(window, c + 1)
                {
                    ids.push((at(i), lit.to_string()));
                }
            }
        }
    }
    (wrong, ids)
}

#[test]
fn message_lines_carry_message_ids() {
    let described = described();
    let (mut wrong, ids) = message_lines();
    for (at, lit) in ids {
        if !is_msgid(&lit) {
            wrong.push(format!("{at}: literal {lit:?} where a message ID goes"));
        } else if !described.contains(&lit) {
            wrong.push(format!("{at}: {lit} is in no shipped message file"));
        }
    }
    for (file, why) in ALLOWED {
        assert!(!why.trim().is_empty(), "{file} is allowed with no reason");
    }
    assert!(
        wrong.is_empty(),
        "{} message line texts that are not message IDs:\n  {}",
        wrong.len(),
        wrong.join("\n  ")
    );
}
