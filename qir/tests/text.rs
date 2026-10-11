// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The text form round-trips and the verifier accepts what it should.

use qir::{Inst, QirError, Type, parse, print, verify};

const SAMPLE: &str = include_str!("programs/fact.qir");

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

#[test]
fn binary_round_trips() {
    let m = parse(SAMPLE).unwrap();
    let bytes = qir::binary::write(&m);
    let again = qir::binary::read(&bytes).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(m, again);
}
