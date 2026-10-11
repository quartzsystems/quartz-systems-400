// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Generated code runs: arithmetic, control flow, storage, calls,
//! parameters and the conditions the runtime is told about.

mod common;

use codegen::abi::Env;
use common::{Running, max_depth, raised};

type Fn1 = extern "sysv64" fn(*mut Env, i64, i64, i32) -> i32;
type Fn0 = extern "sysv64" fn(*mut Env, i64, i64) -> i32;
type FnV = extern "sysv64" fn(*mut Env, i64, i64);

/// Reinterpret a loaded procedure as a function pointer.
///
/// # Safety
///
/// The procedure's signature must be the one the type names.
unsafe fn f<T: Copy>(addr: usize) -> T {
    // SAFETY: the caller matches the generated signature.
    unsafe { std::mem::transmute_copy(&addr) }
}

#[test]
fn recursion_by_value_and_return() {
    let mut r = Running::new(
        "
module FACT
proc FACT(byval $n: i32) -> i32 export {
L0:
  %n = load $n
  %z = cmp.le %n, 1
  brif %z, ONE, REC
ONE:
  ret 1
REC:
  %m = sub %n, 1
  %f = callb FACT(%m)
  %r = mul.chk %n, %f
  ret %r
}
",
        &[],
    );
    let fact: Fn1 = unsafe { f(r.addr("FACT")) };
    assert_eq!(fact(r.env(), 1, 0, 5), 120);
    assert_eq!(fact(r.env(), 1, 0, 12), 479_001_600);
    assert_eq!(max_depth(), 12);
    assert_eq!(r.pending(), 0);
    // 13! overflows an i32: MCH1210 and percolation to the caller.
    assert_eq!(fact(r.env(), 1, 0, 13), 0);
    assert_eq!(r.pending(), 1210);
    assert_eq!(raised(), vec![1210]);
}

#[test]
fn statics_are_big_endian_and_persist() {
    let mut r = Running::new(
        "
module STAT
static $count: i32 inz(40)
static $total: i64
static $flag: ind
proc BUMP() -> i32 export {
L0:
  %c = load $count
  %c2 = add %c, 1
  store $count, %c2
  %t = load $total
  %c64 = cvt.i64 %c2
  %t2 = add %t, %c64
  store $total, %t2
  %big = cmp.gt %c2, 41
  store $flag, %big
  ret %c2
}
",
        &[],
    );
    let bump: Fn0 = unsafe { f(r.addr("BUMP")) };
    assert_eq!(bump(r.env(), 0, 0), 41);
    assert_eq!(bump(r.env(), 0, 0), 42);
    assert_eq!(r.static_bytes("count"), &[0, 0, 0, 42]);
    assert_eq!(r.static_bytes("total"), &[0, 0, 0, 0, 0, 0, 0, 83]);
    assert_eq!(r.static_bytes("flag"), &[0xF1]);
}

#[test]
fn by_reference_parameters_and_literals() {
    let mut r = Running::new(
        "
module REF
proc ADD($a: i32, $b: i32, $r: i32) export {
L0:
  %a = load $a
  %b = load $b
  %s = add %a, %b
  store $r, %s
  ret
}
proc MAIN() -> i32 export {
  auto $x: i32 inz(30)
  auto $y: i32
  auto $out: i32
L0:
  store $y, 12
  callb ADD($x, $y, $out)
  callb ADD($out, 100, $out)
  %o = load $out
  ret %o
}
",
        &[],
    );
    let main: Fn0 = unsafe { f(r.addr("MAIN")) };
    assert_eq!(main(r.env(), 0, 0), 142);
}

