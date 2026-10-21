// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Programs calling programs: activation groups, static storage per
//! activation, service programs with signatures, procedure pointers,
//! reclaiming groups, and calls that fail.

#![allow(clippy::similar_names)]

use ile::binder::{Resolved, SrvPgmInfo};
use ile::{
    ActGrpAttr, BindInput, BindOptions, BndDir, BndDirEntry, BndEntryKind, CreateData, ExportKind,
    ModuleObject, PgmKind, ProgramObject, Signature,
};
use mi::{Cond, Job, MemSource};

fn module(text: &str) -> ModuleObject {
    ModuleObject::compile(
        qir::parse(text).unwrap_or_else(|e| panic!("{e}")),
        CreateData::default(),
        0,
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn program(text: &str, actgrp: ActGrpAttr) -> ProgramObject {
    let m = module(text);
    let mut opts = BindOptions::new(&m.name.clone(), PgmKind::Pgm);
    opts.attr.actgrp = actgrp;
    opts.allow_unresolved = true;
    ile::bind(BindInput {
        modules: vec![(m, "QGPL".into())],
        bnddirs: Vec::new(),
        lookup: &|_| None,
        options: opts,
    })
    .unwrap_or_else(|e| panic!("{e}"))
    .0
}

/// A program that counts its calls in a static and displays the count.
fn counter(name: &str, actgrp: ActGrpAttr) -> ProgramObject {
    let text = format!(
        "module {name}\nstatic $n: i32\nproc {name}() entry {{\nL0:\n  %n = load $n\n  %n2 = add %n, 1\n  store $n, %n2\n  dsply $n\n  ret\n}}\n"
    );
    program(&text, actgrp)
}

#[test]
fn activation_groups_scope_static_storage() {
    let mut src = MemSource::default();
    src.add("QGPL", counter("NEWPGM", ActGrpAttr::New));
    src.add("QGPL", counter("NAMED", ActGrpAttr::Named("GRP1".into())));
    src.add("QGPL", counter("CALLER", ActGrpAttr::Caller));
    src.add("QGPL", counter("DFT", ActGrpAttr::Default));
    src.add(
        "QGPL",
        program(
            "module MAIN\nproc MAIN() entry {\nL0:\n  call NEWPGM()\n  call NEWPGM()\n  call NAMED()\n  call NAMED()\n  call CALLER()\n  call CALLER()\n  call DFT()\n  ret\n}\n",
            ActGrpAttr::Named("MAINGRP".into()),
        ),
    );
    let mut job = Job::boxed(Box::new(src));
    job.call_program("MAIN", "*LIBL", &[]).unwrap();
    assert_eq!(job.console, vec!["1", "1", "1", "2", "1", "2", "1"]);
    assert!(!job.pending());
    assert_eq!(job.depth, 0);
    let groups: Vec<(String, usize)> = job
        .groups()
        .map(|g| (g.name.clone(), g.activations.len()))
        .collect();
    assert_eq!(
        groups,
        vec![
            ("*DFTACTGRP".to_string(), 1),
            ("MAINGRP".to_string(), 2),
            ("GRP1".to_string(), 1)
        ],
        "the *NEW groups are gone, CALLER shares MAIN's group"
    );
    // The named group keeps its activation across a second top-level call.
    job.call_program("NAMED", "*LIBL", &[]).unwrap();
    assert_eq!(job.console.last().map(String::as_str), Some("3"));
    job.rclactgrp("GRP1").unwrap();
    job.call_program("NAMED", "*LIBL", &[]).unwrap();
    assert_eq!(
        job.console.last().map(String::as_str),
        Some("1"),
        "reclaimed: statics start over"
    );
    assert!(job.rclactgrp("NOSUCH").is_err());
}

const UTIL: &str = "module UTIL\nstatic $n: i32\nproc BUMP() -> i32 export {\nL0:\n  %n = load $n\n  %n2 = add %n, 1\n  store $n, %n2\n  ret %n2\n}\nproc TWICE(byval $v: i32) -> i32 export {\nL0:\n  %v = load $v\n  %r = mul %v, 2\n  ret %r\n}\n";

fn service_program(text: &str, actgrp: ActGrpAttr) -> ProgramObject {
    let m = module(text);
    let mut opts = BindOptions::new(&m.name.clone(), PgmKind::SrvPgm);
    opts.attr.actgrp = actgrp;
    ile::bind(BindInput {
        modules: vec![(m, "QGPL".into())],
        bnddirs: Vec::new(),
        lookup: &|_| None,
        options: opts,
    })
    .unwrap()
    .0
}

fn bound_to(text: &str, srv: &ProgramObject, sig: Option<Signature>) -> ProgramObject {
    let m = module(text);
    let info = SrvPgmInfo {
        name: srv.name.clone(),
        lib: "QGPL".into(),
        exports: srv
            .exports
            .iter()
            .map(|e| (e.name.clone(), e.kind))
            .collect(),
        signature: sig.unwrap_or_else(|| srv.signature().unwrap()),
    };
    let dir = BndDir {
        entries: vec![BndDirEntry {
            name: srv.name.clone(),
            lib: "QGPL".into(),
            kind: BndEntryKind::SrvPgm,
            defer: false,
        }],
    };
    let lookup = move |_: &BndDirEntry| Some(Resolved::SrvPgm(info.clone()));
    ile::bind(BindInput {
        modules: vec![(m, "QGPL".into())],
        bnddirs: vec![dir],
        lookup: &lookup,
        options: BindOptions::new("MAIN", PgmKind::Pgm),
    })
    .unwrap()
    .0
}

const MAIN_SRV: &str = "module MAIN\nextern proc BUMP() -> i32\nextern proc TWICE(byval $v: i32) -> i32\nstatic $out: i32\nproc MAIN() entry {\nL0:\n  %a = callb BUMP()\n  %b = callb BUMP()\n  %c = callb TWICE(%b)\n  store $out, %c\n  dsply $out\n  ret\n}\n";

#[test]
fn service_programs_run_in_their_own_activation() {
    let util = service_program(UTIL, ActGrpAttr::Caller);
    let main = bound_to(MAIN_SRV, &util, None);
    assert_eq!(main.srvpgms.len(), 1);
    let mut src = MemSource::default();
    src.add("QGPL", util);
    src.add("QGPL", main);
    let mut job = Job::boxed(Box::new(src));
    job.call_program("MAIN", "*LIBL", &[]).unwrap();
    assert_eq!(job.console, vec!["4"]);
    job.call_program("MAIN", "QGPL", &[]).unwrap();
    assert_eq!(
        job.console,
        vec!["4", "8"],
        "the service program's static persists in the caller's group"
    );
    assert!(!job.pending());
    assert_eq!(job.depth, 0);
}

#[test]
fn a_changed_signature_is_mch4431() {
    let util = service_program(UTIL, ActGrpAttr::Caller);
    let stale = bound_to(
        MAIN_SRV,
        &util,
        Some(Signature::of_exports(&["OLD".into()])),
    );
    let mut src = MemSource::default();
    src.add("QGPL", util);
    src.add("QGPL", stale);
    let mut job = Job::boxed(Box::new(src));
    assert_eq!(job.call_program("MAIN", "*LIBL", &[]), Err(Cond::Mch4431));
    assert!(job.console.is_empty());
}

#[test]
fn procedure_pointers_and_missing_programs() {
    let text = "module PP\nproto FN(byval $v: i32) -> i32\nstatic $r: i32\nproc DOUBLE(byval $v: i32) -> i32 {\nL0:\n  %v = load $v\n  %d = mul %v, 2\n  ret %d\n}\nproc PP() entry {\n  auto $x: i32 inz(21)\nL0:\n  %f = procaddr DOUBLE\n  %r = callpp %f as FN(21)\n  store $r, %r\n  dsply $r\n  monitor M, MCH3401 -> GONE\n  call NOSUCH($x)\n  dsply \"not reached\"\n  endmon M\n  ret\nGONE:\n  dsply \"no program\"\n  %z = const.procptr *NULL\n  %q = callpp %z as FN(1)\n  dsply \"not reached either\"\n  ret\n}\n";
    let mut src = MemSource::default();
    src.add("QGPL", program(text, ActGrpAttr::Caller));
    let mut job = Job::boxed(Box::new(src));
    job.call_program("PP", "*LIBL", &[]).unwrap();
    assert_eq!(job.console, vec!["42", "no program"]);
    assert!(job.pending(), "the null procedure pointer percolates out");
    // Unhandled at the control boundary: function check, inquiry
    // answered C, and CEE9901 left pending for the job.
    assert_eq!(job.exception.as_ref().unwrap().msgid, "CEE9901");
    assert!(job.raised.iter().any(|e| e.cond == Some(Cond::Mch3601)));
    assert!(job.raised.iter().any(|e| e.msgid == "CPF9999"));
    assert_eq!(job.inquiry_log, vec!["C"]);
    assert_eq!(job.handled.len(), 1);
    assert_eq!(job.handled[0].cond, Some(Cond::Mch3401));
}

#[test]
fn parameters_pass_through_a_dynamic_call() {
    let callee = program(
        "module ADDER\nproc ADDER($a: i32, $b: i32, $sum: i32) entry {\nL0:\n  %n = parms\n  %a = load $a\n  %b = load $b\n  %s = add %a, %b\n  store $sum, %s\n  dsply %n\n  ret\n}\n",
        ActGrpAttr::Caller,
    );
    let caller = program(
        "module CALLER\nstatic $x: i32 inz(40)\nstatic $y: i32 inz(2)\nstatic $z: i32\nstatic $name: char(10) inz(\"ADDER\")\nproc CALLER() entry {\nL0:\n  call ADDER($x, $y, $z)\n  dsply $z\n  call $name($x, $x, $z)\n  dsply $z\n  call QGPL/ADDER($z)\n  ret\n}\n",
        ActGrpAttr::Caller,
    );
    let mut src = MemSource::default();
    src.add("QGPL", callee);
    src.add("QGPL", caller);
    let mut job = Job::boxed(Box::new(src));
    job.call_program("CALLER", "*LIBL", &[]).unwrap();
    assert_eq!(job.console, vec!["3", "42", "3", "80"]);
    // The last call passed one parameter; reading the second is a null pointer.
    assert_eq!(job.clear().map(|e| e.msgid), Some("CEE9901".into()));
    assert!(job.raised.iter().any(|e| e.cond == Some(Cond::Mch3601)));
    // A native program in QSYS is found before any object.
    job.natives
        .insert("QNATIVE".into(), |j: &mut Job, args: &[u64]| {
            j.console.push(format!("native with {} args", args.len()));
        });
    job.call_program("QNATIVE", "QSYS", &[1, 2]).unwrap();
    assert_eq!(
        job.console.last().map(String::as_str),
        Some("native with 2 args")
    );
    let _ = ExportKind::Proc;
}
