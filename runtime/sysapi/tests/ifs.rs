// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! IR programs calling the IFS procedures over an IFS in memory: open,
//! write, read and close through the job's descriptor table, `stat`
//! into IBM's layout, a directory listing, `errno` through `__errno()`,
//! text-mode conversion, `fcntl` byte-range locks (a pointer third
//! argument) and `Qp0lGetAttr`.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use ifs::Errno;
use ifs::client::IfsClient;
use ifs::ops::OpenArgs;
use mi::exec::Program;
use sysapi::MemProvider;
use sysapi::ifs::layout::{self as lay, STAT};
use sysapi::ifs::memory::MemIfs;

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, c| {
        let _ = write!(s, "{c:02X}");
        s
    })
}

/// A static holding `s` in EBCDIC with a NUL.
fn cstr(name: &str, s: &str) -> String {
    let mut b = ccsid::to_ebcdic(s).unwrap();
    b.push(0);
    format!("static ${name}: bytes({}) inz(x\"{}\")\n", b.len(), hex(&b))
}

fn run(sys: &MemIfs, text: &str) -> Program {
    let mut p = Program::from_text(text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    sysapi::install(p.job(), Rc::new(RefCell::new(MemProvider::default())));
    let client = Rc::new(sys.client("QSTEST", 7, 37));
    sysapi::ifs::configure(p.job(), client, 37);
    p.call_entry().unwrap_or_else(|e| panic!("{e}"));
    p
}

fn bin4(p: &Program, name: &str) -> i32 {
    let b = p.static_bytes(name).unwrap();
    i32::from_be_bytes(b[..4].try_into().unwrap())
}

const PROTOS: &str = "
extern proc open(byval $p: ptr, byval $o: i32, byval $m: u32, byval $c: u32) -> i32
extern proc read(byval $fd: i32, byval $b: ptr, byval $n: u32) -> i32
extern proc write(byval $fd: i32, byval $b: ptr, byval $n: u32) -> i32
extern proc close(byval $fd: i32) -> i32
extern proc stat(byval $p: ptr, byval $b: ptr) -> i32
extern proc fstat(byval $fd: i32, byval $b: ptr) -> i32
extern proc unlink(byval $p: ptr) -> i32
extern proc mkdir(byval $p: ptr, byval $m: u32) -> i32
extern proc opendir(byval $p: ptr) -> ptr
extern proc readdir(byval $d: ptr) -> ptr
extern proc closedir(byval $d: ptr) -> i32
extern proc lseek(byval $fd: i32, byval $o: i32, byval $w: i32) -> i32
extern proc __errno() -> ptr
";

#[test]
fn write_read_stat_and_errno() {
    let sys = MemIfs::new();
    let text = format!(
        "
module T1
{PROTOS}
{}{}
static $data: bytes(5) inz(x\"C8C5D3D3D6\")
static $back: bytes(16)
static $st: bytes(128)
static $fst: bytes(128)
static $fd: i32 inz(-9)
static $wn: i32 inz(-9)
static $rn: i32 inz(-9)
static $pos: i32 inz(-9)
static $src: i32 inz(-9)
static $crc: i32 inz(-9)
static $mrc: i32 inz(-9)
static $merr: i32 inz(0)
proc T1() entry {{
L0:
  %p = addr $path
  %fd = callb open(%p, 74, 420, 0)
  store $fd, %fd
  %d = addr $data
  %w = callb write(%fd, %d, 5)
  store $wn, %w
  %f = addr $fst
  %x = callb fstat(%fd, %f)
  %c = callb close(%fd)
  %fd2 = callb open(%p, 1, 0, 0)
  %s = callb lseek(%fd2, 1, 0)
  store $pos, %s
  %b = addr $back
  %r = callb read(%fd2, %b, 16)
  store $rn, %r
  %c2 = callb close(%fd2)
  %sb = addr $st
  %rc = callb stat(%p, %sb)
  store $src, %rc
  %c3 = callb close(%fd2)
  store $crc, %c3
  %m = addr $missing
  %mr = callb open(%m, 1, 0, 0)
  store $mrc, %mr
  %ep = callb __errno()
  %e = load [%ep]:i32
  store $merr, %e
  ret
}}
",
        cstr("path", "/home/QSTEST/a.txt"),
        cstr("missing", "/home/QSTEST/none.txt"),
    );
    let p = run(&sys, &text);
    assert_eq!(bin4(&p, "fd"), 0, "the first descriptor");
    assert_eq!(bin4(&p, "wn"), 5);
    assert_eq!(bin4(&p, "pos"), 1);
    assert_eq!(bin4(&p, "rn"), 4);
    assert_eq!(&p.static_bytes("back").unwrap()[..4], &[0xC5, 0xD3, 0xD3, 0xD6]);
    assert_eq!(bin4(&p, "src"), 0);
    let st = p.static_bytes("st").unwrap();
    assert_eq!(lay::get_n(&st, &STAT, "st_size"), 5);
    assert_eq!(lay::get_n(&st, &STAT, "st_mode") & 0o170_000, 0o100_000);
    assert_eq!(lay::get_n(&st, &STAT, "st_mode") & 0o707, 0o604, "no group: its bits are 0");
    assert_eq!(ccsid::from_ebcdic(&st[48..58]), "*STMF     ");
    assert_eq!(st[58], 0);
    assert_eq!(lay::get_n(&st, &STAT, "st_uid"), 110, "QSTEST's UID");
    let fst = p.static_bytes("fst").unwrap();
    assert_eq!(lay::get_n(&fst, &STAT, "st_size"), 5, "fstat while open");
    assert_eq!(bin4(&p, "crc"), -1, "closed twice");
    assert_eq!(bin4(&p, "mrc"), -1);
    assert_eq!(bin4(&p, "merr"), Errno::ENOENT.code());
}

#[test]
fn a_directory_listing() {
    let sys = MemIfs::new();
    let c = sys.client("QSTEST", 1, 37);
    for n in ["b.txt", "a.txt"] {
        let fd = c.open(&format!("/home/QSTEST/{n}"), OpenArgs::new(0o112).mode(0o644)).unwrap();
        c.close(fd).unwrap();
    }
    let text = format!(
        "
module T2
{PROTOS}
{}
static $names: bytes(64)
static $count: i32 inz(0)
proc T2() entry {{
L0:
  %p = addr $dir
  %d = callb opendir(%p)
  br NEXT
NEXT:
  %e = callb readdir(%d)
  %nul = const.ptr *NULL
  %z = cmp.eq %e, %nul
  brif %z, DONE, ONE
ONE:
  %n = load $count
  %n2 = add %n, 1
  store $count, %n2
  %at = ptradd %e, 56
  %first = load [%at]:u8
  %n64 = cvt.i64 %n
  %base = addr $names
  %slot = ptradd %base, %n64
  store [%slot]:u8, %first
  br NEXT
DONE:
  %c = callb closedir(%d)
  ret
}}
",
        cstr("dir", "/home/QSTEST"),
    );
    let p = run(&sys, &text);
    assert_eq!(bin4(&p, "count"), 4, ". .. a.txt b.txt");
    let names = p.static_bytes("names").unwrap();
    assert_eq!(&names[..4], &[0x4B, 0x4B, 0x81, 0x82], "the first byte of each name, in order");
}

#[test]
fn text_mode_converts_to_the_job_ccsid() {
    let sys = MemIfs::new();
    // O_CREAT | O_WRONLY | O_CCSID with 819: the file is tagged 819 and
    // the bytes written are kept as they are.
    let text = format!(
        "
module T3
{PROTOS}
{}
static $ascii: bytes(4) inz(x\"4142430A\")
static $back: bytes(8)
static $st: bytes(128)
static $rn: i32 inz(-9)
proc T3() entry {{
L0:
  %p = addr $path
  %fd = callb open(%p, 42, 420, 819)
  %a = addr $ascii
  %w = callb write(%fd, %a, 4)
  %c = callb close(%fd)
  %fd2 = callb open(%p, 16777217, 0, 0)
  %b = addr $back
  %r = callb read(%fd2, %b, 8)
  store $rn, %r
  %c2 = callb close(%fd2)
  %sb = addr $st
  %s = callb stat(%p, %sb)
  ret
}}
",
        cstr("path", "/home/QSTEST/t.txt"),
    );
    let p = run(&sys, &text);
    let st = p.static_bytes("st").unwrap();
    assert_eq!(lay::get_n(&st, &STAT, "st_ccsid"), 819);
    assert_eq!(lay::get_n(&st, &STAT, "st_size"), 4);
    assert_eq!(bin4(&p, "rn"), 4);
    assert_eq!(&p.static_bytes("back").unwrap()[..4], &[0xC1, 0xC2, 0xC3, 0x25], "EBCDIC, LF as 0x25");
}

#[test]
fn byte_range_locks_through_fcntl() {
    let sys = MemIfs::new();
    let text = format!(
        "
module T4
{PROTOS}
extern proc fcntl(byval $fd: i32, byval $c: i32, byval $l: ptr) -> i32
{}
static $lk: bytes(48) inz(x\"00020000000000000000000A\")
static $q: bytes(48) inz(x\"00010000000000000000000A\")
static $set: i32 inz(-9)
static $get: i32 inz(-9)
static $fl: i32 inz(-9)
static $err: i32 inz(0)
proc T4() entry {{
L0:
  %p = addr $path
  %fd = callb open(%p, 76, 420, 0)
  %l = addr $lk
  %s = callb fcntl(%fd, 4, %l)
  store $set, %s
  %qq = addr $q
  %g = callb fcntl(%fd, 3, %qq)
  store $get, %g
  %nul = const.ptr *NULL
  %f = callb fcntl(%fd, 6, %nul)
  store $fl, %f
  %ep = callb __errno()
  %e = load [%ep]:i32
  store $err, %e
  %c = callb close(%fd)
  ret
}}
",
        cstr("path", "/home/QSTEST/l.dat"),
    );
    let p = run(&sys, &text);
    assert_eq!(bin4(&p, "set"), 0, "F_SETLK");
    assert_eq!(bin4(&p, "get"), 0, "F_GETLK");
    let q = p.static_bytes("q").unwrap();
    assert_eq!(u16::from_be_bytes([q[0], q[1]]), 3, "our own lock never blocks: F_UNLCK");
    assert_eq!(bin4(&p, "fl") & 0o7, 0o4, "F_GETFL: O_RDWR, {:o} errno {}", bin4(&p, "fl"), bin4(&p, "err"));
}

#[test]
fn qp0lgetattr_answers_the_ccsid_and_type() {
    let sys = MemIfs::new();
    {
        let c = sys.client("QSTEST", 3, 37);
        let fd = c.open("/home/QSTEST/g.txt", OpenArgs::new(0o52).mode(0o644).conv(1208)).unwrap();
        c.close(fd).unwrap();
    }
    let q = {
        let mut b = vec![0u8; 32];
        let name = ccsid::to_ebcdic("/home/QSTEST/g.txt").unwrap();
        b[16..20].copy_from_slice(&u32::try_from(name.len()).unwrap().to_be_bytes());
        b[20] = 0x61;
        b.extend(name);
        b
    };
    let text = format!(
        "
module T5
extern proc Qp0lGetAttr(byval $p: ptr, byval $l: ptr, byval $b: ptr, byval $n: u32, byval $need: ptr, byval $got: ptr, byval $f: u32) -> i32
static $qlg: bytes({}) inz(x\"{}\")
static $ids: bytes(12) inz(x\"00000002000000000000001B\")
static $buf: bytes(256)
static $need: i32 inz(0)
static $got: i32 inz(0)
static $rc: i32 inz(-9)
proc T5() entry {{
L0:
  %p = addr $qlg
  %l = addr $ids
  %b = addr $buf
  %n = addr $need
  %g = addr $got
  %r = callb Qp0lGetAttr(%p, %l, %b, 256, %n, %g, 1)
  store $rc, %r
  ret
}}
",
        q.len(),
        hex(&q),
    );
    let p = run(&sys, &text);
    assert_eq!(bin4(&p, "rc"), 0);
    let buf = p.static_bytes("buf").unwrap();
    // Object type: 16-byte header, 11 bytes, padded to 28.
    assert_eq!(u32::from_be_bytes(buf[0..4].try_into().unwrap()), 28);
    assert_eq!(u32::from_be_bytes(buf[4..8].try_into().unwrap()), 0);
    assert_eq!(ccsid::from_ebcdic(&buf[16..21]), "*STMF");
    assert_eq!(u32::from_be_bytes(buf[32..36].try_into().unwrap()), 27, "QP0L_ATTR_CCSID");
    assert_eq!(u16::from_be_bytes([buf[44], buf[45]]), 1208);
    assert_eq!(bin4(&p, "got"), 48);
}
