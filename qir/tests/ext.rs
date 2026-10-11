// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The storage instructions round-trip and verify.

#![allow(clippy::too_many_lines)]

use qir::{Inst, Selector, Type, parse, print, verify};

const SAMPLE: &str = r#"
module STOR lang(RPGLE) ccsid(37) actgrp(QILE) dbgview(*ALL)

type Cust = struct { id: packed(7,0), name: char(30), bal: packed(11,2), since: date(*ISO) }
type Infds = struct { status: zoned(5,0) @10, rrn: i32 @397 }

static $cust: Cust
static $ds: Infds
static $key: packed(7,0)
static $msgkey: char(4)
static $txt: varchar(80)
static $u: ucs2(10)
static $ts: timestamp(6)
static $tm: time(*HMS)
static $n: i32
static $amt: packed(15,5)
static $big: packed(63,0)
static $pp: ptr
based $view: Cust on $pp
static $dq: char(100)
static $pgmname: char(10) inz("MYPGM")

file CUST file(CUSTOMER) lib(QSTEST) usage(*UPDATE) keyed fmt(CUSTREC)
file SCREEN file(MYDSPF) dev(*DSP)
file REPORT file(QSYSPRT) dev(*PRT)

extern proc HELPER($c: Cust)
extern proc CEEDOD(byval $n: i32)

proc MAIN() entry {
  auto $tmp: char(30)
  auto $p: packed(9,2)
  auto $z: zoned(5,0)
  auto $d: date(*ISO)
  auto $name10: char(10)
  auto $id7: char(7)
L0:
  dec.add $p, $cust.bal, 12.50 prec(12,2) halfadj
  dec.mul $amt, $p, $n prec(63,23) resdecpos
  dec.div $amt, $amt, 3 prec(63,30)
  dec.sub $z, $p, $p prec(12,2)
  dec.rem $z, $cust.id, 7 prec(7,0)
  dec.mov $cust.bal, $p halfadj
  dec.neg $p, $p
  dec.abs $amt, $cust.bal
  %i = const.i32 5
  dec.mov $z, %i
  %f = cvt.i64 $cust.id
  %neg = dec.cmp.lt $p, 0
  cvt $tmp, $p
  cvt $p, $tmp halfadj
  cvt $tmp, $cust.since datfmt(*USA)
  cvt $d, "2026-03-01"
  cvt $z, $d datfmt(*YMD)
  cvt $u, $tmp ccsid(37)
  cvt $ts, $d
  str.cat $tmp, $cust.name, " LTD"
  str.subst $name10, $cust.name, 1, 10
  %pos = str.scan "A", $cust.name
  %pos2 = str.scan "A", $cust.name, %pos
  %bad = str.check "0123456789", $name10
  %badr = str.checkr "0123456789", $name10, 10
  str.xlate $tmp, "abc", "ABC", $cust.name
  str.trim $txt, $cust.name
  str.triml $txt, $cust.name, " *"
  str.trimr $tmp, $tmp
  %same = str.cmp.eq $tmp, $cust.name
  %len = str.len $txt
  str.setlen $txt, 5
  str.replace $tmp, $cust.name, "X", 3
  str.replace $tmp, $cust.name, "XY", 3, 1
  dt.add $d, $cust.since, 30, *DAYS
  dt.add $ts, $ts, -1, *HOURS
  %days = dt.diff $d, $cust.since, *DAYS
  %yr = dt.extract $cust.since, *YEARS
  %ok = dt.test $tmp datfmt(*ISO)
  %ok2 = dt.test $d
  dt.now $tm
  %a = addr $cust
  %b = ptradd %a, 7
  %diff = ptrdiff %b, %a
  %fp = procaddr HELPER
  %fp2 = procaddr MAIN
  %sp = rslvsp *PGM, $pgmname, "QGPL"
  store $pp, %a
  mov $tmp, $view.name
  monitor M1, MCH1211 -> DIV0, 100..199 -> STAT, "MCH*" -> ANYMCH, *FILE -> FILEERR, *ALL -> ALL
  dec.div $amt, $amt, $n prec(63,20)
  monitor M2, *PROGRAM -> ALL
  %st = dbopen CUST infds($ds)
  %st2 = dbio CUST CHAIN rec($cust) key($key)
  %st3 = dbio CUST READ rec($cust) nolock
  %st4 = dbio CUST SETLL rrn(%i)
  %st5 = dbio CUST UPDATE rec($cust) fmt(CUSTREC)
  %st6 = dbio CUST UNLOCK
  %st7 = dbclose CUST
  %st8 = dspio SCREEN EXFMT fmt(PROMPT) rec($tmp)
  %st9 = prtio REPORT WRITE fmt(LINE)
  endmon M2
  endmon M1
  br DONE
DIV0:
  sndpgmmsg msgid("CPF9898") msgf("QCPFMSG", "QSYS") data($tmp) type(*ESCAPE) to(*PRV) key($msgkey)
  br DONE
STAT:
  sndpgmmsg msgid("CPF9897") msgf("QCPFMSG") type(*INFO) to(*EXT)
  %got = rcvmsg pgmq(*SAME) type(*ANY) key($msgkey) rmv msg($tmp) msgid($id7) data($txt) keyout($msgkey) sev($n)
  %got2 = rcvmsg pgmq(2) key(x"00000001")
  rmvmsg pgmq(*SAME) key($msgkey)
  rmvmsg pgmq(*CTLBDY) clear(*ALL)
  br DONE
ANYMCH:
  dsply $tmp reply($name10)
  dsply "done"
  br DONE
FILEERR:
  dtaara.lock "MYDTA", "*LIBL"
  dtaara.read "MYDTA", "*LIBL", $tmp
  dtaara.write $name10, "QGPL", $tmp
  dtaara.unlock "MYDTA", "*LIBL"
  br DONE
ALL:
  dtaq.snd "MYQ", "QGPL", $dq, 100
  %rl = dtaq.rcv "MYQ", "QGPL", $dq, 100, -1
  br DONE
DONE:
  ret
}
"#;

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
