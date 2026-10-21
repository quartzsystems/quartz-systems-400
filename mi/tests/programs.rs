// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IR programs run through the real runtime library: decimal
//! arithmetic, text, conversions, dates, pointers and monitors.

#![allow(clippy::many_single_char_names)]

use mi::{Cond, Program};

fn run(text: &str) -> Program {
    let mut p = Program::from_text(text, &[]).unwrap_or_else(|e| panic!("{e}"));
    p.call_entry().unwrap_or_else(|e| panic!("{e}"));
    p
}

fn statics(p: &Program, name: &str) -> Vec<u8> {
    p.static_bytes(name)
        .unwrap_or_else(|| panic!("no {name}"))
        .to_vec()
}

fn text(p: &Program, name: &str) -> String {
    ccsid::decode_trimmed(&statics(p, name))
}

fn num(p: &Program, name: &str) -> String {
    let m = p.module();
    let v = m.vars.iter().find(|v| v.name == name).expect("var");
    let b = statics(p, name);
    match &v.ty {
        qir::Type::Packed(s) => qdecimal::Packed::new(s.digits, s.scale)
            .unwrap()
            .decode(&b)
            .unwrap()
            .to_string(),
        qir::Type::Zoned(s) => qdecimal::Zoned::new(s.digits, s.scale)
            .unwrap()
            .decode(&b)
            .unwrap()
            .to_string(),
        qir::Type::I32 => i32::from_be_bytes(b[..4].try_into().unwrap()).to_string(),
        qir::Type::I64 => i64::from_be_bytes(b[..8].try_into().unwrap()).to_string(),
        qir::Type::F64 => f64::from_be_bytes(b[..8].try_into().unwrap()).to_string(),
        other => panic!("{other} is not numeric"),
    }
}

