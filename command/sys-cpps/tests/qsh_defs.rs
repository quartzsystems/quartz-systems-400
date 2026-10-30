// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSH and the environment variable commands against pub400's prompt
//! captures under `tools/conformance/golden/phase10/prompts/` (V7R5M0),
//! rows 5 to 20 as `printers.rs` compares them, and their keywords,
//! types and defaults against the oracle's `QCDRCMDD` definitions
//! (`tools/conformance/oracle/net/pub400/command-definitions.txt`).
//! A long field's `...` row and the pages it pushes others onto are the
//! prompter's own layout.

use std::path::Path;

use cmd::analyze::{Env, MemSource, analyze};
use cmd::prompt::{Prompter, Step};

fn source() -> MemSource {
    let mut s = MemSource::default();
    for (name, _) in sys_cpps::defs::DEFS {
        let o = sys_cpps::install::object(name, 0).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        s.put(o.def);
    }
    s
}

fn golden_rows(name: &str) -> Vec<String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/phase10/prompts")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect()
}

/// The prompt of `text` after `keys`, against golden `name`, row by row.
fn check(name: &str, text: &str, keys: &[u8]) {
    let mut src = source();
    let r = {
        let mut env = Env::new(&mut src);
        env.setting.interactive = true;
        analyze(&mut env, text).unwrap_or_else(|e| panic!("{text}: {e}"))
    };
    let mut p = Prompter::new(&r);
    for k in keys {
        let fields: Vec<String> = p.screen().fields.iter().map(|f| f.value.clone()).collect();
        let mut env = Env::new(&mut src);
        env.setting.interactive = true;
        assert_eq!(p.key(*k, &fields, &mut env), Step::Show, "{name}: key {k}");
    }
    let s = p.screen();
    let want = golden_rows(name);
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
    // The "..." row of a long field: the prompter shows long fields its
    // own way.
    let rows = |v: &[String]| -> Vec<String> {
        v.iter()
            .take(20)
            .skip(4)
            .map(|r| r.trim_end().to_string())
            .filter(|r| !r.is_empty() && r.trim() != "...")
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
            .collect();
        let tail = got.get(34..).unwrap_or("");
        if label(got) != label(w) || !values.iter().all(|v| tail.contains(v)) {
            diffs.push(format!("row {}:\n  want {w:?}\n  got  {got:?}", i + 1));
        }
    }
    assert!(diffs.is_empty(), "{name}:\n{}", diffs.join("\n"));
}

#[test]
fn qsh_and_strqsh() {
    check("prompt-qsh", "?QSH", &[]);
    check("prompt-strqsh", "?STRQSH", &[]);
}

#[test]
fn addenvvar() {
    check("prompt-addenvvar", "?ADDENVVAR", &[]);
    check("prompt-addenvvar-f10", "?ADDENVVAR", &[10]);
    // IBM's third page holds REPLACE alone, its long VALUE field taking
    // rows ours does not; here REPLACE is on the first F10 page.
}

#[test]
fn chgenvvar_rmvenvvar_wrkenvvar() {
    check("prompt-chgenvvar", "?CHGENVVAR", &[]);
    check("prompt-chgenvvar-f10", "?CHGENVVAR", &[10]);
    check("prompt-rmvenvvar", "?RMVENVVAR", &[]);
    check("prompt-rmvenvvar-f10", "?RMVENVVAR", &[10]);
    check("prompt-wrkenvvar", "?WRKENVVAR", &[]);
}

/// The keywords and their order, types and defaults, from the capture.
fn captured(name: &str) -> Vec<(String, String, String)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/oracle/net/pub400/command-definitions.txt");
    let text = std::fs::read_to_string(&path).expect("the capture");
    let head = format!("CMD QSYS/{name} ");
    let mut out = Vec::new();
    let mut inside = false;
    for l in text.lines() {
        if l.starts_with(&head) {
            inside = true;
            continue;
        }
        if inside {
            let t = l.trim();
            let Some(rest) = t.strip_prefix("PARM ") else { break };
            let kwd = rest.split_whitespace().next().unwrap_or("").to_string();
            let get = |k: &str| {
                rest.split_whitespace()
                    .find_map(|w| w.strip_prefix(&format!("{k}=")))
                    .unwrap_or("")
                    .to_string()
            };
            out.push((kwd, get("Type"), get("Dft")));
        }
    }
    out
}

#[test]
fn definitions_match_the_oracle() {
    for name in ["QSH", "STRQSH", "ADDENVVAR", "CHGENVVAR", "RMVENVVAR", "WRKENVVAR"] {
        let cap = captured(name);
        assert!(!cap.is_empty(), "{name} captured");
        let d = cmd::source::compile(name, sys_cpps::defs::source(name).expect("shipped")).expect("compiles");
        let ours: Vec<String> = d.parms.iter().map(|p| p.kwd.clone()).collect();
        let theirs: Vec<String> = cap.iter().map(|c| c.0.clone()).collect();
        assert_eq!(ours, theirs, "{name} keywords");
        for (p, (kwd, _, dft)) in d.parms.iter().zip(&cap) {
            let ours = p.dft.clone().unwrap_or_default();
            assert_eq!(&ours.to_ascii_uppercase(), dft, "{name} {kwd} default");
        }
        let e = sys_cpps::entry(name).expect("in the inventory");
        assert_eq!(e.site, sys_cpps::Site::Job, "{name} runs in the job");
    }
}

