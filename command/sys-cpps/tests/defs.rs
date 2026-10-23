// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every shipped definition compiles, the inventory matches the
//! sources, processing program names are unique, and no prompt text or
//! message text repeats an IBM phrasing from the denylist.

use std::collections::HashSet;

use sys_cpps::{Site, all, defs, inventory};

#[test]
fn every_definition_compiles() {
    let mut failed = Vec::new();
    for (name, source) in defs::DEFS {
        if let Err(diags) = cmd::source::compile(name, source) {
            for d in diags {
                failed.push(format!("{name}: {} {}", d.msgid, d.detail));
            }
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
    assert!(defs::DEFS.len() >= 3);
}

#[test]
fn file_names_match_command_names() {
    for (name, source) in defs::DEFS {
        assert!(
            name.len() <= 10 && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
            "{name}: a command name is up to ten upper-case characters"
        );
        assert!(source.contains("CMD"), "{name}: no CMD statement");
    }
}

#[test]
fn inventory_is_consistent() {
    let orphans = inventory::orphans();
    assert!(orphans.is_empty(), "inventory entries without a definition: {orphans:?}");
    let mut cpps = HashSet::new();
    for e in all() {
        assert!(e.cpp.len() <= 10 && e.cpp.starts_with('Q'), "{}: CPP {}", e.name, e.cpp);
        assert!(cpps.insert(e.cpp.clone()), "{}: CPP {} is used twice", e.name, e.cpp);
        if e.site == Site::Stub {
            assert!(e.stub.is_some(), "{}: a stub names its message", e.name);
        }
    }
}

/// IBM phrasings no shipped text may repeat (the tripwire of the
/// acceptance list); one per line, case-insensitive, `#` comments.
const DENYLIST: &str = include_str!("../denylist.txt");

fn denied() -> Vec<String> {
    DENYLIST
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_ascii_lowercase)
        .collect()
}

#[test]
fn no_ibm_phrasing_in_definitions() {
    let denied = denied();
    let mut hits = Vec::new();
    for (name, source) in defs::DEFS {
        let lower = source.to_ascii_lowercase();
        for d in &denied {
            if lower.contains(d) {
                hits.push(format!("{name}: {d:?}"));
            }
        }
    }
    assert!(hits.is_empty(), "\n{}", hits.join("\n"));
}

#[test]
fn no_ibm_phrasing_in_messages() {
    let denied = denied();
    let mut hits = Vec::new();
    for (file, text) in [
        ("qcpfmsg.toml", msgf_source("qcpfmsg.toml")),
        ("qceemsg.toml", msgf_source("qceemsg.toml")),
        ("qsqlmsg.toml", msgf_source("qsqlmsg.toml")),
        ("qrpglemsg.toml", msgf_source("qrpglemsg.toml")),
        ("qrnxmsg.toml", msgf_source("qrnxmsg.toml")),
    ] {
        let lower = text.to_ascii_lowercase();
        for d in &denied {
            if lower.contains(d) {
                hits.push(format!("{file}: {d:?}"));
            }
        }
    }
    assert!(hits.is_empty(), "\n{}", hits.join("\n"));
}

fn msgf_source(file: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../runtime/msgf/msgs")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
