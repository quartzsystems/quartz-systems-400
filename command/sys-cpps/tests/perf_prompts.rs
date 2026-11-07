// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Collection Services commands' prompts and `WRKSYSACT`'s against
//! the primary's captures under `tools/conformance/golden/phase12/perf/`
//! (V7R1M0). `CFGPFRCOL` shows the current configuration through its
//! prompt override; the test gives the values the capture shows, as
//! `QSPMTOVR` would. Rows 5 to 20 are compared: the prompt texts and the
//! values; the prompter's columns are its own.

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
        .join("../../tools/conformance/golden/phase12/perf")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect()
}

/// The prompt of `text` after `keys`: its rows.
fn screen(text: &str, keys: &[u8]) -> Vec<String> {
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
        assert_eq!(p.key(*k, &fields, &mut env), Step::Show, "{text}: key {k}");
    }
    p.screen().rows
}

/// A row's prompt text, dot leaders aside.
fn label(r: &str) -> String {
    // The prompter puts `+ for more values` four columns left of IBM.
    if r.trim() == "Additional Parameters" || r.trim() == "+ for more values" {
        return r.trim().to_string();
    }
    r.chars()
        .take(34)
        .collect::<String>()
        .trim_end_matches(['.', ' '])
        .trim_start()
        .to_string()
}

/// The parameter rows, blank ones aside.
fn rows(v: &[String]) -> Vec<String> {
    v.iter()
        .take(20)
        .skip(4)
        .map(|r| r.trim_end().to_string())
        .filter(|r| !r.is_empty())
        .collect()
}

/// The values a golden row shows after its keyword.
fn values(w: &str) -> Vec<String> {
    if w.trim() == "+ for more values" {
        return Vec::new();
    }
    w.get(34..)
        .unwrap_or("")
        .split_whitespace()
        .filter(|t| {
            *t != ">"
                && !t
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        })
        .map(str::to_string)
        .collect()
}

/// Row by row against golden `name`.
fn check(name: &str, text: &str, keys: &[u8]) {
    let ours = rows(&screen(text, keys));
    let want = rows(&golden_rows(name));
    let mut diffs = Vec::new();
    for (i, w) in want.iter().enumerate() {
        let got = ours.get(i).map_or("", String::as_str);
        let tail = got.get(34..).unwrap_or("");
        if label(got) != label(w) || !values(w).iter().all(|v| tail.contains(v.as_str())) {
            diffs.push(format!("row {}:\n  want {w:?}\n  got  {got:?}", i + 1));
        }
    }
    assert!(diffs.is_empty(), "{name}:\n{}", diffs.join("\n"));
}

/// Every golden row's prompt text is on our display, in any order: for
/// the `F10` displays, where IBM shows `CRTPFRSUM` without the additional
/// parameters' heading and our prompter puts it below one.
fn check_labels(name: &str, text: &str, keys: &[u8]) {
    let ours: Vec<String> = rows(&screen(text, keys)).iter().map(|r| label(r)).collect();
    let missing: Vec<String> = rows(&golden_rows(name))
        .iter()
        .map(|r| label(r))
        .filter(|l| !ours.contains(l))
        .collect();
    assert!(
        missing.is_empty(),
        "{name}: not shown {missing:?}\nours {ours:#?}"
    );
}

/// The configuration the primary shows, as the prompt override gives it.
const PRIMARY_CFG: &str = "INTERVAL(15.00) LIB(QPFRDATA) DFTCOLPRF(*STANDARDP) CYCTIME('000000')      CYCITV(24) RETPERIOD(00120 *HOURS) CRTDBF(*YES) CRTPFRSUM(*NO)";

#[test]
fn collector_commands() {
    check("prompt-strpfrcol", "?STRPFRCOL", &[]);
    check("prompt-endpfrcol", "?ENDPFRCOL", &[]);
    check("prompt-chkpfrcol", "?CHKPFRCOL", &[]);
    check(
        "prompt-cfgpfrcol",
        &format!("?CFGPFRCOL {PRIMARY_CFG}"),
        &[],
    );
    check_labels(
        "prompt-cfgpfrcol-f10",
        &format!("?CFGPFRCOL {PRIMARY_CFG}"),
        &[10],
    );
    check("prompt-crtpfrdta", "?CRTPFRDTA", &[]);
}

#[test]
fn wrksysact() {
    check("prompt-wrksysact", "?WRKSYSACT", &[]);
}

#[test]
fn the_values_override_gives_are_the_configuration() {
    let form = perf::config::Config::default().keyword_form();
    for want in [
        "INTERVAL(15.00)",
        "LIB(QPFRDATA)",
        "DFTCOLPRF(*STANDARDP)",
        "RETPERIOD(00120 *HOURS)",
    ] {
        assert!(
            PRIMARY_CFG.contains(want) && form.contains(want),
            "{want}: {form}"
        );
    }
}

#[test]
fn where_they_run() {
    use sys_cpps::Site;
    for c in [
        "STRPFRCOL",
        "ENDPFRCOL",
        "CFGPFRCOL",
        "CHKPFRCOL",
        "CRTPFRDTA",
    ] {
        assert_eq!(
            sys_cpps::entry(c).map(|e| e.site),
            Some(Site::Service),
            "{c}"
        );
    }
}
