// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The text form round-trips and the verifier accepts what it should.

use qir::{Inst, QirError, Type, parse, print, verify};

const SAMPLE: &str = r#"
; A module exercising every declaration and the core instructions.
module FACT lang(QIR) ccsid(37) actgrp(*CALLER) dbgview(*STMT)

type Rec = struct { name: char(10), amt: packed(9,2), flags: array[3] of ind }
type Big = struct(48) {
  r: Rec @0,
  p: ptr @32
}

static $count: i32 inz(0) export
static $rec: Rec inz(*BLANKS)
const $greet: char(5) = "HELLO"
static $pp: ptr inz(*NULL)
based $r2: Rec on $pp
import $other: packed(7,2)

file CUST file(CUSTOMER) lib(QSTEST) usage(*UPDATE) keyed fmt(CUSTREC:"0123456789ABC")

extern proc CEETREC(byval $rc: i32 *NOPASS)
extern proc OPTP($a: i32 *OMIT)
extern pgm QCMDEXC($cmd: char(1) *VARSIZE, $len: packed(15,5)) pgm(QCMDEXC) lib(QSYS)
extern sysapi QUSROBJD($rcv: char(1) *VARSIZE, byval $len: i32)
proto FP(byval $n: i32) -> i32

proc FACT(byval $n: i32) -> i32 export {
L0:
  %n = load $n
  %z = cmp.le %n, 1
  brif %z, L1, L2
L1:
  ret 1
L2:
  %m = sub %n, 1
  %f = callb FACT(%m)
  %r = mul.chk %n, %f
  ret %r
}

proc MAIN($parm: char(10), $opt: i32 *NOPASS *OMIT) entry {
  auto $i: i32 inz(0)
  auto $s: char(20)
  static $hits: packed(5,0) inz(1)
  auto $big: Big
L0:
  debug.stmt 1
  %i = load $i
  %i2 = add %i, 1
  store $i, %i2
  %f = callb FACT(5)
  store $count, %f
  mov $rec.name, $parm
  mov $s, "text"
  %p = cmp.eq %f, 120
  brif %p, OK, BAD
OK:
  call QCMDEXC($s, 20)
  call QGPL/MYPGM($s)
  call $parm($s)
  callb CEETREC()
  callb OPTP(*OMIT)
  sysapi QUSROBJD($s, 20)
  debug.var "COUNT", $count
  %k = load $rec.flags[1]
  %ix = const.i32 2
  store $rec.flags[%ix], %k
  %q = load $pp
  %v = load [%q +16]:i32
  %w = load [%q]:Big.r.flags[0]
  %n = parms
  %have = passed $opt
  %omit = omitted $opt
  %c = cvt.i64.chk %n
  %d = select %have, %c, 0
  %e = neg %d
  %sh = shl %e, 2
  %ff = const.f64 1.5e0
  %gg = div %ff, 2.0
  mov $big.r, $rec
  %fp = load $big.p
  switch %i2, [1: OK2, 2: BAD], default OK2
OK2:
  debug.scope begin
  debug.scope end
  ret
BAD:
  unreachable
}
"#;

#[test]
fn sample_parses_verifies_and_round_trips() {
    let m = parse(SAMPLE).unwrap_or_else(|e| panic!("{e}"));
    verify(&m).unwrap_or_else(|e| panic!("{e}"));
    let text = print(&m);
    let again = parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(m, again, "{text}");
    assert_eq!(print(&again), text);
}

#[test]
fn registers_take_their_types_from_definitions() {
    let m = parse(SAMPLE).unwrap();
    let (_, main) = m.proc_named("MAIN").unwrap();
    let ty = |n: &str| main.reg_type(main.reg_named(n).unwrap()).unwrap().clone();
    assert_eq!(ty("i2"), Type::I32);
    assert_eq!(ty("p"), Type::Ind);
    assert_eq!(ty("k"), Type::Ind);
    assert_eq!(ty("c"), Type::I64);
    assert_eq!(ty("gg"), Type::F64);
    assert_eq!(ty("fp"), Type::Ptr);
}

