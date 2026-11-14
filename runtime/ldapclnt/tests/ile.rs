// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An IR program calling `QGLDCLNT` as an ILE program does: `ldap_init`,
//! `ldap_simple_bind_s`, `ldap_search_s` (whose later pointers travel on
//! the stack after a pad), `ldap_count_entries`, `ldap_first_entry`,
//! `ldap_get_dn`, `ldap_msgfree`, `ldap_unbind`; and `ber_printf` with a
//! variable list of an integer and a pointer.

mod common;

use std::fmt::Write as _;

use mi::FatPtr;
use mi::exec::Program;

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, c| {
        let _ = write!(s, "{c:02X}");
        s
    })
}

fn ebcdic(t: &str) -> (usize, String) {
    let mut b = ccsid::to_ebcdic(t).unwrap();
    b.push(0);
    (b.len(), hex(&b))
}

fn program(port: u16) -> String {
    let lit = |name: &str, t: &str| {
        let (n, h) = ebcdic(t);
        format!("static ${name}: bytes({n}) inz(x\"{h}\")\n")
    };
    let mut statics = String::new();
    statics += &lit("host", &format!("127.0.0.1:{port}"));
    statics += &lit("dn", "cn=admin,dc=example");
    statics += &lit("pw", "secret");
    statics += &lit("base", "dc=example");
    statics += &lit("filt", "(cn=*)");
    statics += &lit("fmt", "{is}");
    statics += &lit("txt", "hi");
    format!(
        "
module LDAPT
extern proc ldap_init(byval $h: ptr, byval $p: i32) -> ptr
extern proc ldap_simple_bind_s(byval $ld: ptr, byval $d: ptr, byval $pw: ptr) -> i32
extern proc ldap_search_s(byval $ld: ptr, byval $b: ptr, byval $s: i32, byval $f: ptr, byval $a: ptr, byval $o: i32, byval $r: ptr) -> i32
extern proc ldap_count_entries(byval $ld: ptr, byval $r: ptr) -> i32
extern proc ldap_first_entry(byval $ld: ptr, byval $r: ptr) -> ptr
extern proc ldap_get_dn(byval $ld: ptr, byval $e: ptr) -> ptr
extern proc ldap_msgfree(byval $m: ptr) -> i32
extern proc ldap_unbind(byval $ld: ptr) -> i32
extern proc ber_alloc_t(byval $o: i32) -> ptr
extern proc ber_printf(byval $b: ptr, byval $f: ptr, byval $i: i32, byval $s: ptr) -> i32
extern proc ber_flatten(byval $b: ptr, byval $o: ptr) -> i32
{statics}
static $res: ptr
static $dnp: ptr
static $bv: ptr
static $bind: i32 inz(99)
static $srch: i32 inz(99)
static $count: i32 inz(-9)
static $freed: i32 inz(-9)
static $printed: i32 inz(-9)
static $flat: i32 inz(-9)
proc LDAPT() entry {{
L0:
  %h = addr $host
  %ld = callb ldap_init(%h, 0)
  %d = addr $dn
  %pw = addr $pw
  %rc = callb ldap_simple_bind_s(%ld, %d, %pw)
  store $bind, %rc
  %b = addr $base
  %f = addr $filt
  %n = const.ptr *NULL
  %r = addr $res
  %rc2 = callb ldap_search_s(%ld, %b, 2, %f, %n, 0, %r)
  store $srch, %rc2
  %chain = load $res
  %c = callb ldap_count_entries(%ld, %chain)
  store $count, %c
  %e = callb ldap_first_entry(%ld, %chain)
  %dnv = callb ldap_get_dn(%ld, %e)
  store $dnp, %dnv
  %t = callb ldap_msgfree(%chain)
  store $freed, %t
  %u = callb ldap_unbind(%ld)
  %ber = callb ber_alloc_t(0)
  %fm = addr $fmt
  %tx = addr $txt
  %p = callb ber_printf(%ber, %fm, 5, %tx)
  store $printed, %p
  %o = addr $bv
  %fl = callb ber_flatten(%ber, %o)
  store $flat, %fl
  ret
}}
"
    )
}

fn bin4(p: &Program, name: &str) -> i32 {
    let b = p.static_bytes(name).unwrap();
    i32::from_be_bytes(b[..4].try_into().unwrap())
}

fn bytes_at(p: &Program, ptr: [u8; 16], n: usize) -> Vec<u8> {
    let (at, _) = p.job_ref().spaces.extent(FatPtr::from_bytes(&ptr)).unwrap();
    mi::storage::view(at, n).to_vec()
}

#[test]
fn an_ile_program_searches_through_the_bound_procedures() {
    let (addr, seen) = common::server(None, false);
    let mut p = Program::from_text(&program(addr.port())).unwrap_or_else(|e| panic!("{e}"));
    ldapclnt::install(p.job());
    p.call_entry().unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(bin4(&p, "bind"), 0);
    assert_eq!(bin4(&p, "srch"), 0);
    assert_eq!(bin4(&p, "count"), 2);
    assert_eq!(bin4(&p, "freed"), 0x64);
    let dnp: [u8; 16] = p.static_bytes("dnp").unwrap()[..16].try_into().unwrap();
    let dn = bytes_at(&p, dnp, 20);
    assert_eq!(ccsid::from_ebcdic(&dn[..19]), "cn=alice,dc=example");
    assert_eq!(dn[19], 0);
    // The variable list: 5, then the pointer to "hi" (EBCDIC).
    assert_eq!(bin4(&p, "printed"), 0);
    assert_eq!(bin4(&p, "flat"), 0);
    let bv: [u8; 16] = p.static_bytes("bv").unwrap()[..16].try_into().unwrap();
    let head = bytes_at(&p, bv, 32);
    assert_eq!(u32::from_be_bytes(head[..4].try_into().unwrap()), 9);
    let data = bytes_at(&p, head[16..32].try_into().unwrap(), 9);
    assert_eq!(data, [0x30, 0x07, 0x02, 0x01, 0x05, 0x04, 0x02, 0x88, 0x89]);
    std::thread::sleep(std::time::Duration::from_millis(100));
    let s = seen.lock().unwrap();
    assert_eq!(s[0], "bind v3 cn=admin,dc=example secret");
    assert!(
        s[1].starts_with("search dc=example Sub 0 0 (cn=*)"),
        "{}",
        s[1]
    );
    drop(s);
    ldapclnt::uninstall(p.job());
}
