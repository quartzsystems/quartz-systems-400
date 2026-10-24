// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `rpg_bench`: the compiler over the fuzzer's programs (lines a second,
//! front end and code generation apart), and a program's loop of a
//! million decimal EVALs and a hundred thousand subprocedure calls,
//! timed, and the timed members of `rpg/perf/` the oracle runs too;
//! the numbers go into `PHASE9.md`.
//! `cargo test --release -p conformance --test rpg_bench -- --ignored --nocapture`

use std::time::Instant;

use conformance::rpg::{self, fuzz};
use rpg_oracle::case::Case;

const LOOP: &str = "**FREE
ctl-opt dftactgrp(*no);
dcl-f QSYSPRT printer(132);
dcl-ds line len(132) end-ds;
dcl-s i int(10);
dcl-s acc packed(15:2);
dcl-s r packed(9:4);
for i = 1 to 1000000;
  acc += i * 1.25;
  r = %rem(i : 7) / 3;
endfor;
for i = 1 to 100000;
  acc = step(acc);
endfor;
line = %char(acc) + ' ' + %char(r);
write QSYSPRT line;
*inlr = *on;
dcl-proc step;
  dcl-pi *n packed(15:2);
    a packed(15:2) value;
  end-pi;
  return a - 1;
end-proc;
";

#[test]
#[allow(clippy::cast_precision_loss)]
#[ignore = "a benchmark; run with --release --ignored --nocapture"]
fn rpg_bench() {
    // The compiler: 500 fuzzed programs.
    let cases: Vec<Case> = (0..500).map(|s| Case::parse(&format!("FUZ{s:04}"), &fuzz::program(s))).collect();
    let lines: usize = cases.iter().map(|c| c.lines.len()).sum();
    let t = Instant::now();
    for c in &cases {
        let _ = rpgfront::front(rpg::member(c), &mut rpgfront::NoIncludes, &rpgfront::reader::Options::default());
    }
    let front = t.elapsed();
    let t = Instant::now();
    let mut ir = 0usize;
    for c in &cases {
        let out = rpg::compile(c, None);
        assert!(out.created, "{}", c.name);
        ir += out.qir.lines().count();
    }
    let whole = t.elapsed();
    println!(
        "compile: {} programs, {lines} lines, {ir} IR lines; front end {:.1} ms ({:.0} lines/s), \
         to IR {:.1} ms ({:.0} lines/s)",
        cases.len(),
        front.as_secs_f64() * 1e3,
        lines as f64 / front.as_secs_f64(),
        whole.as_secs_f64() * 1e3,
        lines as f64 / whole.as_secs_f64(),
    );

    // The runtime: the loop, compiled, bound and run.
    let case = Case::parse("RPGBENCH", LOOP);
    let t = Instant::now();
    let (o, _, notes) = rpg::run(&case).expect("the bench runs");
    let ran = t.elapsed();
    assert!(o.created, "{notes}");
    let shown: Vec<String> = o.spool.iter().flat_map(|s| s.lines.iter().cloned()).collect();
    println!(
        "run: 1000000 EVAL pairs and 100000 calls in {:.1} ms (compile and bind included): {}",
        ran.as_secs_f64() * 1e3,
        shown.join(" | ").trim()
    );

    // The timed members the oracle runs too (`rpg/perf/`): what each
    // program measured of itself.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("rpg/perf");
    let mut members: Vec<_> = std::fs::read_dir(&dir).expect("rpg/perf").filter_map(|e| e.ok().map(|e| e.path())).collect();
    members.sort();
    for p in members {
        let name = p.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        // The machine-model query is for the oracle's catalog only.
        if p.extension().is_some_and(|e| e == "sqlrpgle") {
            continue;
        }
        let case = Case::load(&p).expect("a member");
        let t = Instant::now();
        let (o, _, notes) = rpg::run(&case).expect("a timed member runs");
        let ran = t.elapsed();
        assert!(o.created, "{name}: {notes}");
        let shown: Vec<String> = o.spool.iter().flat_map(|s| s.lines.iter().cloned()).filter(|l| l.contains(" ms ") || l.contains("cycle")).collect();
        let shown = if shown.is_empty() { vec![format!("{:?} {}", o.run, o.spool.iter().flat_map(|s| s.lines.iter().rev().take(3).cloned()).collect::<Vec<_>>().join(" | "))] } else { shown };
        println!("{name}: {:.1} ms in all: {}", ran.as_secs_f64() * 1e3, shown.join(" | ").trim());
    }
}
