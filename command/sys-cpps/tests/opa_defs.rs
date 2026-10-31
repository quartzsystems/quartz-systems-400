// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The cleanup and backup commands' prompts against pub400's captures
//! under `tools/conformance/golden/phase11/prompts/` (V7R5M0). IBM's
//! `CHGCLNUP` shows the current options through its prompt override
//! program; the test gives the values the capture shows, as our
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
        .join("../../tools/conformance/golden/phase11/prompts")
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
        .filter(|t| *t != ">" && !t.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
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
/// the `F10` displays, where IBM shows `JRNRCVSIZ` above the additional
/// parameters and our prompter puts every `*PMTRQS` one below.
fn check_labels(name: &str, text: &str, keys: &[u8]) {
    let ours: Vec<String> = rows(&screen(text, keys)).iter().map(|r| label(r)).collect();
    let missing: Vec<String> = rows(&golden_rows(name))
        .iter()
        .map(|r| label(r))
        .filter(|l| !ours.contains(l))
        .collect();
    assert!(missing.is_empty(), "{name}: not shown {missing:?}\nours {ours:#?}");
}

/// The options pub400's cleanup has, as the prompt override gives them.
const PUB400_CLNUP: &str = "ALWCLNUP(*YES) STRTIME('HH:MM:SS') USRMSG(31) SYSMSG(31) \
     CRITSYSMSG(*KEEP) SYSPRT(14) SYSLOG(14) JOBQ(*LIBL/QCTL) JRNRCVSIZ(0000100000) \
     CALITM(30) RUNPTY(55)";

#[test]
fn chgclnup() {
    check("prompt-chgclnup", &format!("?CHGCLNUP {PUB400_CLNUP}"), &[]);
    check_labels("prompt-chgclnup-f10", &format!("?CHGCLNUP {PUB400_CLNUP}"), &[10]);
}

#[test]
fn strclnup() {
    check("prompt-strclnup", "?STRCLNUP", &[]);
}

#[test]
fn chgbckup() {
    check("prompt-chgbckup", "?CHGBCKUP", &[]);
    check("prompt-chgbckup-f10", "?CHGBCKUP BCKUPOPT(*DAILY)", &[0]);
    check("prompt-chgbckup-f10-2", "?CHGBCKUP BCKUPOPT(*DAILY)", &[0, 8]);
}

#[test]
fn runbckup_and_dspbckup() {
    check("prompt-runbckup", "?RUNBCKUP", &[]);
    check("prompt-runbckup-f10", "?RUNBCKUP BCKUPOPT(*DAILY)", &[0]);
    check("prompt-dspbckup", "?DSPBCKUP", &[]);
}

#[test]
fn where_they_run() {
    use sys_cpps::Site;
    for c in ["CHGCLNUP", "STRCLNUP", "ENDCLNUP", "CHGBCKUP", "PRTSYSINF", "QSPMTOVR", "QSBCKUP"] {
        assert_eq!(sys_cpps::entry(c).map(|e| e.site), Some(Site::Service), "{c}");
    }
    for c in ["QSMNSAVE", "QSMNRST", "RUNBCKUP", "DSPBCKUP", "DSPBCKSTS"] {
        assert_eq!(sys_cpps::entry(c).map(|e| e.site), Some(Site::Job), "{c}");
    }
    assert!(sys_cpps::inventory::in_service_too("DSPBCKUP"), "DSPBCKUP's text is the service's");
}
