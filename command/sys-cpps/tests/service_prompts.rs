// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `DSPPRB`'s prompt against the pub400 captures under
//! `tools/conformance/golden/phase11/prompts/` (rows 5 to 20: labels and
//! the values shown), page by page. The other service commands were
//! refused by pub400; their definitions are checked for their place.

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
        .join("../../tools/conformance/golden/phase11/prompts")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect()
}

fn label(r: &str) -> String {
    r.chars()
        .take(34)
        .collect::<String>()
        .trim_end_matches(['.', ' ', ':'])
        .trim_start()
        .to_string()
}

fn body(v: &[String]) -> Vec<String> {
    v.iter()
        .take(20)
        .skip(4)
        .map(|r| r.trim_end().to_string())
        .filter(|r| !r.is_empty() && !r.trim().starts_with('+'))
        .collect()
}

/// The prompt of `text` after `keys` against golden `name`.
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
    let ours = body(&s.rows);
    let mut diffs = Vec::new();
    for (i, w) in body(&golden_rows(name)).iter().enumerate() {
        let got = ours.get(i).map_or("", String::as_str);
        let shown: Vec<&str> = w
            .get(34..)
            .unwrap_or("")
            .split_whitespace()
            .filter(|t| !t.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
            .collect();
        let tail = got.get(34..).unwrap_or("");
        if label(got) != label(w) || !shown.iter().all(|v| tail.contains(v)) {
            diffs.push(format!("row {}:\n  want {w:?}\n  got  {got:?}", i + 1));
        }
    }
    assert!(diffs.is_empty(), "{name}:\n{}\n\nours:\n{}", diffs.join("\n"), s.rows.join("\n"));
}

#[test]
fn dspprb_prompt_pages() {
    check("prompt-dspprb", "?DSPPRB", &[]);
    check("prompt-dspprb-2", "?DSPPRB", &[8]);
    check("prompt-dspprb-f10-3", "?DSPPRB", &[8, 8]);
}

#[test]
fn the_service_commands_have_their_place() {
    for name in [
        "ANZPRB", "SNDSRVRQS", "WRKCNTINF", "CHGCNTINF", "DSPSRVSTS", "WRKSRVPVD", "DMPOBJ", "DMPSYSOBJ",
        "DMPJOB", "STRTRC", "ENDTRC", "PRTTRC", "TRCJOB", "TRCINT", "RCLDBXREF",
    ] {
        assert!(sys_cpps::entry(name).is_some(), "{name} has no definition");
    }
}
