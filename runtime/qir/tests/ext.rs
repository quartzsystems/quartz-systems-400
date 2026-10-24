// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The storage instructions round-trip and verify.

#![allow(clippy::too_many_lines)]

use qir::{Inst, Selector, Type, parse, print, verify};

const SAMPLE: &str = include_str!("programs/stor.qir");

#[test]
fn storage_sample_parses_verifies_and_round_trips() {
    let m = parse(SAMPLE).unwrap_or_else(|e| panic!("{e}"));
    verify(&m).unwrap_or_else(|e| panic!("{e}"));
    let text = print(&m);
    let again = parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(m, again, "{text}");
    assert_eq!(print(&again), text);
}

#[test]
fn result_types_of_storage_instructions() {
    let m = parse(SAMPLE).unwrap();
    let (_, main) = m.proc_named("MAIN").unwrap();
    let ty = |n: &str| main.reg_type(main.reg_named(n).unwrap()).unwrap().clone();
    assert_eq!(ty("neg"), Type::Ind);
    assert_eq!(ty("pos"), Type::I32);
    assert_eq!(ty("days"), Type::I64);
    assert_eq!(ty("diff"), Type::I64);
    assert_eq!(ty("a"), Type::Ptr);
    assert_eq!(ty("fp"), Type::ProcPtr);
    assert_eq!(ty("sp"), Type::Ptr);
    assert_eq!(ty("st2"), Type::I32);
    assert_eq!(ty("got"), Type::Ind);
    assert_eq!(ty("rl"), Type::I32);
    assert_eq!(ty("f"), Type::I64);
}

#[test]
fn monitor_selectors_parse() {
    let m = parse(SAMPLE).unwrap();
    let (_, main) = m.proc_named("MAIN").unwrap();
    let mon = main.blocks[0]
        .insts
        .iter()
        .find_map(|i| match i {
            Inst::Monitor { handlers, .. } => Some(handlers),
            _ => None,
        })
        .unwrap();
    let sels: Vec<&Selector> = mon.iter().map(|h| &h.selector).collect();
    assert_eq!(
        sels,
        vec![
            &Selector::MsgId("MCH1211".into()),
            &Selector::Status(100, 199),
            &Selector::MsgId("MCH*".into()),
            &Selector::File,
            &Selector::All
        ]
    );
}

fn rejects(text: &str, needle: &str) {
    let full = format!(
        "module T\nstatic $p: packed(5,2)\nstatic $c: char(10)\nstatic $d: date(*ISO)\nstatic $v: varchar(5)\nstatic $pt: ptr\nfile F file(F) keyed\nfile S file(S) dev(*DSP)\n{text}"
    );
    let err = match parse(&full) {
        Err(e) => e,
        Ok(m) => match verify(&m) {
            Err(e) => e,
            Ok(()) => panic!("accepted:\n{text}"),
        },
    };
    let msg = err.to_string();
    assert!(msg.contains(needle), "wanted {needle:?} in {msg:?}");
}

