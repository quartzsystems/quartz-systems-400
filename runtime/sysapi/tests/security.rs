// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The security APIs' parameters and receivers, over canned answers.

use std::cell::RefCell;
use std::rc::Rc;

use mi::MemUserSpaces;
use mi::exec::Program;
use sec::api::{Answer, AuthInfo, ObjRow, Query, UserRow};
use sec::auth::bits::Auth;
use sec::profile::UsrPrf;
use sysapi::{ApiFail, MemProvider};

const ERRC: &str = "static $err: char(16) inz(x\"00000010000000000000000000000000\")";

fn bin4(b: &[u8], off: usize) -> i32 {
    i32::from_be_bytes(b[off..off + 4].try_into().unwrap())
}

fn chars(b: &[u8], off: usize, n: usize) -> String {
    ccsid::decode_trimmed(&b[off..off + n])
}

/// Answers as a small system would, recording every question.
fn provider(asked: &Rc<RefCell<Vec<Query>>>) -> Rc<RefCell<MemProvider>> {
    let asked = asked.clone();
    let mut m = MemProvider::default();
    m.sec = Some(Box::new(move |q: &Query| {
        asked.borrow_mut().push(q.clone());
        Ok(match q {
            Query::Profile { user } => {
                let mut p = UsrPrf::new(user, 40);
                p.grpprf = "CLERKS".into();
                p.supgrpprf = vec!["AUDIT".into()];
                p.text = "Anne".into();
                p.signon_invalid = 2;
                Answer::Profile { data: p.encode() }
            }
            Query::Check { auths, .. } => Answer::Yes {
                yes: auths.iter().all(|a| a == "*USE"),
            },
            Query::UserAuth { .. } => Answer::Auth {
                info: AuthInfo {
                    bits: Auth::CHANGE.0,
                    source: "*PRIVATE".into(),
                    autl: false,
                    list: String::new(),
                    owner: "QSECOFR".into(),
                    pgp: String::new(),
                },
            },
            Query::Objects { .. } => Answer::Objects {
                rows: vec![ObjRow {
                    name: "DATA".into(),
                    lib: "APPLIB".into(),
                    ty: "*DTAARA".into(),
                    owner: "QSECOFR".into(),
                    bits: Auth::USE.0,
                    owned: false,
                }],
            },
            Query::Holders { .. } => Answer::Users {
                rows: vec![
                    UserRow {
                        name: "QSECOFR".into(),
                        bits: Auth::ALL.0,
                        ..UserRow::default()
                    },
                    UserRow {
                        name: "*PUBLIC".into(),
                        bits: Auth::EXCLUDE.0,
                        ..UserRow::default()
                    },
                ],
            },
            Query::GetHandle { password, .. } if password == "WRONG" => {
                return Err(ApiFail::id("CPF22E2"));
            }
            Query::GetHandle { .. } => Answer::Bytes { data: vec![7; 12] },
            _ => Answer::Done,
        })
    }));
    Rc::new(RefCell::new(m))
}

fn run(text: &str, provider: &Rc<RefCell<MemProvider>>) -> Program {
    let mut p = Program::from_text(text).unwrap_or_else(|e| panic!("{e}"));
    p.job()
        .usrspcs
        .set_source(Box::new(MemUserSpaces::default()));
    sysapi::install(p.job(), provider.clone());
    p.call_entry().unwrap_or_else(|e| panic!("{e}"));
    p
}

fn bytes(p: &Program, name: &str) -> Vec<u8> {
    p.static_bytes(name).unwrap_or_else(|| panic!("no {name}"))
}

