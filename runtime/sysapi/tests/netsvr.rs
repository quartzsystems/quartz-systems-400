// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `QZLS*` natives from an IR program: parameters gathered in
//! `netsvrcfg`'s order, the receiver, list information and user space
//! filled, the error code set, against `netsvrcfg`'s host in memory.

use std::cell::RefCell;
use std::rc::Rc;

use mi::MemUserSpaces;
use mi::exec::Program;
use netsvrcfg::api::MemHost;
use sysapi::{ApiFail, MemProvider};

fn run(text: &str, host: &Rc<RefCell<MemHost>>) -> Program {
    let mut provider = MemProvider::default();
    let h = host.clone();
    provider.netsvr = Some(Box::new(move |api, parms| {
        netsvrcfg::api::call(&mut *h.borrow_mut(), api, parms).map_err(|e| ApiFail {
            msgid: e.id.to_string(),
            data: e.data,
        })
    }));
    let mut p = Program::from_text(text).unwrap_or_else(|e| panic!("{e}"));
    p.job()
        .usrspcs
        .set_source(Box::new(MemUserSpaces::default()));
    sysapi::install(p.job(), Rc::new(RefCell::new(provider)));
    p.call_entry().unwrap_or_else(|e| panic!("{e}"));
    p
}

fn bin4(b: &[u8], off: usize) -> i32 {
    i32::from_be_bytes(b[off..off + 4].try_into().expect("4 bytes"))
}

fn chars(b: &[u8], off: usize, n: usize) -> String {
    ccsid::decode_trimmed(&b[off..off + n])
}

const ERRC: &str = "x\"00000110000000000000000000000000\"";

#[test]
fn shares_are_added_and_listed() {
    let host = Rc::new(RefCell::new(MemHost::new("QS400")));
    host.borrow_mut()
        .paths
        .push(("/home".into(), "QSYS".into()));
    let p = run(
        &format!(
            "
module NS
extern sysapi QZLSADFS($n: char(12), $p: char(5), $pl: i32, $cc: i32, $t: char(50), $pm: i32, $mx: i32, $err: char(272))
extern sysapi QZLSOLST($r: char(4000), $rl: i32, $li: char(64), $f: char(8), $q: char(15), $err2: char(272))
extern sysapi QUSCRTUS($u: char(20), $a: char(10), $s: i32, $i: char(1), $au: char(10), $tx: char(50), $rp: char(10), $err3: char(272))
extern sysapi QZLSLSTI($u: char(20), $f: char(8), $q: char(15), $err4: char(272))
static $n: char(12) inz(\"HOME\")
static $p: char(5) inz(\"/home\")
static $pl: i32 inz(5)
static $cc: i32 inz(0)
static $t: char(50) inz(\"Home directories\")
static $pm: i32 inz(2)
static $mx: i32 inz(-1)
static $err: char(272) inz({ERRC})
static $r: char(4000)
static $rl: i32 inz(4000)
static $li: char(64)
static $f: char(8) inz(\"ZLSL0800\")
static $q: char(15) inz(\"*ALL\")
static $err2: char(272) inz({ERRC})
static $u: char(20) inz(\"NSLIST    QTEMP\")
static $a: char(10) inz(\"NS\")
static $s: i32 inz(16)
static $i: char(1) inz(x\"00\")
static $au: char(10) inz(\"*ALL\")
static $tx: char(50)
static $rp: char(10) inz(\"*YES\")
static $err3: char(272) inz({ERRC})
static $f2: char(8) inz(\"ZLSL0100\")
static $err4: char(272) inz({ERRC})
static $f3: char(8) inz(\"ZLSL9999\")
static $err5: char(272) inz({ERRC})
proc NS() entry {{
L0:
  sysapi QZLSADFS($n, $p, $pl, $cc, $t, $pm, $mx, $err)
  sysapi QZLSOLST($r, $rl, $li, $f, $q, $err2)
  sysapi QUSCRTUS($u, $a, $s, $i, $au, $tx, $rp, $err3)
  sysapi QZLSLSTI($u, $f2, $q, $err4)
  sysapi QZLSOLST($r, $rl, $li, $f3, $q, $err5)
  ret
}}
"
        ),
        &host,
    );
    for e in ["err", "err2", "err3", "err4"] {
        let b = p.static_bytes(e).expect("errc");
        assert_eq!(bin4(&b, 4), 0, "{e}: {}", chars(&b, 8, 7));
    }
    let li = p.static_bytes("li").expect("list info");
    let r = p.static_bytes("r").expect("receiver");
    assert_eq!((bin4(&li, 0), bin4(&li, 4), bin4(&li, 8)), (2, 2, 16));
    assert_eq!(chars(&r, 0, 12), "HOME");
    assert_eq!(chars(&r, 16, 12), "QIBM");
    let e = p.static_bytes("err5").expect("errc");
    assert_eq!(chars(&e, 8, 7), "CPFB688");
    assert_eq!(bin4(&e, 26), 1);
    let mut p = p;
    let us = p
        .job()
        .usrspcs
        .mapped("NSLIST", "QTEMP")
        .expect("the space")
        .to_vec();
    assert_eq!(bin4(&us, 0x40), 576);
    assert_eq!(chars(&us, 0x48, 8), "ZLSL0100");
    assert_eq!(bin4(&us, 0x84), 2);
    assert_eq!(chars(&us, 619 + 4, 12), "HOME");
    assert_eq!(bin4(&us, 619 + 36), 619 + 164);
    assert_eq!(chars(&us, 619 + 164, 5), "/home");
    assert!(
        host.borrow()
            .config
            .as_ref()
            .is_some_and(|c| c.share("HOME").is_some())
    );
}
