// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QZNFRTVE` and `QZNFNFSO` from an IR program: the parameters handed
//! to the provider in the service's order, the feedback, receiver and
//! output buffer filled from its answer, the error code set.

use std::cell::RefCell;
use std::rc::Rc;

use mi::exec::Program;
use sysapi::{ApiFail, MemProvider};

type Calls = Rc<RefCell<Vec<(String, Vec<Vec<u8>>)>>>;

fn run(text: &str, calls: &Calls) -> Program {
    let mut provider = MemProvider::default();
    let c = calls.clone();
    provider.nfs = Some(Box::new(move |api, parms| {
        c.borrow_mut().push((api.to_string(), parms.to_vec()));
        match api {
            // The feedback (16), then one entry of 20 bytes.
            "QZNFRTVE" => {
                let mut b = Vec::new();
                for v in [20u32, 20, 1, 0, 20, 16, 4, 819] {
                    b.extend_from_slice(&v.to_be_bytes());
                }
                b.extend_from_slice(b"/a/b");
                Ok(b)
            }
            _ if parms[0] == 9u32.to_be_bytes() => Err(ApiFail::id("CPF3C90")),
            _ => Ok(vec![0, 0, 0, 12, 0, 0, 0, 12, 0, 0, 0, 1]),
        }
    }));
    let mut p = Program::from_text(text).unwrap_or_else(|e| panic!("{e}"));
    sysapi::install(p.job(), Rc::new(RefCell::new(provider)));
    p.call_entry().unwrap_or_else(|e| panic!("{e}"));
    p
}

fn bin4(b: &[u8], off: usize) -> u32 {
    u32::from_be_bytes(b[off..off + 4].try_into().expect("4 bytes"))
}

const ERRC: &str = "x\"00000110000000000000000000000000\"";

#[test]
fn export_information_and_options() {
    let calls: Calls = Rc::default();
    let p = run(
        &format!(
            "
module NF
extern sysapi QZNFRTVE($r: char(100), $rl: i32, $fb: char(16), $f: char(8), $pa: char(5), $pl: i32, $pc: i32, $dc: i32, $h: i32, $err: char(272))
extern sysapi QZNFNFSO($o: i32, $in: char(4), $il: i32, $out: char(12), $ol: i32, $f2: char(8), $err2: char(272))
static $r: char(100)
static $rl: i32 inz(100)
static $fb: char(16)
static $f: char(8) inz(\"EXPE0200\")
static $pa: char(5) inz(\"/a/b \")
static $pl: i32 inz(4)
static $pc: i32 inz(37)
static $dc: i32 inz(819)
static $h: i32 inz(0)
static $err: char(272) inz({ERRC})
static $o: i32 inz(1)
static $in: char(4)
static $il: i32 inz(0)
static $out: char(12)
static $ol: i32 inz(12)
static $f2: char(8) inz(\"NFOP0101\")
static $err2: char(272) inz({ERRC})
static $o9: i32 inz(9)
static $err3: char(272) inz({ERRC})
proc NF() entry {{
L0:
  sysapi QZNFRTVE($r, $rl, $fb, $f, $pa, $pl, $pc, $dc, $h, $err)
  sysapi QZNFNFSO($o, $in, $il, $out, $ol, $f2, $err2)
  sysapi QZNFNFSO($o9, $in, $il, $out, $ol, $f2, $err3)
  ret
}}
"
        ),
        &calls,
    );
    let fb = p.static_bytes("fb").expect("feedback");
    assert_eq!((bin4(&fb, 0), bin4(&fb, 8)), (20, 1));
    let r = p.static_bytes("r").expect("receiver");
    assert_eq!(&r[16..20], b"/a/b");
    let out = p.static_bytes("out").expect("output");
    assert_eq!(bin4(&out, 8), 1);
    for e in ["err", "err2"] {
        assert_eq!(bin4(&p.static_bytes(e).unwrap(), 4), 0, "{e}");
    }
    let e3 = p.static_bytes("err3").unwrap();
    assert_eq!(&ccsid::decode_trimmed(&e3[8..15]), "CPF3C90");
    let c = calls.borrow();
    assert_eq!(c[0].0, "QZNFRTVE");
    assert_eq!(c[0].1[0], 100u32.to_be_bytes());
    assert_eq!(ccsid::decode_trimmed(&c[0].1[1]), "EXPE0200");
    assert_eq!(c[0].1[2].len(), 4);
    assert_eq!(c[0].1[4], 819u32.to_be_bytes());
    assert_eq!(c[1].0, "QZNFNFSO");
    assert_eq!(c[1].1[0], 1u32.to_be_bytes());
    assert!(c[1].1[1].is_empty());
}
