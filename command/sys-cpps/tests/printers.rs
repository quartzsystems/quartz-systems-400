// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The printer commands' prompts against pub400's captures under
//! `tools/conformance/golden/phase10/prompts/` (V7R5M0): `CRTDEVPRT`'s
//! key-parameter display, `CRTWSCST` and `STRRMTWTR` with and without
//! `F10`. `CRTDEVPRT`'s `F10` display and the remote `CRTOUTQ` are
//! IBM's prompt-control program at work and are not matched
//! (`docs/NET_OPEN_QUESTIONS.md`, Printers). Rows 5 to 20 are compared:
//! the prompt texts and the values; the prompter's columns are its own.

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
    let rows = |v: &[String]| -> Vec<String> {
        v.iter()
            .take(20)
            .skip(4)
            .map(|r| r.trim_end().to_string())
            .filter(|r| !r.is_empty())
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
fn crtdevprt_key_parameters() {
    check("prompt-crtdevprt-lan", "?CRTDEVPRT DEVCLS(*LAN)", &[]);
    check(
        "prompt-crtdevprt-ip",
        "?CRTDEVPRT DEVCLS(*LAN) TYPE(3812) MODEL(1)",
        &[],
    );
}

#[test]
fn crtwscst() {
    check("prompt-crtwscst", "?CRTWSCST", &[]);
    check("prompt-crtwscst-f10", "?CRTWSCST", &[10]);
}

#[test]
fn strrmtwtr() {
    check("prompt-strrmtwtr", "?STRRMTWTR", &[]);
    check("prompt-strrmtwtr-f10", "?STRRMTWTR", &[10]);
}

#[test]
fn crtdevprt_ip_parameters_follow_lanattach() {
    // After the key parameters, `LANATTACH(*IP)` brings the port and
    // the rest IBM's F10 display lists for it.
    let mut src = source();
    let r = {
        let mut env = Env::new(&mut src);
        env.setting.interactive = true;
        analyze(
            &mut env,
            "?CRTDEVPRT DEVD(LJ4) DEVCLS(*LAN) TYPE(3812) MODEL(1) LANATTACH(*IP)",
        )
        .unwrap()
    };
    let mut p = Prompter::new(&r);
    let fields: Vec<String> = p.screen().fields.iter().map(|f| f.value.clone()).collect();
    let mut env = Env::new(&mut src);
    env.setting.interactive = true;
    assert_eq!(p.key(0, &fields, &mut env), Step::Show);
    let text = p.screen().text();
    let want = golden_rows("prompt-crtdevprt-ip-f10");
    for label in ["LANATTACH", "PORT", "ONLINE", "FONT", "FORMFEED", "SEPDRAWER", "SEPPGM"] {
        let row = want.iter().find(|r| r.contains(label)).unwrap();
        let prompt = row.trim_start().split(" . ").next().unwrap();
        let prompt = prompt.split("  ").next().unwrap().trim();
        assert!(text.contains(prompt), "{label}: {prompt:?} not shown");
    }
    assert!(!text.contains("Remote location"), "RMTLOCNAME waits for PORT");
}