#[test]
fn decimal_arithmetic_follows_the_rules() {
    let p = run("
module DEC
static $a: packed(5,2) inz(123.45)
static $b: packed(7,3) inz(2.5)
static $sum: packed(9,3)
static $prod: packed(15,5)
static $quot: packed(15,10)
static $rem: packed(5,0)
static $half: packed(5,0)
static $trunc: packed(5,0)
static $neg: zoned(7,2)
static $i: i32
static $f: f64
static $cmp: ind
static $back: packed(9,2)
proc MAIN() entry {
L0:
  dec.add $sum, $a, $b prec(9,3)
  dec.mul $prod, $a, $b prec(13,5)
  dec.div $quot, $a, $b prec(20,10)
  dec.rem $rem, $a, 7 prec(5,0)
  dec.mov $half, $a halfadj
  dec.mov $trunc, $a
  dec.neg $neg, $b
  %i = cvt.i32 $a
  store $i, %i
  %f = cvt.f64 $b
  store $f, %f
  %c = dec.cmp.gt $a, $b
  store $cmp, %c
  dec.mov $back, %i
  ret
}
");
    assert_eq!(num(&p, "sum"), "125.950");
    assert_eq!(num(&p, "prod"), "308.62500");
    assert_eq!(num(&p, "quot"), "49.3800000000");
    assert_eq!(num(&p, "rem"), "4");
    assert_eq!(num(&p, "half"), "123");
    assert_eq!(num(&p, "trunc"), "123");
    assert_eq!(num(&p, "neg"), "-2.50");
    assert_eq!(num(&p, "i"), "123");
    assert_eq!(num(&p, "f"), "2.5");
    assert_eq!(statics(&p, "cmp"), vec![0xF1]);
    assert_eq!(num(&p, "back"), "123.00");
    assert!(p.job_ref().raised.is_empty());
}

#[test]
fn decimal_errors_raise_conditions() {
    let mut p = run(r#"
module DECERR
static $small: packed(3,0)
static $big: packed(9,0) inz(123456)
static $bad: packed(3,0) inz(x"FFFF")
static $z: packed(3,0)
static $got: i32
proc MAIN() entry {
L0:
  monitor M1, MCH1210 -> SIZE, *ALL -> OTHER
  dec.mov $small, $big
  store $got, 1
  endmon M1
  br NEXT
SIZE:
  store $got, 2
  br NEXT
OTHER:
  store $got, 3
  br NEXT
NEXT:
  monitor M2, 907 -> DATA, *ALL -> DONE
  dec.add $z, $bad, 1 prec(3,0)
  endmon M2
  br DONE
DATA:
  %g = load $got
  %g2 = mul %g, 10
  store $got, %g2
  br DONE
DONE:
  dec.div $z, $big, 0 prec(9,0)
  ret
}
"#);
    assert_eq!(num(&p, "got"), "20");
    let j = p.job();
    assert_eq!(
        j.handled.iter().map(|e| e.cond).collect::<Vec<_>>(),
        vec![Cond::Mch1210, Cond::Mch1202]
    );
    assert_eq!(j.exception.as_ref().map(|e| e.cond), Some(Cond::Mch1211));
    assert!(j.pending());
}

#[test]
fn text_operations() {
    let p = run(r#"
module TXT
static $name: char(10) inz("  SMITH ")
static $cat: char(20)
static $sub: char(3)
static $trim: varchar(10)
static $xl: char(10)
static $rep: char(12)
static $pos: i32
static $chk: i32
static $len: i32
static $eq: ind
static $lt: ind
static $u: ucs2(6)
static $u2: char(6)
proc MAIN() entry {
L0:
  str.trim $trim, $name
  str.cat $cat, $trim, ", JOHN"
  str.subst $sub, $trim, 1, 3
  %p = str.scan "IT", $name
  store $pos, %p
  %c = str.check "SMITH ", $name, 3
  store $chk, %c
  str.xlate $xl, "HT", "ht", $name
  str.replace $rep, $cat, "JANE", 8, 4
  %l = str.len $trim
  store $len, %l
  %e = str.cmp.eq $trim, "SMITH"
  store $eq, %e
  %t = str.cmp.gt "ABC", $name
  store $lt, %t
  cvt $u, $trim
  cvt $u2, $u
  ret
}
"#);
    let trim = statics(&p, "trim");
    assert_eq!(&trim[..2], &[0, 5]);
    assert_eq!(ccsid::from_ebcdic(&trim[2..7]), "SMITH");
    assert_eq!(text(&p, "cat"), "SMITH, JOHN");
    assert_eq!(text(&p, "sub"), "SMI");
    assert_eq!(num(&p, "pos"), "5");
    assert_eq!(num(&p, "chk"), "0");
    assert_eq!(text(&p, "xl"), "  SMIth");
    assert_eq!(text(&p, "rep"), "SMITH, JANE");
    assert_eq!(num(&p, "len"), "5");
    assert_eq!(statics(&p, "eq"), vec![0xF1]);
    assert_eq!(statics(&p, "lt"), vec![0xF1]);
    assert_eq!(
        statics(&p, "u"),
        vec![0, 0x53, 0, 0x4D, 0, 0x49, 0, 0x54, 0, 0x48, 0, 0x20]
    );
    assert_eq!(text(&p, "u2"), "SMITH");
}

#[test]
fn conversions_between_classes() {
    let p = run(r#"
module CVT
static $txt: char(12) inz(" -12.5 ")
static $n: packed(7,2)
static $back: char(12)
static $d: date(*ISO)
static $dtxt: char(10) inz("03/01/2026")
static $dnum: packed(8,0)
static $usa: char(10)
static $ts: timestamp(6)
static $tsdate: date(*USA)
static $flag: ind
static $ftxt: char(1)
static $i: i32 inz(7)
static $itxt: char(5)
proc MAIN() entry {
L0:
  cvt $n, $txt
  cvt $back, $n
  cvt $d, $dtxt datfmt(*USA)
  cvt $dnum, $d
  cvt $usa, $d datfmt(*USA)
  cvt $ts, $d
  cvt $tsdate, $ts
  mov $ftxt, "1"
  cvt $flag, $ftxt
  cvt $itxt, $i
  ret
}
"#);
    assert_eq!(num(&p, "n"), "-12.50");
    assert_eq!(text(&p, "back"), "-12.50");
    assert_eq!(text(&p, "d"), "2026-03-01");
    assert_eq!(num(&p, "dnum"), "20260301");
    assert_eq!(text(&p, "usa"), "03/01/2026");
    assert_eq!(text(&p, "ts"), "2026-03-01-00.00.00.000000");
    assert_eq!(text(&p, "tsdate"), "03/01/2026");
    assert_eq!(statics(&p, "flag"), vec![0xF1]);
    assert_eq!(text(&p, "itxt"), "7");
}

#[test]
fn date_arithmetic() {
    let p = run(r#"
module DT
static $d: date(*ISO) inz("2024-01-31")
static $next: date(*ISO)
static $later: date(*ISO)
static $days: i64
static $months: i64
static $yr: i32
static $t: time(*HMS) inz("23:30:00")
static $t2: time(*HMS)
static $ok: ind
static $bad: ind
static $now: timestamp(0)
static $nowd: date(*ISO)
static $same: ind
static $dtx: char(10)
static $ntx: char(10)
proc MAIN() entry {
L0:
  dt.add $next, $d, 1, *MONTHS
  dt.add $later, $d, 400, *DAYS
  %dd = dt.diff $later, $d, *DAYS
  store $days, %dd
  %mm = dt.diff $later, $d, *MONTHS
  store $months, %mm
  %y = dt.extract $later, *YEARS
  store $yr, %y
  dt.add $t2, $t, 45, *MINUTES
  %ok = dt.test "2024-02-29" datfmt(*ISO)
  store $ok, %ok
  %bad = dt.test "2023-02-29" datfmt(*ISO)
  store $bad, %bad
  dt.now $now
  dt.now $nowd
  cvt $d, $now
  cvt $dtx, $d
  cvt $ntx, $nowd
  %s = str.cmp.eq $dtx, $ntx
  store $same, %s
  ret
}
"#);
    assert_eq!(text(&p, "next"), "2024-02-29");
    assert_eq!(text(&p, "later"), "2025-03-06");
    assert_eq!(num(&p, "days"), "400");
    assert_eq!(num(&p, "months"), "13");
    assert_eq!(num(&p, "yr"), "2025");
    assert_eq!(text(&p, "t2"), "00:15:00");
    assert_eq!(statics(&p, "ok"), vec![0xF1]);
    assert_eq!(statics(&p, "bad"), vec![0xF0]);
    assert_eq!(statics(&p, "same"), vec![0xF1]);
    assert_eq!(text(&p, "now").len(), 19);
}

#[test]
fn pointers_and_based_storage() {
    let mut p = run(r#"
module PTR
type Rec = struct { id: i32, name: char(6) }
static $rec: Rec
static $pp: ptr
based $view: Rec on $pp
static $second: i32
static $diff: i64
static $after: i32
static $dead: i32
proc FRAME() -> ptr {
  auto $local: i32 inz(99)
L0:
  %a = addr $local
  ret %a
}
proc MAIN() entry {
  auto $loc: Rec
L0:
  store $rec.id, 42
  mov $rec.name, "ABCDEF"
  %p = addr $rec
  store $pp, %p
  %id = load $view.id
  store $second, %id
  %q = ptradd %p, 4
  %c = load [%q]:u8
  %d = ptrdiff %q, %p
  store $diff, %d
  %lp = addr $loc
  monitor M, MCH0601 -> RANGE
  %bad = ptradd %lp, 11
  store $after, 1
  endmon M
  br DEAD
RANGE:
  store $after, 2
  br DEAD
DEAD:
  %fp = callb FRAME()
  monitor M2, MCH3402 -> GONE
  %v = load [%fp]:i32
  store $dead, %v
  endmon M2
  ret
GONE:
  store $dead, -1
  ret
}
"#);
    assert_eq!(num(&p, "second"), "42");
    assert_eq!(num(&p, "diff"), "4");
    assert_eq!(num(&p, "after"), "2");
    assert_eq!(num(&p, "dead"), "-1");
    let j = p.job();
    assert_eq!(
        j.handled.iter().map(|e| e.cond).collect::<Vec<_>>(),
        vec![Cond::Mch0601, Cond::Mch3402]
    );
    assert!(!j.pending());
}

#[test]
fn monitors_nest_and_percolate() {
    let mut p = run(r#"
module MON
static $log: char(20)
static $at: i32 inz(0)
proc INNER() {
  auto $z: packed(3,0)
L0:
  monitor IN, MCH1210 -> SIZE
  dec.div $z, 1, 0 prec(3,0)
  endmon IN
  ret
SIZE:
  mov $log, "wrong handler"
  ret
}
proc MAIN() entry {
L0:
  monitor OUT, *PROGRAM -> CAUGHT
  callb INNER()
  mov $log, "not reached"
  endmon OUT
  ret
CAUGHT:
  mov $log, "caught in caller"
  %a = load $at
  %a2 = add %a, 1
  store $at, %a2
  dsply $log
  dsply $at
  ret
}
"#);
    assert_eq!(text(&p, "log"), "caught in caller");
    assert_eq!(num(&p, "at"), "1");
    let j = p.job();
    assert_eq!(
        j.console,
        vec!["caught in caller".to_string(), "1".to_string()]
    );
    assert_eq!(j.handled.len(), 1);
    assert_eq!(j.handled[0].cond, Cond::Mch1211);
    assert_eq!(j.handled[0].depth, 2, "raised inside INNER");
    assert_eq!(j.depth, 0);
    assert!(!j.pending());
}

#[test]
fn unhandled_exception_percolates_out() {
    let mut p = run("
module OUT
static $p: packed(3,0)
proc MAIN() entry {
L0:
  dec.div $p, 1, 0 prec(3,0)
  mov $p, 5
  ret
}
");
    let j = p.job();
    assert!(j.pending());
    assert_eq!(j.exception.as_ref().unwrap().msgid, "MCH1211");
    assert_eq!(j.exception.as_ref().unwrap().status, 102);
    assert_eq!(j.depth, 0);
}
