// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Retranslation of code made by an older code generator, and the
//! native stack budget: runaway recursion ends in `MCH4429` that the
//! program can monitor.

use ile::{ActGrpAttr, BindInput, BindOptions, CreateData, ModuleObject, PgmKind};
use mi::{Cond, Job, MemSource};

fn program(text: &str) -> ile::ProgramObject {
    let m = ModuleObject::compile(qir::parse(text).unwrap(), CreateData::default(), 0).unwrap();
    let mut opts = BindOptions::new(&m.name.clone(), PgmKind::Pgm);
    opts.attr.actgrp = ActGrpAttr::Caller;
    ile::bind(BindInput {
        modules: vec![(m, "QGPL".into())],
        bnddirs: Vec::new(),
        lookup: &|_| None,
        options: opts,
    })
    .unwrap()
    .0
}

#[test]
fn old_code_is_translated_again_from_the_ir() {
    let mut p = program(
        "module OLD\nstatic $n: i32 inz(20)\nproc OLD() entry {\nL0:\n  %n = load $n\n  %m = mul %n, 2\n  dsply %m\n  ret\n}\n",
    );
    // Pretend the stored code came from an earlier code generator and
    // is unusable: zero it out as well, so running it would crash.
    p.modules[0].module.code.version = 0;
    p.modules[0].module.code.code.fill(0xCC);
    let mut src = MemSource::default();
    src.add("QGPL", p);
    let mut job = Job::boxed(Box::new(src));
    job.call_program("OLD", "*LIBL", &[]).unwrap();
    assert_eq!(job.console, vec!["40"]);
    assert_eq!(job.act.retranslated, 1);
    job.call_program("OLD", "*LIBL", &[]).unwrap();
    assert_eq!(job.act.retranslated, 1, "the loaded copy is reused");
}

#[test]
fn runaway_recursion_is_mch4429_at_the_stack_budget() {
    let p = program(
        "module DEEP\nstatic $depth: i32\nproc DOWN(byval $n: i32) {\nL0:\n  %n = load $n\n  store $depth, %n\n  %m = add %n, 1\n  callb DOWN(%m)\n  ret\n}\nproc DEEP() entry {\nL0:\n  monitor M, MCH4429 -> OVER\n  callb DOWN(1)\n  dsply \"not reached\"\n  endmon M\n  ret\nOVER:\n  dsply \"stack exhausted\"\n  ret\n}\n",
    );
    let mut src = MemSource::default();
    src.add("QGPL", p);
    let mut job = Job::boxed(Box::new(src));
    job.stack_bytes = 256 * 1024;
    job.call_program("DEEP", "*LIBL", &[]).unwrap();
    assert_eq!(job.console, vec!["stack exhausted"]);
    assert_eq!(job.handled.len(), 1);
    assert_eq!(job.handled[0].cond, Some(Cond::Mch4429));
    assert_eq!(job.depth, 0, "every entry was left");
    assert!(job.max_depth > 100, "it got deep first: {}", job.max_depth);
    assert!(!job.pending());
}