#[test]
fn conditions_raise_and_resume_with_zero() {
    let mut r = Running::new(
        r#"
module COND
proc DIV(byval $a: i32, byval $b: i32) -> i32 export {
L0:
  %a = load $a
  %b = load $b
  %q = div %a, %b
  ret %q
}
proc IDX(byval $i: i32) -> i32 export {
  auto $arr: array[3] of i32 inz(x"000000010000000200000003")
L0:
  %i = load $i
  %v = load $arr[%i]
  ret %v
}
proc NARROW(byval $v: i64) -> i8 export {
L0:
  %v = load $v
  %n = cvt.i8.chk %v
  ret %n
}
"#,
        &[],
    );
    let div: extern "sysv64" fn(*mut Env, i64, i64, i32, i32) -> i32 = unsafe { f(r.addr("DIV")) };
    assert_eq!(div(r.env(), 2, 0, 17, 5), 3);
    assert_eq!(div(r.env(), 2, 0, -17, 5), -3);
    assert_eq!(div(r.env(), 2, 0, 17, 0), 0);
    assert_eq!(r.pending(), 1211);
    r.clear_pending();
    assert_eq!(div(r.env(), 2, 0, i32::MIN, -1), 0);
    assert_eq!(r.pending(), 1210);
    r.clear_pending();

    let idx: Fn1 = unsafe { f(r.addr("IDX")) };
    assert_eq!(idx(r.env(), 1, 0, 0), 1);
    assert_eq!(idx(r.env(), 1, 0, 2), 3);
    assert_eq!(idx(r.env(), 1, 0, 3), 0);
    assert_eq!(r.pending(), 603);
    r.clear_pending();
    assert_eq!(idx(r.env(), 1, 0, -1), 0);
    assert_eq!(r.pending(), 603);
    r.clear_pending();

    let narrow: extern "sysv64" fn(*mut Env, i64, i64, i64) -> i8 = unsafe { f(r.addr("NARROW")) };
    assert_eq!(narrow(r.env(), 1, 0, -100), -100);
    assert_eq!(narrow(r.env(), 1, 0, 200), 0);
    assert_eq!(r.pending(), 1210);
    assert_eq!(raised(), vec![1211, 1210, 603, 603, 1210]);
}

#[test]
fn control_flow_switch_select_and_statements() {
    let mut r = Running::new(
        "
module FLOW
proc CLASSIFY(byval $n: i32) -> i32 export {
L0:
  debug.stmt 10
  %n = load $n
  switch %n, [0: ZERO, 1: ONE, -1: MINUS], default OTHER
ZERO:
  debug.stmt 20
  ret 100
ONE:
  ret 101
MINUS:
  ret 99
OTHER:
  debug.stmt 30
  %neg = cmp.lt %n, 0
  %big = const.i32 1000
  %r = select %neg, -1000, %big
  %i = const.i32 0
  %acc = const.i32 0
  br LOOP
LOOP:
  %done = cmp.ge %i, %n
  brif %done, END, BODY
BODY:
  %acc = add %acc, %i
  %i = add %i, 1
  br LOOP
END:
  %out = add %r, %acc
  ret %out
}
",
        &[],
    );
    let classify: Fn1 = unsafe { f(r.addr("CLASSIFY")) };
    assert_eq!(classify(r.env(), 1, 0, 0), 100);
    assert_eq!(classify(r.env(), 1, 0, 1), 101);
    assert_eq!(classify(r.env(), 1, 0, -1), 99);
    assert_eq!(classify(r.env(), 1, 0, -5), -1000);
    assert_eq!(classify(r.env(), 1, 0, 5), 1010);
    let p = r.code.proc_named("CLASSIFY").unwrap();
    let stmts: Vec<u32> = p.stmts.iter().map(|s| s.stmt).collect();
    assert!(
        stmts.contains(&10) && stmts.contains(&20) && stmts.contains(&30),
        "{stmts:?}"
    );
    assert_eq!(
        r.code.stmt_at(p.offset + p.stmts[0].start - p.offset),
        Some(p.stmts[0].stmt)
    );
}

