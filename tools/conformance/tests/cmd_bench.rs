// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 7 benches, timed in memory: the analyzer over the kit's
//! strings, the prompter's first displays, and the CL compiler over
//! the corpus. `cargo test --release -p conformance --test cmd_bench --
//! --ignored --nocapture` prints the rates `docs/PHASE7.md` quotes.

#![allow(clippy::cast_precision_loss)] // rates, not sums

use std::time::Instant;

use cmd::analyze::{Env, Setting, analyze};
use cmd::prompt::Prompter;
use conformance::cl::{Kit, compile};
use conformance::cmdkit::{defs, run, strings};

#[test]
#[ignore = "a bench: run with --ignored --nocapture in release"]
fn analyzer_strings_per_second() {
    let strings = strings().expect("the kit's strings");
    let rounds = 20;
    let t = Instant::now();
    let mut lines = 0;
    for _ in 0..rounds {
        lines += run(&strings).len();
    }
    let secs = t.elapsed().as_secs_f64();
    eprintln!(
        "analyzer: {lines} strings in {secs:.2}s = {:.0} strings/s",
        lines as f64 / secs
    );
}

#[test]
#[ignore = "a bench: run with --ignored --nocapture in release"]
fn prompter_screens_per_second() {
    let mut src = defs();
    let mut env = Env::new(&mut src);
    env.setting = Setting {
        interactive: true,
        program: true,
        exec: false,
        module: false,
        syntax_only: false,
    };
    let texts = [
        "CRTPF FILE(QGPL/CUST) RCDLEN(80)",
        "SBMJOB CMD(CALL QGPL/HELLO) JOB(HELLO)",
        "CHGJOB",
        "CRTUSRPRF USRPRF(ANNE)",
        "CPYF FROMFILE(QGPL/CUST) TOFILE(QGPL/CUST2)",
    ];
    let resolved: Vec<_> = texts
        .iter()
        .map(|t| analyze(&mut env, t).expect("analyzes"))
        .collect();
    let rounds = 200;
    let t = Instant::now();
    let mut screens: usize = 0;
    for _ in 0..rounds {
        for r in &resolved {
            let p = Prompter::new(r);
            let s = p.screen();
            assert_eq!(s.rows.len(), 24);
            screens += 1;
        }
    }
    let secs = t.elapsed().as_secs_f64();
    eprintln!(
        "prompter: {screens} first displays in {secs:.2}s = {:.0} screens/s",
        screens as f64 / secs
    );
}

#[test]
#[ignore = "a bench: run with --ignored --nocapture in release"]
fn cl_programs_per_second() {
    let kit = Kit::locate();
    let cases = kit.cases().expect("the corpus");
    let rounds = 5;
    let t = Instant::now();
    let mut programs: usize = 0;
    let mut statements: usize = 0;
    for _ in 0..rounds {
        for c in &cases {
            let out = compile(c);
            statements += out.listing.len();
            programs += 1;
        }
    }
    let secs = t.elapsed().as_secs_f64();
    eprintln!(
        "cl: {programs} programs ({statements} listing lines) in {secs:.2}s = {:.1} programs/s",
        programs as f64 / secs
    );
}
