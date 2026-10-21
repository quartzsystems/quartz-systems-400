// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `callbench`: a million bound calls and a hundred thousand dynamic
//! program calls, timed; the numbers go into `PHASE4.md`.
//! `cargo test --release -p conformance --test callbench -- --ignored --nocapture`

use std::time::Instant;

use conformance::qir::{Case, program};
use mi::{Job, MemSource};

const BOUND: &str = "
module CALLBENCH
static $acc: i64
proc STEP(byval $v: i64) -> i64 {
L0:
  %v = load $v
  %w = add %v, 1
  ret %w
}
proc CALLBENCH() entry {
L0:
  %i = const.i64 0
  %n = const.i64 1000000
  br TEST
TEST:
  %done = cmp.ge %i, %n
  brif %done, OUT, BODY
BODY:
  %i = callb STEP(%i)
  br TEST
OUT:
  store $acc, %i
  dsply $acc
  ret
}
";

const DYNAMIC: &str = "
module DYNBENCH
static $acc: i64
proc DYNBENCH() entry {
L0:
  %i = const.i64 0
  %n = const.i64 100000
  br TEST
TEST:
  %done = cmp.ge %i, %n
  brif %done, OUT, BODY
BODY:
  call BUMP()
  %i = add %i, 1
  br TEST
OUT:
  dsply \"done\"
  ret
}
====
module BUMP
static $n: i64
proc BUMP() entry {
L0:
  %n = load $n
  %m = add %n, 1
  store $n, %m
  ret
}
";

fn run(text: &str) -> (Box<Job>, std::time::Duration) {
    let case = Case::parse("BENCH", text);
    let mut src = MemSource::default();
    let mut main = None;
    for m in &case.modules {
        let p = program(m).unwrap_or_else(|e| panic!("{e}"));
        if main.is_none() {
            main = Some(p.name.clone());
        }
        src.add("QGPL", p);
    }
    let mut job = Job::boxed(Box::new(src));
    job.stack_bytes = 8 << 20;
    let start = Instant::now();
    job.call_program(main.as_deref().unwrap(), "QGPL", &[])
        .unwrap();
    (job, start.elapsed())
}

#[test]
#[ignore = "a benchmark; run with --ignored --nocapture in release"]
fn callbench() {
    let (job, bound) = run(BOUND);
    assert_eq!(job.console, vec!["1000000"]);
    let (job2, dynamic) = run(DYNAMIC);
    assert_eq!(job2.console, vec!["done"]);
    let per_bound = bound.as_nanos() / 1_000_000;
    let per_dyn = dynamic.as_nanos() / 100_000;
    println!("callbench: 1e6 bound calls in {bound:?} ({per_bound} ns each)");
    println!("callbench: 1e5 dynamic calls in {dynamic:?} ({per_dyn} ns each)");
}
