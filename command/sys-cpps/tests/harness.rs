// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every command string the QEMU harness types that names a shipped
//! command must analyze against its definition. The strings are read
//! from the harness sources (the lines that send them), `format!`
//! placeholders inside parentheses or qualified names standing in as
//! `X`, so a keyword the harness uses that the definition lacks is
//! found here and not in a twenty-minute QEMU run.

use std::path::{Path, PathBuf};

use cmd::analyze::{Env, MemSource, Setting};
use sys_cpps::{defs, install};

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

/// Whether a line sends what its literals hold.
fn sends(line: &str) -> bool {
    let t = line.trim_start();
    if line.contains("want(")
        || line.contains("refuse(")
        || line.contains("Err(")
        || line.contains("ok(&")
    {
        return false;
    }
    line.contains(".command(")
        || line.contains("entry(")
        || line.contains("entry_wait(")
        || line.contains(".send(")
        || line.contains("poll(")
        || t.starts_with('"')
        || t.starts_with("&format!(\"")
        || t.starts_with("format!(\"")
}

/// A literal with placeholders replaced by `X`; `None` when a
/// placeholder stands for a whole positional word (the string is a
/// pattern, not a command).
fn substituted(s: &str) -> Option<String> {
    let mut t = String::new();
    let mut depth = 0;
    let mut brace = 0;
    let mut prev = ' ';
    for c in s.chars() {
        match c {
            '{' => {
                brace += 1;
                if brace == 1 {
                    if depth == 0 && (prev == ' ' || t.is_empty()) {
                        return None;
                    }
                    t.push('X');
                    prev = 'X';
                }
            }
            '}' => brace -= 1,
            _ if brace > 0 => {}
            '(' => {
                depth += 1;
                t.push(c);
                prev = c;
            }
            ')' => {
                depth -= 1;
                t.push(c);
                prev = c;
            }
            _ => {
                t.push(c);
                prev = c;
            }
        }
    }
    Some(t.replace("\\\"", "\""))
}

/// The command strings of a source file, each with the lines that
/// follow it (where a test says which refusal it expects).
fn commands(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if !sends(line) {
            continue;
        }
        let following = lines[i..(i + 4).min(lines.len())].join("\n");
        let mut rest: &str = line;
        while let Some(start) = rest.find('"') {
            let after = &rest[start + 1..];
            let Some(end) = after.find('"') else {
                break;
            };
            let lit = &after[..end];
            rest = &after[end + 1..];
            let Some(s) = substituted(lit) else {
                continue;
            };
            let mut words = s.split_whitespace();
            let Some(first) = words.next() else {
                continue;
            };
            let tail: Vec<&str> = words.collect();
            if s.starts_with("svc.") || !first.chars().all(|c| c.is_ascii_alphanumeric()) {
                continue;
            }
            // Prose after the command word is a description, not a
            // command; a single word is checked only on a command line.
            let prose = tail
                .iter()
                .any(|w| !w.starts_with('\'') && w.chars().any(|c| c.is_ascii_lowercase()));
            if prose || (tail.is_empty() && !line.contains(".command(")) {
                continue;
            }
            out.push((s, following.clone()));
        }
    }
    out
}

#[test]
fn harness_commands_analyze() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/src");
    let mut files = Vec::new();
    sources(&root, &mut files);
    assert!(
        !files.is_empty(),
        "no harness sources under {}",
        root.display()
    );
    let mut src = MemSource::default();
    for (name, _) in defs::DEFS {
        src.put(
            install::object(name, 0)
                .expect("shipped definitions compile")
                .def,
        );
    }
    let mut env = Env::new(&mut src);
    env.setting = Setting {
        interactive: true,
        ..Setting::default()
    };
    let mut failed = Vec::new();
    let mut checked = 0;
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        for (lit, following) in commands(&text) {
            let word = lit
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_ascii_uppercase();
            if defs::source(&word).is_none() {
                continue;
            }
            checked += 1;
            if let Err(e) = cmd::analyze(&mut env, &lit) {
                // The setting is the harness's business; a refusal the
                // test expects is right.
                if e.msgid() == "CPD0031" || following.contains(e.msgid()) {
                    continue;
                }
                failed.push(format!(
                    "{}: {lit}\n    {}",
                    f.file_name().unwrap().to_string_lossy(),
                    e.to_string().replace('\n', "; ")
                ));
            }
        }
    }
    assert!(checked > 100, "only {checked} command strings found");
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}