#[test]
fn initializers_are_encoded() {
    let m = parse(SAMPLE).unwrap();
    let count = m.vars.iter().find(|v| v.name == "count").unwrap();
    assert_eq!(count.inz.as_deref(), Some(&[0, 0, 0, 0][..]));
    let greet = m.vars.iter().find(|v| v.name == "greet").unwrap();
    assert_eq!(
        greet.inz.as_deref(),
        Some(&[0xC8, 0xC5, 0xD3, 0xD3, 0xD6][..])
    );
    let rec = m.vars.iter().find(|v| v.name == "rec").unwrap();
    let b = rec.inz.as_ref().unwrap();
    assert_eq!(b.len(), 18);
    assert_eq!(&b[..10], &[0x40; 10]);
    assert_eq!(&b[10..15], &[0, 0, 0, 0, 0x0C]);
}

#[test]
fn labels_resolve_forward() {
    let m = parse(SAMPLE).unwrap();
    let (_, fact) = m.proc_named("FACT").unwrap();
    match &fact.blocks[0].insts[2] {
        Inst::BrIf { t, f, .. } => {
            assert_eq!(fact.label_name(*t), "L1");
            assert_eq!(fact.label_name(*f), "L2");
        }
        other => panic!("{other:?}"),
    }
}

fn rejects(text: &str, needle: &str) {
    let full = format!("module T\n{text}");
    let err = match parse(&full) {
        Err(e) => e,
        Ok(m) => match verify(&m) {
            Err(e) => e,
            Ok(()) => panic!("accepted:\n{text}"),
        },
    };
    let msg = err.to_string();
    assert!(msg.contains(needle), "wanted {needle:?} in {msg:?}");
    let _ = QirError::Binary { msg: String::new() };
}

#[test]
fn verifier_rejects() {
    rejects(
        "proc P() {\nL0:\n  brif 1, A, B\nA:\n  %x = const.i32 1\n  br C\nB:\n  br C\nC:\n  %y = add %x, 1\n  ret\n}",
        "may be read before it is assigned",
    );
    rejects(
        "proc P() {\nL0:\n  %x = const.i32 1\n  %x = const.i64 2\n  ret\n}",
        "is a i32 and is assigned a i64",
    );
    rejects(
        "proc P() {\nL0:\n  %x = const.i32 1\n}",
        "does not end in a terminator",
    );
    rejects(
        "proc P() {\nL0:\n  br NOWHERE\n}",
        "label NOWHERE is not defined",
    );
    rejects("type T = struct { a: char(3), p: ptr @3 }\n", "aligned");
    rejects(
        "proto F($a: i32 *NOPASS, $b: i32)\n",
        "follows a *NOPASS parameter",
    );
    rejects(
        "proc P() {\n  auto $a: char(3)\nL0:\n  %x = load $a\n  ret\n}",
        "only scalars load",
    );
    rejects(
        "proc P() {\n  auto $a: i32\nL0:\n  %x = const.i64 1\n  store $a, %x\n  ret\n}",
        "a i64 where a i32",
    );
    rejects(
        "proc P() {\n  auto $a: ptr\n  auto $b: i32\nL0:\n  %x = load $a\n  %y = load $b\n  %z = cmp.lt %x, %x\n  ret\n}",
        "equality only",
    );
    rejects("proc P() -> i32 {\nL0:\n  ret\n}", "returns a value");
    rejects(
        "proc P(byval $n: i32) entry {\nL0:\n  ret\n}",
        "entry parameters are by reference",
    );
    rejects(
        "proc A() {\nL0:\n  ret\n}\nproc A() {\nL0:\n  ret\n}",
        "declared twice",
    );
    rejects(
        "proc P() {\nL0:\n  callb Q(1)\n  ret\n}\nproc Q(byval $n: i32, $m: i32) {\nL0:\n  ret\n}",
        "parameter m is not passed",
    );
    rejects(
        "proc P() {\n  auto $a: i32\nL0:\n  callb Q(%u)\n  ret\n}\nproc Q(byval $n: i32) {\nL0:\n  ret\n}",
        "never assigned",
    );
    rejects(
        "static $x: i32 inz(x\"0102030405\")\n",
        "5 bytes do not fit i32",
    );
    rejects(
        "static $p: i32\nbased $r: i32 on $p\n",
        "which is not a ptr",
    );
    rejects(
        "proc P() {\nL0:\n  %x = const.i32 1\n  %y = cvt.i64 %x\n  %z = add %x, %y\n  ret\n}",
        "operands are a i32 and a i64",
    );
}
