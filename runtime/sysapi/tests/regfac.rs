// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The registration facility APIs from IR programs: `QUSADDEP` adds an
//! exit program, `QUSRTVEI` returns it in `EXTI0200` and the points in
//! `EXTI0100` at `QUSREG`'s offsets, `QUSRMVEP` removes it and the
//! error code reports `CPF3CDD` the second time; `QUSRGPT` and
//! `QUSDRGPT` register and deregister; the `QUSRGFA2` procedure
//! answers as the program does.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use mi::MemUserSpaces;
use mi::exec::Program;
use sysapi::MemProvider;

const ERRC: &str = "x\"0000002000000000000000000000000000000000000000000000000000000000\"";

fn run(text: &str) -> (Program, Rc<RefCell<MemProvider>>) {
    let provider = Rc::new(RefCell::new(MemProvider::default()));
    let mut p = Program::from_text(text).unwrap_or_else(|e| panic!("{e}"));
    p.job()
        .usrspcs
        .set_source(Box::new(MemUserSpaces::default()));
    sysapi::install(p.job(), provider.clone());
    p.call_entry().unwrap_or_else(|e| panic!("{e}"));
    (p, provider)
}

fn bytes(p: &Program, name: &str) -> Vec<u8> {
    p.static_bytes(name).unwrap_or_else(|| panic!("no {name}"))
}

fn bin4(b: &[u8], off: usize) -> i32 {
    i32::from_be_bytes(b[off..off + 4].try_into().unwrap())
}

fn chars(b: &[u8], off: usize, n: usize) -> String {
    ccsid::decode_trimmed(&b[off..off + n])
}

