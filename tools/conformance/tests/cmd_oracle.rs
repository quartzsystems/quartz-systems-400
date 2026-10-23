// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The analyzer against the real box: `tools/conformance/cmd/oracle.txt`
//! is the `DSPMSG OUTPUT(*PRINT)` listing `ORACLE.CLLE` leaves, one
//! message `n OK command` or `n CPF0006 CPDxxxx` (the escape and the
//! first diagnostic) per string of `strings.txt`. Each outcome is
//! compared with `expected.txt`: accepted here and there, or the
//! box's first diagnostic against the identifier this analyzer sends
//! (`QCMDCHK` echoes the string as typed, so spellings are not
//! compared). Skipped while the listing is absent.

use std::path::PathBuf;

fn kit() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("cmd")
}

/// `expected.txt`: each string on its own line, its outcome lines
/// indented after it; the first outcome line is what counts.
fn expected() -> Vec<(String, String)> {
    let text = std::fs::read_to_string(kit().join("expected.txt")).expect("expected.txt");
    let mut out: Vec<(String, String)> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("  ") {
            if let Some(last) = out.last_mut()
                && last.1.is_empty()
            {
                last.1 = rest.trim().to_string();
            }
        } else if !line.trim().is_empty() {
            out.push((line.to_string(), String::new()));
        }
    }
    out
}

/// The listing's messages: `(n, outcome)`, the first per `n`.
fn oracle(text: &str) -> Vec<(usize, String)> {
    let mut out: Vec<(usize, String)> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        let Some((n, rest)) = t.split_once(' ') else {
            continue;
        };
        let Ok(n) = n.parse::<usize>() else {
            continue;
        };
        let rest = rest.trim();
        let is_outcome = rest.starts_with("OK ")
            || (rest.len() >= 7
                && rest.as_bytes()[..3].iter().all(u8::is_ascii_uppercase)
                && rest.as_bytes()[3..7].iter().all(u8::is_ascii_hexdigit));
        if is_outcome && out.iter().all(|(m, _)| *m != n) {
            out.push((n, rest.to_string()));
        }
    }
    out
}

/// The diagnostic that says why: the second identifier when the first
/// is the analyzer's escape (`CPF0001` here, `QCMDCHK`'s `CPF0006`
/// there), else the first.
fn first_diagnostic(outcome: &str) -> &str {
    let mut ids = outcome.split_whitespace();
    let first = ids.next().unwrap_or("");
    if matches!(first, "CPF0001" | "CPF0006") {
        ids.next().unwrap_or(first)
    } else {
        first
    }
}

/// Strings whose outcome still differs from the box, each named in
/// `docs/CMD_OPEN_QUESTIONS.md` item 15 (QCMDCHK-only behaviour and
/// validity checkers not yet written). A difference anywhere else
/// fails; one of these that starts to agree should leave the list.
const OPEN: &[usize] = &[
    157, 186, 197, 301, 442, 443, 444, 445, 446, 494, 527, 554, 555, 604, 723, 724, 750, 751, 764,
];

#[test]
fn analyzer_agrees_with_the_box() {
    let path = kit().join("oracle.txt");
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("skipped: {} not captured yet", path.display());
        return;
    };
    let expected = expected();
    let mut differences = Vec::new();
    let mut compared = 0;
    for (n, theirs) in oracle(&text) {
        let Some((string, ours)) = expected.get(n.saturating_sub(1)) else {
            differences.push(format!("{n}: no expected outcome for the box's `{theirs}`"));
            continue;
        };
        compared += 1;
        let same = match (ours.strip_prefix("OK "), theirs.strip_prefix("OK ")) {
            (Some(_), Some(_)) => true,
            (None, None) => first_diagnostic(ours) == first_diagnostic(&theirs),
            _ => false,
        };
        if same && OPEN.contains(&n) {
            differences.push(format!("{n}: agrees now; take it off OPEN"));
        } else if !same && OPEN.contains(&n) {
            eprintln!("still open: {n}: {string}");
        } else if !same {
            differences.push(format!("{n}: {string}\n    ours:   {ours}\n    theirs: {theirs}"));
        }
    }
    assert!(compared > 0, "the listing held no outcomes");
    assert!(
        differences.is_empty(),
        "{} of {compared} outcomes differ:\n{}",
        differences.len(),
        differences.join("\n")
    );
}

#[test]
fn the_listing_parser_reads_dspmsg_output() {
    let text = "  Display Messages\n  Queue . . : ORACLE\n   1 OK CRTLIB LIB(ABC)\n   2 CPF0006 CPD0012\n   2 CPD0012 ignored\n Bottom\n";
    assert_eq!(
        oracle(text),
        vec![
            (1, "OK CRTLIB LIB(ABC)".to_string()),
            (2, "CPF0006 CPD0012".to_string())
        ]
    );
    let e = expected();
    assert!(!e.is_empty() && !e[0].1.is_empty(), "expected.txt reads as pairs");
}
