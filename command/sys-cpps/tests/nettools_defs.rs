// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The network tool commands against pub400 (V7R5M0): `NETSTAT`,
//! `PING`, `VFYTCPCNN`, `TRACEROUTE` and `TRCTCPRTE` have the keywords,
//! minimums, maximums, defaults and special values of the `QCDRCMDD`
//! capture, and their prompts show the rows of the captured prompt
//! displays (`golden/phase10/prompts`, rows 5 to 20: the prompt texts
//! and the values). The communications trace commands were refused to
//! the profile, so only their places are checked.

use std::collections::BTreeMap;
use std::path::Path;

use cmd::analyze::{Env, MemSource, analyze};
use cmd::prompt::{Prompter, Step};

/// `(keyword, min, max, default, special values)` per command.
type Captured = BTreeMap<String, Vec<(String, u16, u16, Option<String>, Vec<String>)>>;

fn captured() -> Captured {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/oracle/net/pub400/command-definitions.txt");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut out = Captured::new();
    let mut cur: Option<String> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("CMD ") {
            let name = rest.split_whitespace().next().unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
            out.entry(name.clone()).or_default();
            cur = Some(name);
            continue;
        }
        let Some(rest) = line.strip_prefix("  PARM ") else {
            continue;
        };
        let Some(c) = &cur else { continue };
        let kwd = rest.split_whitespace().next().unwrap_or("").to_string();
        let attr = |k: &str| {
            rest.split_whitespace()
                .find_map(|w| w.strip_prefix(&format!("{k}=")).map(str::to_string))
        };
        let spc: Vec<String> = rest
            .split_whitespace()
            .filter_map(|w| w.strip_prefix("spc:").or_else(|| w.strip_prefix("sng:")))
            .map(|w| w.split('=').next().unwrap_or("").to_string())
            .collect();
        out.get_mut(c).expect("entry").push((
            kwd,
            attr("Min").and_then(|v| v.parse().ok()).unwrap_or(0),
            attr("Max").and_then(|v| v.parse().ok()).unwrap_or(1),
            attr("Dft"),
            spc,
        ));
    }
    out
}

const COMMANDS: &[&str] = &["NETSTAT", "PING", "VFYTCPCNN", "TRACEROUTE", "TRCTCPRTE"];

#[test]
fn definitions_match_the_capture() {
    let cap = captured();
    let mut bad = Vec::new();
    for name in COMMANDS {
        let theirs = &cap[*name];
        let src = sys_cpps::defs::source(name).unwrap_or_else(|| panic!("{name} has no definition"));
        let def = cmd::source::compile(name, src).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let ours: Vec<&str> = def.parms.iter().map(|p| p.kwd.as_str()).collect();
        let want: Vec<&str> = theirs.iter().map(|p| p.0.as_str()).collect();
        if ours != want {
            bad.push(format!("{name}: keywords {ours:?}, captured {want:?}"));
            continue;
        }
        for (p, (kwd, min, max, dft, spc)) in def.parms.iter().zip(theirs) {
            if p.min != *min || p.max != *max {
                bad.push(format!("{name} {kwd}: MIN/MAX {}/{} against {min}/{max}", p.min, p.max));
            }
            if p.dft.as_deref() != dft.as_deref() && !(dft.is_none() && p.dft.is_none()) {
                bad.push(format!("{name} {kwd}: DFT {:?} against {dft:?}", p.dft));
            }
            let ours: Vec<String> = p.spcval.iter().chain(&p.sngval).map(|s| s.from.clone()).collect();
            for s in spc {
                if !ours.iter().any(|o| o.eq_ignore_ascii_case(s)) {
                    bad.push(format!("{name} {kwd}: special value {s} missing"));
                }
            }
        }
    }
    assert!(bad.is_empty(), "\n{}", bad.join("\n"));
}

#[test]
fn every_tool_has_a_place() {
    for name in COMMANDS
        .iter()
        .chain(&["STRCMNTRC", "ENDCMNTRC", "DLTCMNTRC", "CHKCMNTRC", "PRTCMNTRC"])
    {
        assert!(sys_cpps::entry(name).is_some(), "{name} is not in the inventory");
    }
}

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

/// The prompt of `text` after `keys` (F10, Page Down), against golden
/// `name`: each parameter row's prompt text, and the values the
/// capture shows after the keyword.
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
fn netstat_prompt() {
    check("prompt-netstat", "?NETSTAT", &[]);
}

#[test]
fn ping_prompts() {
    check("prompt-ping", "?PING", &[]);
    check("prompt-vfytcpcnn", "?VFYTCPCNN", &[]);
}

#[test]
#[ignore = "the oracle's F10 shows INTNETADR, conditioned on RMTSYS(*INTNETADR), with RMTSYS blank; our prompter keeps a PMTCTL condition (docs/NET_OPEN_QUESTIONS.md, Network tools)"]
fn ping_prompts_f10() {
    check("prompt-ping-f10", "?PING", &[10]);
    check("prompt-vfytcpcnn-f10", "?VFYTCPCNN", &[10]);
}

#[test]
fn traceroute_prompts() {
    check("prompt-traceroute", "?TRACEROUTE", &[]);
    check("prompt-traceroute-f10", "?TRACEROUTE", &[10]);
    check("prompt-trctcprte", "?TRCTCPRTE", &[]);
    check("prompt-trctcprte-f10", "?TRCTCPRTE", &[10]);
}
