// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The exception model end to end: the function check, the inquiry
//! and its replies, the dump, `CEE9901` to a cancelled program's
//! caller, and the condition-management APIs.

#![allow(clippy::similar_names)]

use ile::{ActGrpAttr, BindInput, BindOptions, CreateData, ModuleObject, PgmKind, ProgramObject};
use mi::exec::Program;
use mi::{Cond, Job, MemSource};

fn program(text: &str) -> ProgramObject {
    let m = ModuleObject::compile(
        qir::parse(text).unwrap_or_else(|e| panic!("{e}")),
        CreateData::default(),
        0,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let mut opts = BindOptions::new(&m.name.clone(), PgmKind::Pgm);
    opts.attr.actgrp = ActGrpAttr::Caller;
    ile::bind(BindInput {
        modules: vec![(m, "QGPL".into())],
        bnddirs: Vec::new(),
        lookup: &|_| None,
        options: opts,
    })
    .unwrap_or_else(|e| panic!("{e}"))
    .0
}

fn ids(j: &Job) -> Vec<&str> {
    j.raised.iter().map(|e| e.msgid.as_str()).collect()
}

/// A program whose callee divides by zero, with its own statements.
const FAILS: &str = "
module FAILS
static $z: packed(3,0)
proc SUB() {
L0:
  debug.stmt 7
  dec.div $z, 1, 0 prec(3,0)
  debug.stmt 8
  dsply \"not reached\"
  ret
}
proc FAILS() entry {
L0:
  debug.stmt 3
  callb SUB()
  debug.stmt 4
  dsply \"after\"
  ret
}
";

#[test]
fn an_unhandled_condition_becomes_a_function_check_and_an_inquiry() {
    let mut p = Program::from_text(FAILS).unwrap();
    p.call_entry().unwrap();
    let j = p.job();
    assert_eq!(ids(j), vec!["MCH1211", "CPF9999", "CEE9901"]);
    assert_eq!(j.raised[0].depth, 2, "sent to SUB");
    assert_eq!(j.raised[1].depth, 2, "the function check goes to SUB too");
    assert_eq!(j.raised[2].depth, 0, "the cancel leaves the job with it");
    assert_eq!(j.inquiry_log, vec!["C"]);
    let inq = &j.ext_msgq.msgs[0];
    assert_eq!(inq.msgid, "CEE9901");
    assert_eq!(inq.msg_type, qir::MsgType::Inq);
    assert_eq!(inq.reply.as_deref(), Some("C"));
    assert!(
        inq.text.contains("FAILS") && inq.text.contains("MCH1211"),
        "{}",
        inq.text
    );
    assert!(j.console.is_empty());
    assert!(j.pending());
    assert_eq!(j.depth, 0);
    assert!(j.last_dump.is_none());
}

#[test]
fn a_pssr_monitor_takes_the_function_check() {
    let mut p = Program::from_text(
        "
module PSSR
static $z: packed(3,0)
proc SUB() {
L0:
  dec.div $z, 1, 0 prec(3,0)
  ret
}
proc PSSR() entry {
L0:
  monitor M, CPF9999 -> HANDLE
  callb SUB()
  endmon M
  ret
HANDLE:
  dsply \"function check\"
  ret
}
",
    )
    .unwrap();
    p.call_entry().unwrap();
    let j = p.job();
    assert_eq!(j.console, vec!["function check"]);
    assert_eq!(ids(j), vec!["MCH1211", "CPF9999"]);
    assert_eq!(j.handled.len(), 1);
    assert_eq!(j.handled[0].msgid, "CPF9999");
    assert_eq!(
        ccsid::decode_trimmed(&j.handled[0].data[..7]),
        "MCH1211",
        "the function check names the condition"
    );
    assert!(j.inquiry_log.is_empty());
    assert!(!j.pending());
}

#[test]
fn reply_g_resumes_and_i_ignores() {
    let mut p = Program::from_text(FAILS).unwrap();
    p.job().inquiry_replies.push_back("x".into());
    p.job().inquiry_replies.push_back("g".into());
    p.call_entry().unwrap();
    let j = p.job();
    assert_eq!(j.console, vec!["not reached", "after"]);
    assert_eq!(j.inquiry_log, vec!["G"], "an invalid reply is skipped");
    assert!(!j.pending());
    assert_eq!(j.handled.len(), 1);
    assert_eq!(j.handled[0].msgid, "CPF9999");
    assert!(j.inquiry_replies.is_empty());
}

#[test]
fn replies_d_and_f_dump() {
    let mut p = Program::from_text(FAILS).unwrap();
    p.job().inquiry_replies.push_back("D".into());
    p.call_entry().unwrap();
    let j = p.job();
    assert!(j.pending());
    let d = j.last_dump.clone().expect("a dump");
    assert!(d.contains("*FULL"), "{d}");
    assert!(d.contains("MCH1211") && d.contains("CPF9999"), "{d}");
    assert!(
        d.contains("FAILS      SUB        7"),
        "statement of the failing entry:\n{d}"
    );
    assert!(
        d.contains("FAILS      FAILS      3"),
        "statement of the caller:\n{d}"
    );
    assert!(d.contains("*CTLBDY"), "{d}");
    assert!(
        d.contains("Static storage of QTEMP/FAILS") && d.contains("z          packed(3,0)"),
        "{d}"
    );

    let mut p = Program::from_text(FAILS).unwrap();
    p.job().inquiry_replies.push_back("F".into());
    p.call_entry().unwrap();
    let d = p.job().last_dump.clone().expect("a dump");
    assert!(
        d.contains("*CALLSTK") && !d.contains("Static storage"),
        "{d}"
    );
}

#[test]
fn a_cancelled_program_sends_cee9901_to_its_caller() {
    let mut src = MemSource::default();
    src.add("QGPL", program(FAILS));
    src.add(
        "QGPL",
        program(
            "
module CALLER
proc CALLER() entry {
L0:
  monitor M, CEE9901 -> GOT
  call FAILS()
  dsply \"not reached\"
  endmon M
  ret
GOT:
  dsply \"caller got it\"
  ret
}
",
        ),
    );
    let mut job = Job::boxed(Box::new(src));
    job.call_program("CALLER", "*LIBL", &[]).unwrap();
    assert_eq!(job.console, vec!["caller got it"]);
    assert_eq!(ids(&job), vec!["MCH1211", "CPF9999", "CEE9901"]);
    assert_eq!(job.raised[2].depth, 1, "sent to CALLER");
    assert_eq!(job.handled.len(), 1);
    assert_eq!(job.handled[0].msgid, "CEE9901");
    assert_eq!(job.inquiry_log, vec!["C"]);
    assert!(!job.pending());
    assert_eq!(job.depth, 0);
}

#[test]
fn a_caller_without_a_monitor_gets_its_own_inquiry() {
    let mut src = MemSource::default();
    src.add("QGPL", program(FAILS));
    src.add(
        "QGPL",
        program("module OUTER\nproc OUTER() entry {\nL0:\n  call FAILS()\n  dsply \"not reached\"\n  ret\n}\n"),
    );
    let mut job = Job::boxed(Box::new(src));
    job.inquiry_replies.push_back("C".into());
    job.inquiry_replies.push_back("G".into());
    job.call_program("OUTER", "*LIBL", &[]).unwrap();
    assert_eq!(job.inquiry_log, vec!["C", "G"]);
    assert_eq!(
        ids(&job),
        vec!["MCH1211", "CPF9999", "CEE9901", "CPF9999"],
        "the caller's function check is for CEE9901"
    );
    assert_eq!(
        job.console,
        vec!["not reached"],
        "G resumes OUTER after the call"
    );
    assert!(!job.pending());
}

#[test]
fn condition_handlers_resume_or_percolate() {
    let mut p = Program::from_text(
        "
module HDL
extern proc CEEHDLR($p: procptr, $t: i32, $fc: char(12))
extern proc CEEHDLU($p: procptr, $fc: char(12))
static $pp: procptr
static $count: i32
static $fc: char(12)
static $seen: char(12)
static $z: packed(3,0)
proc H($tok: char(12), $utok: i32, $rc: i32, $new: char(12)) {
L0:
  mov $seen, $tok
  %c = load $utok
  %c2 = add %c, 1
  store $utok, %c2
  %ten = const.i32 10
  %twenty = const.i32 20
  %first = cmp.eq %c2, 1
  %r = select %first, %ten, %twenty
  store $rc, %r
  ret
}
proc HDL() entry {
L0:
  %p = procaddr H
  store $pp, %p
  callb CEEHDLR($pp, $count, $fc)
  dec.div $z, 1, 0 prec(3,0)
  dsply \"resumed\"
  monitor M, MCH1211 -> MON
  dec.div $z, 2, 0 prec(3,0)
  endmon M
  ret
MON:
  dsply \"monitor after percolate\"
  callb CEEHDLU($pp, $fc)
  callb CEEHDLU($pp, $fc)
  ret
}
",
    )
    .unwrap();
    p.call_entry().unwrap();
    let count = i32::from_be_bytes(p.static_bytes("count").unwrap().try_into().unwrap());
    assert_eq!(count, 2, "the handler ran for both conditions");
    let seen = p.static_bytes("seen").unwrap();
    assert_eq!(&seen[2..4], &[0x12, 0x11], "message number in the token");
    assert_eq!(ccsid::decode_trimmed(&seen[5..8]), "MCH");
    assert_eq!(seen[0..2], [0, 3], "severity 40 is class 3");
    let fc = p.static_bytes("fc").unwrap();
    assert_eq!(
        &fc[2..4],
        &[0x08, 0x09],
        "the second CEEHDLU finds nothing: CEE0809"
    );
    let j = p.job();
    assert_eq!(j.console, vec!["resumed", "monitor after percolate"]);
    assert_eq!(ids(j), vec!["MCH1211", "MCH1211"]);
    assert_eq!(j.handled.len(), 2);
    assert!(!j.pending());
}

#[test]
fn cancel_handlers_run_when_an_entry_is_cancelled() {
    let mut p = Program::from_text(
        "
module CAN
extern proc CEERTX($p: procptr, $t: i32, $fc: char(12) *NOPASS)
static $pp: procptr
static $flag: i32
static $z: packed(3,0)
proc CANCELLED($utok: i32) {
L0:
  store $utok, 99
  dsply \"cancel handler\"
  ret
}
proc SUB() {
L0:
  %p = procaddr CANCELLED
  store $pp, %p
  callb CEERTX($pp, $flag)
  dec.div $z, 1, 0 prec(3,0)
  ret
}
proc OK() {
L0:
  %p = procaddr CANCELLED
  store $pp, %p
  callb CEERTX($pp, $flag)
  ret
}
proc CAN() entry {
L0:
  callb OK()
  monitor M, *ALL -> MON
  callb SUB()
  endmon M
  ret
MON:
  dsply \"caught\"
  ret
}
",
    )
    .unwrap();
    p.call_entry().unwrap();
    let flag = i32::from_be_bytes(p.static_bytes("flag").unwrap().try_into().unwrap());
    assert_eq!(flag, 99);
    let j = p.job();
    assert_eq!(
        j.console,
        vec!["cancel handler", "caught"],
        "a normal return runs no cancel handler"
    );
    assert_eq!(j.handled[0].cond, Some(Cond::Mch1211));
    assert!(!j.pending());
}

#[test]
fn tokens_encode_decode_and_signal() {
    let mut p = Program::from_text(
        "
module SIG
extern proc CEENCOD($c1: i16, $c2: i16, $case: i16, $sev: i16, $ctl: i16, $fac: char(3), $isi: i32, $tok: char(12), $fc: char(12))
extern proc CEEDCOD($tok: char(12), $c1: i16, $c2: i16, $case: i16, $sev: i16, $ctl: i16, $fac: char(3), $isi: i32, $fc: char(12))
extern proc CEESGL($tok: char(12), $q: i32, $fc: char(12))
static $c1: i16 inz(3)
static $c2: i16 inz(x\"1211\")
static $case: i16 inz(1)
static $sev: i16 inz(3)
static $ctl: i16
static $fac: char(3) inz(\"MCH\")
static $isi: i32
static $tok: char(12)
static $fc: char(12)
static $o1: i16
static $o2: i16
static $ocase: i16
static $osev: i16
static $octl: i16
static $ofac: char(3)
static $oisi: i32
proc SIG() entry {
L0:
  callb CEENCOD($c1, $c2, $case, $sev, $ctl, $fac, $isi, $tok, $fc)
  callb CEEDCOD($tok, $o1, $o2, $ocase, $osev, $octl, $ofac, $oisi, $fc)
  monitor M, 102 -> GOT
  callb CEESGL($tok, $isi, $fc)
  dsply \"not reached\"
  endmon M
  ret
GOT:
  dsply \"signaled\"
  ret
}
",
    )
    .unwrap();
    p.call_entry().unwrap();
    assert_eq!(p.static_bytes("o2").unwrap(), vec![0x12, 0x11]);
    assert_eq!(p.static_bytes("osev").unwrap(), vec![0, 3]);
    assert_eq!(p.static_bytes("ocase").unwrap(), vec![0, 1]);
    assert_eq!(
        ccsid::decode_trimmed(&p.static_bytes("ofac").unwrap()),
        "MCH"
    );
    let j = p.job();
    assert_eq!(j.console, vec!["signaled"]);
    assert_eq!(j.handled.len(), 1);
    assert_eq!(
        j.handled[0].cond,
        Some(Cond::Mch1211),
        "a signaled MCH1211 is the condition"
    );
    assert!(!j.pending());
}