#[test]
fn parameter_count_and_omission() {
    let mut r = Running::new(
        "
module PARMS
proc OPT($a: i32, $b: i32 *NOPASS *OMIT, $c: i32 *NOPASS) -> i32 export {
L0:
  %n = parms
  %pb = passed $b
  %ob = omitted $b
  %pc = passed $c
  %n10 = mul %n, 10
  %b1 = cvt.i32 %pb
  %o1 = cvt.i32 %ob
  %c1 = cvt.i32 %pc
  %o100 = mul %o1, 100
  %c1000 = mul %c1, 1000
  %s1 = add %n10, %b1
  %s2 = add %s1, %o100
  %s3 = add %s2, %c1000
  ret %s3
}
proc CALLS(byval $which: i32) -> i32 export {
  auto $x: i32
L0:
  %w = load $which
  switch %w, [1: ONE, 2: TWO, 3: THREE], default THREE
ONE:
  %r1 = callb OPT($x)
  ret %r1
TWO:
  %r2 = callb OPT($x, *OMIT)
  ret %r2
THREE:
  %r3 = callb OPT($x, $x, $x)
  ret %r3
}
",
        &[],
    );
    let calls: Fn1 = unsafe { f(r.addr("CALLS")) };
    assert_eq!(calls(r.env(), 1, 0, 1), 10);
    assert_eq!(calls(r.env(), 1, 0, 2), 120);
    assert_eq!(calls(r.env(), 1, 0, 3), 1031);
}

extern "sysv64" fn twice(_env: *mut Env, _argc: i64, _opdesc: i64, n: i32) -> i32 {
    n * 2
}

#[test]
fn imported_procedures_resolve_by_name() {
    let mut r = Running::new(
        "
module IMP
extern proc TWICE(byval $n: i32) -> i32
proc MAIN(byval $n: i32) -> i32 export {
L0:
  %n = load $n
  %t = callb TWICE(%n)
  %u = callb TWICE(%t)
  ret %u
}
",
        &[("TWICE", twice as *const () as usize)],
    );
    assert!(
        r.code
            .relocs
            .iter()
            .any(|x| x.target == codegen::RelocTarget::ImportProc("TWICE".into()))
    );
    let main: Fn1 = unsafe { f(r.addr("MAIN")) };
    assert_eq!(main(r.env(), 1, 0, 7), 28);
}

#[test]
fn text_moves_pad_with_blanks() {
    let mut r = Running::new(
        r#"
module TXT
static $long: char(8)
static $short: char(3) inz("ABC")
static $rec: struct { a: char(2), n: i16 }
proc MAIN() export {
L0:
  mov $long, $short
  mov $rec.a, "XY"
  store $rec.n, -2
  ret
}
"#,
        &[],
    );
    let main: FnV = unsafe { f(r.addr("MAIN")) };
    main(r.env(), 0, 0);
    assert_eq!(
        r.static_bytes("long"),
        &[0xC1, 0xC2, 0xC3, 0x40, 0x40, 0x40, 0x40, 0x40]
    );
    assert_eq!(r.static_bytes("rec"), &[0xE7, 0xE8, 0xFF, 0xFE]);
}

#[test]
fn floats_and_conversions() {
    let mut r = Running::new(
        "
module FLT
static $d: f64 inz(2.5)
proc MAIN(byval $n: i32) -> i32 export {
L0:
  %n = load $n
  %f = cvt.f64 %n
  %d = load $d
  %p = mul %f, %d
  %i = cvt.i32 %p
  %half = div %p, 2.0
  %hi = cvt.i32.chk %half
  %s = add %i, %hi
  ret %s
}
",
        &[],
    );
    let main: Fn1 = unsafe { f(r.addr("MAIN")) };
    assert_eq!(main(r.env(), 1, 0, 4), 10 + 5);
    assert_eq!(main(r.env(), 1, 0, 3), 7 + 3);
}

#[test]
fn unsupported_instructions_are_reported() {
    let m = qir::parse(
        "module U\nstatic $p: packed(5,2)\nproc P() {\nL0:\n  dec.add $p, $p, 1 prec(5,2)\n  ret\n}\n",
    )
    .unwrap();
    let err = codegen::compile(&m).unwrap_err().to_string();
    assert!(
        err.contains("DecBin") && err.contains("not supported"),
        "{err}"
    );
}