#[test]
fn verifier_rejects_storage_mistakes() {
    rejects(
        "proc P() {\nL0:\n  dec.add $p, $p, 1\n  ret\n}",
        "expected prec",
    );
    rejects(
        "proc P() {\nL0:\n  dec.add $p, $p, 1 prec(70,0)\n  ret\n}",
        "digits",
    );
    rejects(
        "proc P() {\nL0:\n  dec.add $c, $p, 1 prec(5,0)\n  ret\n}",
        "not numeric",
    );
    rejects("proc P() {\nL0:\n  dec.mov $p, $c\n  ret\n}", "not numeric");
    rejects(
        "proc P() {\nL0:\n  %x = dec.cmp.eq 1, 2\n  ret\n}",
        "two literals",
    );
    rejects(
        "proc P() {\nL0:\n  cvt $pt, $c\n  ret\n}",
        "no conversion from a char(10)",
    );
    rejects("proc P() {\nL0:\n  cvt $c, $c\n  ret\n}", "no conversion");
    rejects(
        "proc P() {\nL0:\n  str.cat $p, $c, $c\n  ret\n}",
        "not a string",
    );
    rejects(
        "proc P() {\nL0:\n  str.setlen $c, 3\n  ret\n}",
        "not varying",
    );
    rejects(
        "proc P() {\nL0:\n  dt.add $d, $d, 1, *HOURS\n  ret\n}",
        "does not apply",
    );
    rejects(
        "proc P() {\nL0:\n  %x = load $pt\n  %y = ptradd %x, $c\n  ret\n}",
        "where an integer",
    );
    rejects(
        "proc P() {\nL0:\n  monitor M, *ALL -> H\n  ret\nH:\n  ret\n}",
        "not closed",
    );
    rejects("proc P() {\nL0:\n  endmon M\n  ret\n}", "has no monitor");
    rejects(
        "proc P() {\nL0:\n  monitor A, *ALL -> H\n  monitor B, *ALL -> H\n  endmon A\n  endmon B\n  ret\nH:\n  ret\n}",
        "closes monitor B",
    );
    rejects(
        "proc P() {\nL0:\n  monitor M\n  endmon M\n  ret\n}",
        "no handlers",
    );
    rejects(
        "proc P() {\nL0:\n  br IN\nL1:\n  monitor M, *ALL -> H\n  br IN\nIN:\n  %x = const.i32 1\n  endmon M\n  ret\nH:\n  ret\n}",
        "branches into monitor",
    );
    rejects(
        "proc P() {\nL0:\n  monitor M, *ALL -> H\n  br H\nH:\n  %x = const.i32 1\n  endmon M\n  ret\n}",
        "lies inside",
    );
    rejects(
        "proc P() {\nL0:\n  monitor M, *ALL -> H\n  %x = const.i32 1\n  endmon M\n  br H\nH:\n  %y = add %x, 1\n  ret\n}",
        "may be read before it is assigned",
    );
    rejects(
        "proc P() {\nL0:\n  %s = dbio F READ\n  ret\n}",
        "needs rec()",
    );
    rejects(
        "proc P() {\nL0:\n  %s = dbio F CHAIN rec($c)\n  ret\n}",
        "needs key() or rrn()",
    );
    rejects(
        "proc P() {\nL0:\n  %s = dbio S READ rec($c)\n  ret\n}",
        "not a database file",
    );
    rejects(
        "proc P() {\nL0:\n  %s = dspio F READ fmt(X)\n  ret\n}",
        "not a dspio file",
    );
    rejects(
        "proc P() {\nL0:\n  sndpgmmsg msgid(\"CPF98\") msgf(\"QCPFMSG\") type(*ESCAPE)\n  ret\n}",
        "char(7)",
    );
    rejects(
        "proc P() {\nL0:\n  sndpgmmsg msgid(\"CPF9898\") msgf(\"QCPFMSG\") type(*ESCAPE) key($c)\n  ret\n}",
        "char(4)",
    );
    rejects(
        "proc P() {\nL0:\n  %g = rcvmsg type(*ANY)\n  ret\n}",
        "needs pgmq()",
    );
    rejects(
        "proc P() {\nL0:\n  dtaara.lock \"X\", \"Y\", $c\n  ret\n}",
        "takes no place",
    );
    rejects(
        "proc P() {\nL0:\n  dtaara.read \"X\", \"Y\"\n  ret\n}",
        "needs a place",
    );
    rejects(
        "proc P() {\nL0:\n  dtaq.snd \"X\", \"ALIBRARYNAMETOOLONG\", $c, 10\n  ret\n}",
        "1 to 10 characters",
    );
    rejects(
        "proc P() {\nL0:\n  %x = procaddr NOPE\n  ret\n}",
        "not a procedure",
    );
}

#[test]
fn binary_round_trips_storage_instructions() {
    let m = parse(SAMPLE).unwrap();
    let bytes = qir::binary::write(&m);
    let again = qir::binary::read(&bytes).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(m, again);
    // Every byte matters: flipping any tag or count must fail or change the module.
    for i in 8..bytes.len().min(400) {
        let mut b = bytes.clone();
        b[i] ^= 0x40;
        if let Ok(other) = qir::binary::read(&b) {
            assert_ne!(other, m, "byte {i} did not matter");
        }
    }
}

/// Long varying types, strided arrays, file handles and null maps.
const PHASE9: &str = r#"module P9 lang(RPGLE) ccsid(37) actgrp(QILE) dbgview(*NONE)
type Rec = struct(30) { arr: array[3] of char(10) @0, code: array[3:10] of char(3) @0 }
static $v: varchar(70000:4)
static $u: varucs2(5:4)
static $g: vargraphic(6)
static $r: Rec
static $nm: array[2] of ind
static $buf: char(20)
file F file(CUSTF) lib(*LIBL) usage(*UPDATE) keyed
proc MAIN() entry {
B0:
  mov $v, "abc"
  %n = str.len $v
  mov $r.code[1], "xyz"
  %h = fileref F
  filebind F, %h
  %s = dbio F READ rec($buf) nulls($nm)
  ret
}
"#;

#[test]
fn phase9_types_and_file_handles_round_trip() {
    let m = parse(PHASE9).unwrap_or_else(|e| panic!("{e}"));
    verify(&m).unwrap_or_else(|e| panic!("{e}"));
    let text = print(&m);
    assert!(text.contains("varchar(70000:4)") && text.contains("array[3:10] of char(3)"), "{text}");
    assert!(text.contains("filebind F, %h") && text.contains("nulls($nm)"), "{text}");
    assert_eq!(parse(&text).unwrap(), m);
    assert_eq!(qir::binary::read(&qir::binary::write(&m)).unwrap(), m);
    let (_, main) = m.proc_named("MAIN").unwrap();
    assert_eq!(main.reg_type(main.reg_named("h").unwrap()), Some(&Type::I32));
    assert!(parse(&PHASE9.replace("varchar(70000:4)", "varchar(70000)")).is_err());
}