#[test]
fn profiles_and_authorities() {
    let asked = Rc::new(RefCell::new(Vec::new()));
    let prov = provider(&asked);
    let p = run(
        &format!(
            "
module SEC
extern sysapi QSYRUSRI($r: char(800), $rl: i32, $fmt: char(8), $u: char(10), $err: char(16))
extern sysapi QSYCUSRA($ind: char(1), $u: char(10), $o: char(20), $t: char(10), $a: char(20), $n: i32, $lvl: i32, $err: char(16))
extern sysapi QSYRUSRA($r: char(80), $rl: i32, $fmt: char(8), $u: char(10), $o: char(20), $t: char(10), $err: char(16))
extern sysapi QSYRAUTU($r: char(200), $rl: i32, $fmt: char(8), $fb: char(16), $o: char(20), $t: char(10), $err: char(16))
static $usri: char(800)
static $ulen: i32 inz(800)
static $f300: char(8) inz(\"USRI0300\")
static $anne: char(10) inz(\"ANNE\")
static $ind1: char(1)
static $ind2: char(1)
static $obj: char(20) inz(\"DATA      APPLIB\")
static $ty: char(10) inz(\"*DTAARA\")
static $use: char(20) inz(\"*USE      \")
static $chg: char(20) inz(\"*USE      *CHANGE\")
static $one: i32 inz(1)
static $two: i32 inz(2)
static $zero: i32 inz(0)
static $usra: char(80)
static $alen: i32 inz(80)
static $fa: char(8) inz(\"USRA0100\")
static $rtua: char(200)
static $rtlen: i32 inz(200)
static $frt: char(8) inz(\"RTUA0100\")
static $fb: char(16)
{ERRC}
proc SEC() entry {{
L0:
  sysapi QSYRUSRI($usri, $ulen, $f300, $anne, $err)
  sysapi QSYCUSRA($ind1, $anne, $obj, $ty, $use, $one, $zero, $err)
  sysapi QSYCUSRA($ind2, $anne, $obj, $ty, $chg, $two, $zero, $err)
  sysapi QSYRUSRA($usra, $alen, $fa, $anne, $obj, $ty, $err)
  sysapi QSYRAUTU($rtua, $rtlen, $frt, $fb, $obj, $ty, $err)
  ret
}}
"
        ),
        &prov,
    );
    assert_eq!(bin4(&bytes(&p, "err"), 4), 0);
    let u = bytes(&p, "usri");
    assert!(bin4(&u, 4) > 400, "bytes available");
    assert_eq!(chars(&u, 8, 10), "ANNE");
    assert_eq!(bin4(&u, 32), 2, "sign-on attempts not valid");
    assert_eq!(chars(&u, 36, 10), "*ENABLED");
    assert_eq!(chars(&u, 54, 1), "Y", "no password");
    assert_eq!(chars(&u, 94, 10), "*USER");
    assert_eq!(chars(&u, 119, 10), "CLERKS");
    assert_eq!(bin4(&u, 176), 1, "one supplemental group");
    assert_eq!(chars(&u, 180, 50), "Anne");
    let at = usize::try_from(bin4(&u, 172)).unwrap();
    assert_eq!(chars(&u, at, 10), "AUDIT");
    assert_eq!(chars(&bytes(&p, "ind1"), 0, 1), "Y");
    assert_eq!(chars(&bytes(&p, "ind2"), 0, 1), "N");
    let a = bytes(&p, "usra");
    assert_eq!(chars(&a, 8, 10), "*CHANGE");
    assert_eq!(chars(&a, 19, 11), "YNNNNNYYYYY");
    assert_eq!(chars(&a, 30, 16), "*PRIVATE");
    assert_eq!(chars(&a, 56, 10), "QSECOFR");
    let fb = bytes(&p, "fb");
    assert_eq!((bin4(&fb, 8), bin4(&fb, 12)), (2, 40), "two entries of 40");
    let r = bytes(&p, "rtua");
    assert_eq!(chars(&r, 0, 10), "QSECOFR");
    assert_eq!(chars(&r, 11, 10), "*ALL");
    assert_eq!(chars(&r, 40, 10), "*PUBLIC");
    assert_eq!(chars(&r, 51, 10), "*EXCLUDE");
    let asked = asked.borrow();
    assert_eq!(
        asked[1],
        Query::Check {
            user: "ANNE".into(),
            object: ("DATA".into(), "APPLIB".into(), "*DTAARA".into()),
            auths: vec!["*USE".into()],
        }
    );
}

#[test]
fn handles_and_lists() {
    let asked = Rc::new(RefCell::new(Vec::new()));
    let prov = provider(&asked);
    let mut p = run(
        &format!(
            "
module PH
extern sysapi QUSCRTUS($n: char(20), $a: char(10), $s: i32, $i: char(1), $au: char(10), $t: char(50), $r: char(10), $err: char(16))
extern sysapi QSYLOBJA($us: char(20), $fmt: char(8), $u: char(10), $t: char(10), $w: char(10), $h: char(20), $err: char(16))
extern sysapi QSYGETPH($u: char(10), $pw: char(10), $h: char(12), $err: char(16))
extern sysapi QWTSETP($h: char(12), $err: char(16))
extern sysapi QSYRLSPH($h: char(12), $err: char(16))
static $us: char(20) inz(\"AUTS      QGPL\")
static $attr: char(10) inz(\"USRSPC\")
static $size: i32 inz(16)
static $init: char(1) inz(x\"00\")
static $auth: char(10) inz(\"*ALL\")
static $text: char(50)
static $no: char(10) inz(\"*NO\")
static $fmt: char(8) inz(\"OBJA0100\")
static $cur: char(10) inz(\"*CURRENT\")
static $all: char(10) inz(\"*ALL\")
static $which: char(10) inz(\"*ALL\")
static $cont: char(20)
static $anne: char(10) inz(\"ANNE\")
static $pw: char(10) inz(\"SECRET77\")
static $bad: char(10) inz(\"WRONG\")
static $h: char(12)
static $h2: char(12)
{ERRC}
static $err2: char(16) inz(x\"00000010000000000000000000000000\")
proc PH() entry {{
L0:
  sysapi QUSCRTUS($us, $attr, $size, $init, $auth, $text, $no, $err)
  sysapi QSYLOBJA($us, $fmt, $cur, $all, $which, $cont, $err)
  sysapi QSYGETPH($anne, $pw, $h, $err)
  sysapi QWTSETP($h, $err)
  sysapi QSYRLSPH($h, $err)
  sysapi QSYGETPH($anne, $bad, $h2, $err2)
  ret
}}
"
        ),
        &prov,
    );
    assert_eq!(bin4(&bytes(&p, "err"), 4), 0);
    assert_eq!(bytes(&p, "h"), vec![7; 12]);
    let e2 = bytes(&p, "err2");
    assert_eq!(chars(&e2, 8, 7), "CPF22E2");
    let asked = asked.borrow().clone();
    assert!(asked.contains(&Query::SetHandle {
        handle: vec![7; 12]
    }));
    assert!(asked.contains(&Query::ReleaseHandle {
        handle: vec![7; 12]
    }));
    let us = p
        .job()
        .usrspcs
        .mapped("AUTS", "QGPL")
        .expect("the list space")
        .to_vec();
    assert_eq!(chars(&us, 72, 8), "OBJA0100");
    assert_eq!(chars(&us, 80, 10), "QSYLOBJA");
    let off = usize::try_from(bin4(&us, 124)).unwrap();
    assert_eq!(bin4(&us, 132), 1);
    assert_eq!(bin4(&us, 136), 62);
    let e = &us[off..off + 62];
    assert_eq!(chars(e, 0, 10), "DATA");
    assert_eq!(chars(e, 10, 10), "APPLIB");
    assert_eq!(chars(e, 20, 10), "*DTAARA");
    assert_eq!(chars(e, 30, 10), "QSECOFR");
    assert_eq!(chars(e, 40, 1), "N");
    assert_eq!(chars(e, 41, 10), "*USE");
}
