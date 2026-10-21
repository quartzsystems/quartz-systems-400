// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The debug core: a program compiled with a debug view stops at
//! breakpoints and steps while under `STRDBG`, `EVAL` shows its
//! variables the way IBM's debugger does, and `ENDDBG` puts the
//! production code back.

#![allow(clippy::type_complexity)]

use std::cell::RefCell;
use std::rc::Rc;

use ile::{ActGrpAttr, BindInput, BindOptions, CreateData, ModuleObject, PgmKind};
use mi::debug::{Resume, Stop};
use mi::{Job, MemSource};

const SUB: &str = "
module DSUB dbgview(*STMT)
proc DSUB($v: i32) entry {
  auto $d: i32 inz(7)
L0:
  debug.var \"V\", $v
  debug.var \"D\", $d
  debug.stmt 1
  %a = load $v
  %b = add %a, 1
  store $v, %b
  debug.stmt 2
  ret
}
";

const MAIN: &str = "
module DMAIN dbgview(*STMT)
static $count: i32 inz(0)
static $name: char(8) inz(\"QUARTZ\")
static $amt: packed(7,2) inz(12.5)
static $flag: ind inz(1)
static $p: ptr
proc DMAIN() entry {
  auto $loc: char(4) inz(\"abcd\")
L0:
  debug.var \"COUNT\", $count
  debug.var \"NAME\", $name
  debug.var \"AMT\", $amt
  debug.var \"FLAG\", $flag
  debug.var \"P\", $p
  debug.var \"LOC\", $loc
  debug.stmt 10
  %c = load $count
  %c2 = add %c, 1
  store $count, %c2
  debug.stmt 11
  call DSUB($count)
  debug.stmt 12
  dsply $count
  debug.stmt 13
  ret
}
";

fn program(text: &str) -> ile::ProgramObject {
    let m = ModuleObject::compile(
        qir::parse(text).unwrap_or_else(|e| panic!("{e}")),
        CreateData::default(),
        0,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert!(
        m.debug_code.is_some(),
        "a debug view asks for the debug variant"
    );
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

fn job() -> Box<Job> {
    let mut src = MemSource::default();
    src.add("QGPL", program(SUB));
    src.add("QGPL", program(MAIN));
    Job::boxed(Box::new(src))
}

#[test]
fn without_strdbg_the_production_code_runs_and_never_stops() {
    let mut j = job();
    let seen = Rc::new(RefCell::new(0));
    let s = seen.clone();
    j.set_debug_hook(Box::new(move |_, _| {
        *s.borrow_mut() += 1;
        Resume::Go
    }));
    j.addbkp("DMAIN", 11);
    j.call_program("DMAIN", "*LIBL", &[]).unwrap();
    assert_eq!(j.console, vec!["2"]);
    assert_eq!(*seen.borrow(), 0, "no program is under debug");
}

#[test]
fn breakpoints_steps_and_eval() {
    let mut j = job();
    let log: Rc<RefCell<Vec<(Stop, Vec<String>)>>> = Rc::new(RefCell::new(Vec::new()));
    let l = log.clone();
    let script = Rc::new(RefCell::new(vec![
        Resume::StepOver,
        Resume::StepInto,
        Resume::StepOver,
        Resume::Go,
    ]));
    let sc = script.clone();
    j.set_debug_hook(Box::new(move |job: &mut Job, stop: &Stop| {
        let vals: Vec<String> = job
            .dspvar()
            .iter()
            .map(|(n, _)| format!("{n}={}", job.eval(n).unwrap_or_else(|e| e)))
            .collect();
        l.borrow_mut().push((stop.clone(), vals));
        let mut s = sc.borrow_mut();
        if s.is_empty() {
            Resume::Go
        } else {
            s.remove(0)
        }
    }));
    assert_eq!(j.strdbg("DMAIN", "*LIBL").unwrap(), "QGPL/DMAIN");
    j.strdbg("DSUB", "QGPL").unwrap();
    assert!(j.strdbg("NOSUCH", "QGPL").is_err());
    j.addbkp("DMAIN", 10);
    j.call_program("DMAIN", "*LIBL", &[]).unwrap();
    assert_eq!(j.console, vec!["2"], "DMAIN and DSUB each add one");
    let log = log.borrow();
    let stops: Vec<(String, u32, u32)> = log
        .iter()
        .map(|(s, _)| (s.program.clone(), s.stmt, s.depth))
        .collect();
    assert_eq!(
        stops,
        vec![
            ("QGPL/DMAIN".into(), 10, 1),
            ("QGPL/DMAIN".into(), 11, 1),
            ("QGPL/DSUB".into(), 1, 2),
            ("QGPL/DSUB".into(), 2, 2),
        ],
        "breakpoint, step over, step into the callee, step over inside it"
    );
    let at10 = &log[0].1;
    assert!(at10.contains(&"COUNT=0".to_string()), "{at10:?}");
    assert!(at10.contains(&"NAME='QUARTZ  '".to_string()), "{at10:?}");
    assert!(at10.contains(&"AMT=12.50".to_string()), "{at10:?}");
    assert!(at10.contains(&"FLAG='1'".to_string()), "{at10:?}");
    assert!(at10.contains(&"P=SPP:*NULL".to_string()), "{at10:?}");
    assert!(at10.contains(&"LOC='abcd'".to_string()), "{at10:?}");
    let at11 = &log[1].1;
    assert!(at11.contains(&"COUNT=1".to_string()), "{at11:?}");
    let in_sub = &log[2].1;
    assert!(
        in_sub.contains(&"V=1".to_string()) && in_sub.contains(&"D=7".to_string()),
        "{in_sub:?}"
    );
    assert_eq!(log[2].0.procedure, "DSUB");
    assert_eq!(j.debug.as_ref().unwrap().stops.len(), 4);
    drop(log);

    // Hexadecimal and character views of a variable at a later stop.
    let hexes: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let h = hexes.clone();
    j.set_debug_hook(Box::new(move |job: &mut Job, _| {
        h.borrow_mut().push(job.eval("NAME:x").unwrap());
        h.borrow_mut().push(job.eval("name:c").unwrap());
        h.borrow_mut().push(job.eval("NOPE").unwrap_err());
        Resume::Go
    }));
    j.call_program("DMAIN", "*LIBL", &[]).unwrap();
    let hexes = hexes.borrow();
    assert_eq!(
        hexes[0],
        "0000  D8E4C1D9 E3E94040                     *QUARTZ  *"
    );
    assert_eq!(hexes[1], "'QUARTZ  '");
    assert!(hexes[2].contains("NOPE"));
    drop(hexes);

    // ENDDBG: the production code runs again, with no stops.
    j.enddbg();
    assert_eq!(
        j.rmvbkp("DMAIN", None),
        0,
        "the breakpoints went with the debugger"
    );
    j.call_program("DMAIN", "*LIBL", &[]).unwrap();
    // The debug variant kept its statics across the two debug runs;
    // the production copy loaded again starts with fresh ones.
    assert_eq!(j.console, vec!["2", "4", "2"]);
    assert!(j.debug.is_none());
}

#[test]
fn an_active_program_cannot_enter_debug() {
    let mut j = job();
    j.set_debug_hook(Box::new(|job: &mut Job, _| {
        assert!(job.strdbg("DMAIN", "QGPL").is_err(), "it is on the stack");
        Resume::Go
    }));
    j.strdbg("DMAIN", "QGPL").unwrap();
    j.addbkp("QGPL/DMAIN", 12);
    j.call_program("DMAIN", "*LIBL", &[]).unwrap();
    assert_eq!(j.debug.as_ref().unwrap().stops.len(), 1);
    assert_eq!(j.rmvbkp("QGPL/DMAIN", Some(12)), 1);
}