#[test]
fn exit_programs_are_added_retrieved_and_removed() {
    let (p, provider) = run(&format!(
        "
module REG
extern sysapi QUSADDEP($n: char(20), $f: char(8), $num: i32, $pgm: char(20), $d: char(9), $dl: i32, $attr: char(20), $err: char(32))
extern sysapi QUSRTVEI($h: char(16), $r: char(1000), $rl: i32, $fmt: char(8), $n: char(20), $f: char(8), $num: i32, $sel: char(4), $err: char(32))
extern sysapi QUSRMVEP($n: char(20), $f: char(8), $num: i32, $err: char(32))
static $name: char(20) inz(\"QIBM_QZRC_RMT\")
static $fmt: char(8) inz(\"CZRC0100\")
static $low: i32 inz(-1)
static $one: i32 inz(1)
static $pgm: char(20) inz(\"RMTEXIT   QSTEST\")
static $data: char(9) inz(\"HELLODATA\")
static $dl: i32 inz(9)
static $attr: char(20) inz(x\"00000001000000100000000500000001F2404040\")
static $err: char(32) inz({ERRC})
static $err2: char(32) inz({ERRC})
static $h: char(16) inz(*BLANKS)
static $r200: char(1000)
static $r100: char(1000)
static $rafter: char(1000)
static $rl: i32 inz(1000)
static $f200: char(8) inz(\"EXTI0200\")
static $f100: char(8) inz(\"EXTI0100\")
static $gen: char(20) inz(\"QIBM_QZ*\")
static $all: char(8) inz(\"*ALL\")
static $nosel: char(4) inz(x\"00000000\")
proc REG() entry {{
L0:
  sysapi QUSADDEP($name, $fmt, $low, $pgm, $data, $dl, $attr, $err)
  sysapi QUSRTVEI($h, $r200, $rl, $f200, $name, $fmt, $low, $nosel, $err)
  sysapi QUSRTVEI($h, $r100, $rl, $f100, $gen, $all, $low, $nosel, $err)
  sysapi QUSRMVEP($name, $fmt, $one, $err)
  sysapi QUSRTVEI($h, $rafter, $rl, $f200, $name, $fmt, $low, $nosel, $err)
  sysapi QUSRMVEP($name, $fmt, $one, $err2)
  ret
}}
"
    ));
    let e = bytes(&p, "err");
    assert_eq!(bin4(&e, 4), 0, "{}", chars(&e, 8, 7));

    // EXTI0200: one program, its data after the fixed part.
    let r = bytes(&p, "r200");
    assert_eq!(bin4(&r, 28), 1, "programs returned");
    let at = usize::try_from(bin4(&r, 24)).unwrap();
    assert_eq!(at, 36);
    assert_eq!(bin4(&r, 32), 76, "entry length");
    let e0 = &r[at..];
    assert_eq!(chars(e0, 4, 20), "QIBM_QZRC_RMT");
    assert_eq!(chars(e0, 24, 8), "CZRC0100");
    assert_eq!(chars(e0, 32, 1), "1", "registered");
    assert_eq!(bin4(e0, 36), 1, "*LOW gave number 1");
    assert_eq!(chars(e0, 40, 10), "RMTEXIT");
    assert_eq!(chars(e0, 50, 10), "QSTEST");
    let off = usize::try_from(bin4(e0, 64)).unwrap();
    let len = usize::try_from(bin4(e0, 68)).unwrap();
    assert_eq!(len, 9);
    assert_eq!(
        chars(&r, off, len),
        "HELLODATA",
        "the data offset is from the receiver's start"
    );
    assert_eq!(chars(e0, 72, 1), "2", "threadsafe");
    assert_eq!(bin4(&r, 0), bin4(&r, 4), "everything fit");
    assert_eq!(chars(&r, 8, 16), "", "no continuation");

    // EXTI0100: the host server points, the remote command one with
    // its program counted.
    let r = bytes(&p, "r100");
    let n = usize::try_from(bin4(&r, 28)).unwrap();
    let len = usize::try_from(bin4(&r, 32)).unwrap();
    assert_eq!(len, 204);
    let fits = (1000 - 36) / 204;
    assert_eq!(n, fits, "as many as fit");
    assert!(bin4(&r, 4) > bin4(&r, 0), "more available than returned");
    assert_ne!(chars(&r, 8, 16), "", "a continuation handle");
    let names: Vec<String> = (0..n).map(|i| chars(&r, 36 + i * len, 20)).collect();
    assert!(names.iter().all(|s| s.starts_with("QIBM_QZ")), "{names:?}");
    let all = provider
        .borrow()
        .regfac
        .select("QIBM_QZ*", "*ALL", regfac::Which::All)
        .len();
    assert!(all > n);

    let r = bytes(&p, "rafter");
    assert_eq!(bin4(&r, 28), 0, "removed");
    let e2 = bytes(&p, "err2");
    assert_eq!(chars(&e2, 8, 7), "CPF3CDD");
    assert_eq!(bin4(&e2, 16), 1, "the number in the message data");
}

#[test]
fn points_are_registered_and_retrieved_by_procedure() {
    let ctl = controls();
    let (p, provider) = run(&format!(
        "
module REG2
extern sysapi QUSRGPT($n: char(20), $f: char(8), $c: char(82), $err: char(32))
extern sysapi QUSDRGPT($n: char(20), $f: char(8), $err: char(32))
extern proc QusRetrieveExitInformation($h: char(16), $r: char(400), $rl: i32, $fmt: char(8), $n: char(20), $f: char(8), $num: i32, $sel: char(4), $err: char(32))
static $name: char(20) inz(\"ACME_EXIT_POINT\")
static $fmt: char(8) inz(\"ACME0100\")
static $ctl: char(82) inz(x\"{ctl}\")
static $err: char(32) inz({ERRC})
static $err2: char(32) inz({ERRC})
static $h: char(16) inz(*BLANKS)
static $r: char(400)
static $rl: i32 inz(400)
static $f100: char(8) inz(\"EXTI0100\")
static $low: i32 inz(-1)
static $nosel: char(4) inz(x\"00000000\")
proc REG2() entry {{
L0:
  sysapi QUSRGPT($name, $fmt, $ctl, $err)
  callb QusRetrieveExitInformation($h, $r, $rl, $f100, $name, $fmt, $low, $nosel, $err)
  sysapi QUSRGPT($name, $fmt, $ctl, $err2)
  sysapi QUSDRGPT($name, $fmt, $err)
  ret
}}
"
    ));
    let e = bytes(&p, "err");
    assert_eq!(bin4(&e, 4), 0, "{}", chars(&e, 8, 7));
    let r = bytes(&p, "r");
    assert_eq!(bin4(&r, 28), 1);
    let e0 = &r[36..];
    assert_eq!(chars(e0, 0, 20), "ACME_EXIT_POINT");
    assert_eq!(chars(e0, 20, 8), "ACME0100");
    assert_eq!(bin4(e0, 28), 4, "maximum from key 2");
    assert_eq!(bin4(e0, 32), 0);
    assert_eq!(chars(e0, 36, 1), "1", "deregistration allowed by default");
    assert_eq!(chars(e0, 38, 1), "1", "registered");
    assert_eq!(chars(e0, 39, 10), "*NONE");
    assert_eq!(chars(e0, 123, 1), "T");
    assert_eq!(chars(e0, 151, 50), "ACME EXIT POINT");
    assert_eq!(chars(&bytes(&p, "err2"), 8, 7), "CPF3CD1");
    assert!(
        provider
            .borrow()
            .regfac
            .point("ACME_EXIT_POINT", "ACME0100")
            .is_none(),
        "deregistered"
    );
}

/// Exit point controls: key 2 (maximum 4) and key 10 (a text).
fn controls() -> String {
    let mut b: Vec<u8> = Vec::new();
    b.extend(2i32.to_be_bytes());
    for (key, data) in [
        (2i32, 4i32.to_be_bytes().to_vec()),
        (10, {
            let mut t = ccsid::to_ebcdic("ACME EXIT POINT").unwrap();
            t.resize(50, 0x40);
            t
        }),
    ] {
        let len = i32::try_from(data.len()).unwrap();
        b.extend((12 + len).to_be_bytes());
        b.extend(key.to_be_bytes());
        b.extend(len.to_be_bytes());
        b.extend(data);
    }
    assert_eq!(b.len(), 82);
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02X}");
        s
    })
}
