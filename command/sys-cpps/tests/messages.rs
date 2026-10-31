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
