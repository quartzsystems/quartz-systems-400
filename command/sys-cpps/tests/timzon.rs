// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The time zone commands' prompts against the primary's captures under
//! `tools/conformance/golden/verified/oracle71/timzon/` (V7R1M0), rows 5
//! to 20 as `phase12.rs` compares them: the prompt texts and the values.
//! The second page after F10 is not compared: IBM keeps the DSTEND group
//! whole on it, the prompter breaks the page inside the group.

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
        .join("../../tools/conformance/golden/verified/oracle71/timzon")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect()
}

/// The prompt of `text` after `keys` (10 for F10, 26 for Page Down)
/// against golden `name`, row by row.
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
    let rows = |v: &[String]| -> Vec<String> {
        v.iter()
            .take(20)
            .skip(4)
            .map(|r| r.trim_end().to_string())
            .filter(|r| !r.is_empty())
            .collect()
    };
    let ours = rows(&s.rows);
    let mut diffs = Vec::new();
    for (i, w) in rows(&want).iter().enumerate() {
        let got = ours.get(i).map_or("", String::as_str);
        let values: Vec<&str> = w
            .get(34..)
            .unwrap_or("")
            .split_whitespace()
            .filter(|t| {
                *t != ">"
                    && !t
                        .chars()
                        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            })
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
fn time_zone_prompts() {
    check("prompt-crttimzon", "?CRTTIMZON", &[]);
    check("prompt-crttimzon-f10", "?CRTTIMZON", &[10]);
    check(
        "prompt-crttimzon-dst",
        "?CRTTIMZON TIMZON(QSPROBE) OFFSET(-300) DSTNAME(QDT)",
        &[],
    );
    check(
        "prompt-crttimzon-dst-f10",
        "?CRTTIMZON TIMZON(QSPROBE) OFFSET(-300) DSTNAME(QDT)",
        &[10],
    );
    check("prompt-dlttimzon", "?DLTTIMZON", &[]);
    check("prompt-wrktimzon", "?WRKTIMZON", &[]);
}
