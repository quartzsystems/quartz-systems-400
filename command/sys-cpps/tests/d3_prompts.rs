// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The V1R1M0 release commands' prompts (items D3) against the
//! primary's captures under `tools/conformance/golden/verified/oracle71/d3/`
//! (V7R1M0, `tools/conformance/probes/rel/d3-prompts71.tn`). Rows 5 to 20
//! are compared as `phase12.rs` compares them: the prompt texts and the
//! values.

use std::path::Path;

use cmd::analyze::{Env, MemSource, analyze};
use cmd::prompt::Prompter;

fn source() -> MemSource {
    let mut s = MemSource::default();
    for (name, _) in sys_cpps::defs::DEFS {
        let o = sys_cpps::install::object(name, 0).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        s.put(o.def);
    }
    s
}

fn golden_rows(dir: &str, name: &str) -> Vec<String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/verified/oracle71")
        .join(dir)
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect()
}

/// The prompt of `text`, against golden `name` of `d3/`, row by row.
fn check(name: &str, text: &str) {
    check_in("d3", name, text);
}

/// The prompt of `text`, against golden `dir/name`, row by row.
fn check_in(dir: &str, name: &str, text: &str) {
    let mut src = source();
    let r = {
        let mut env = Env::new(&mut src);
        env.setting.interactive = true;
        analyze(&mut env, text).unwrap_or_else(|e| panic!("{text}: {e}"))
    };
    let p = Prompter::new(&r);
    let s = p.screen();
    let want = golden_rows(dir, name);
    let mut diffs = Vec::new();
    // The parameter rows: each row's prompt text (to column 34, dot
    // leaders aside) must match, and every value the capture shows after
    // the keyword must be on our row. The title, the instruction, the
    // function key rows and the columns are the prompter's own layout.
    let label = |r: &str| {
        if r.trim() == "Additional Parameters" {
            return r.trim().to_string();
        }
        r.chars()
            .take(34)
            .collect::<String>()
            .trim_end_matches(['.', ' '])
            .trim_start()
            .to_string()
    };
    // Blank rows aside: the prompter spaces "Additional Parameters" its
    // own way.
    let rows = |v: &[String]| -> Vec<String> {
        v.iter()
            .take(20)
            .skip(4)
            .map(|r| r.trim_end().to_string())
            // The prompter places "+ for more values" its own way.
            .filter(|r| !r.is_empty() && r.trim() != "+ for more values")
            .collect()
    };
    let ours = rows(&s.rows);
    for (i, w) in rows(&want).iter().enumerate() {
        let got = ours.get(i).map_or("", String::as_str);
        let values: Vec<&str> = w
            .get(34..)
            .unwrap_or("")
            .split_whitespace()
            .filter(|t| *t != ">" && !t.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
            // IBM ends a list it cut with "..."; what it shows must be ours.
            .map(|t| t.trim_end_matches("..."))
            .collect();
        let tail = got.get(34..).unwrap_or("");
        if label(got) != label(w) || !values.iter().all(|v| tail.contains(v)) {
            diffs.push(format!("row {}:\n  want {w:?}\n  got  {got:?}", i + 1));
        }
    }
    assert!(diffs.is_empty(), "{name}:\n{}", diffs.join("\n"));
}

#[test]
fn security_attributes_and_function_usage() {
    check("prompt-dspseca", "?DSPSECA");
    check("prompt-wrkfcnusg", "?WRKFCNUSG");
    check("prompt-chgfcnusg", "?CHGFCNUSG");
    check("prompt-dspfcnusg", "?DSPFCNUSG");
}

#[test]
fn chgseca_shows_the_next_numbers() {
    // The capture's values come from the prompt override program.
    check("prompt-chgseca", "?CHGSECA UID(109) GID(102)");
}

#[test]
fn journal_commands() {
    check("prompt-wrkjrnrcv", "?WRKJRNRCV");
    check("prompt-rcvjrne", "?RCVJRNE");
    check("prompt-cmpjrnimg", "?CMPJRNIMG");
}

#[test]
fn save_file_data_telnet_and_pass_through() {
    check("prompt-savsavfdta", "?SAVSAVFDTA");
    check("prompt-telnet", "?TELNET");
    check("prompt-strpasthr", "?STRPASTHR");
}
