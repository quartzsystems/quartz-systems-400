// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The TCP/IP configuration commands against the oracle's `QCDRCMDD`
//! capture (pub400, V7R5M0): the same keywords in the same order, each
//! with the capture's minimum, maximum, default and special values.

use std::collections::BTreeMap;
use std::path::Path;

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

const COMMANDS: &[&str] = &[
    "ADDTCPHTE", "ADDTCPIFC", "ADDTCPPORT", "ADDTCPRTE", "ADDTCPSVR", "CFGTCP", "CFGTCPAPP", "CHGTCPA",
    "CHGTCPDMN", "CHGTCPHTE", "CHGTCPIFC", "CHGTCPRTE", "CHGTCPSVR", "CHGTELNA", "CHGFTPA", "FTP", "STRTCPFTP", "CPYTCPHT", "MRGTCPHT", "RMVTCPHTE",
    "RMVTCPIFC", "RMVTCPPORT", "RMVTCPRTE", "RMVTCPSVR", "RNMTCPHTE",
];

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
fn every_tcp_command_has_a_place() {
    for name in COMMANDS
        .iter()
        .chain(&["STRTCP", "ENDTCP", "ENDTCPABN", "STRTCPIFC", "ENDTCPIFC", "WRKSRVTBLE", "ADDSRVTBLE"])
    {
        assert!(sys_cpps::entry(name).is_some(), "{name} is not in the inventory");
    }
}

/// `CHGTELNA`'s values as the job reads them: numbers with their
/// special values mapped, the table's `*SAME` a value of its own.
#[test]
fn chgtelna_values() {
    let mut src = cmd::analyze::MemSource::default();
    src.put(sys_cpps::install::object("CHGTELNA", 0).unwrap().def);
    let mut env = cmd::analyze::Env::new(&mut src);
    let r = cmd::analyze(&mut env, "CHGTELNA TIMMRKTIMO(600) NBRSVR(*CALC) CCSID(*GERMAN)").unwrap();
    let p = cmd::block::Parms::from_bound(&r.def, r.parms.clone());
    assert_eq!(p.int("TIMMRKTIMO"), Some(600));
    assert_eq!(p.int("NBRSVR"), Some(-2));
    assert_eq!(p.int("NBRTASK"), Some(-1));
    assert_eq!(p.int("CCSID"), Some(17_368_051));
    assert_eq!(p.int("NBRFLDS"), Some(-2));
    let tbl = p.get("TBLVTOUT").unwrap();
    let first = match tbl {
        cmd::Value::Qual(parts) => parts[0].source(),
        other => other.source(),
    };
    assert_eq!(first, "*SAME");
    assert_eq!(p.name("ALWSSL"), "*SAME");
}
