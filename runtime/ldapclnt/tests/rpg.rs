// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An ILE RPG program that copies `QSYSINC/QRPGLESRC(LDAP)` compiles
//! with QS/400's compiler and searches the fake directory through its
//! prototypes: the copy member is valid RPG and its prototypes reach the
//! procedures.

mod common;

use rpgfront::{Includes, Member};

struct Qsysinc;

impl Includes for Qsysinc {
    fn member(&mut self, lib: &str, file: &str, name: &str) -> Option<Member> {
        let ok = (lib == "QSYSINC" || lib == "*LIBL") && file == "QRPGLESRC" && name == "LDAP";
        ok.then(|| {
            let text = ldapclnt::header::ldap_rpgle();
            Member::from_text("QSYSINC", "QRPGLESRC", "LDAP", &text)
        })
    }
}

fn program(port: u16) -> String {
    format!(
        "**FREE
ctl-opt dftactgrp(*no);
/copy QSYSINC/QRPGLESRC,LDAP
dcl-s ld pointer;
dcl-s res pointer;
dcl-s e pointer;
dcl-s dnp pointer;
dcl-ds out qualified;
  rc int(10);
  n int(10);
  dn char(40);
  txt char(40);
end-ds;
ld = ldap_init('127.0.0.1:{port}': 0);
out.rc = ldap_simple_bind_s(ld: 'cn=admin,dc=example': 'secret');
out.rc += ldap_search_s(ld: 'dc=example': LDAP_SCOPE_SUBTREE: '(cn=*)':
                        *null: 0: %addr(res));
out.n = ldap_count_entries(ld: res);
e = ldap_first_entry(ld: res);
dnp = ldap_get_dn(ld: e);
out.dn = %str(dnp);
ldap_memfree(dnp);
ldap_msgfree(res);
out.txt = %str(ldap_err2string(LDAP_NOT_SUPPORTED));
ldap_unbind(ld);
*inlr = *on;
"
    )
}

#[test]
fn an_rpg_program_uses_the_copy_member() {
    let (addr, seen) = common::server(None, false);
    let src = Member::from_text("QGPL", "QRPGLESRC", "LDAPT", &program(addr.port()));
    let opts = rpgc::Options {
        name: "LDAPT".into(),
        ..rpgc::Options::default()
    };
    let c = rpgc::compile(src, &mut Qsysinc, &mut rpgc::NoExtFiles, &opts);
    let severe: Vec<_> = c.diags.iter().filter(|d| d.severity >= 20).collect();
    assert!(c.created, "{severe:#?}");
    assert!(severe.is_empty(), "{severe:#?}");
    let mut p = mi::exec::Program::from_text(&c.qir).unwrap_or_else(|e| panic!("{e}"));
    qrpgrt::install(p.job());
    ldapclnt::install(p.job());
    p.call_entry().unwrap_or_else(|e| panic!("{e}"));
    let out = p.static_bytes("OUT").expect("the OUT data structure");
    assert_eq!(
        i32::from_be_bytes(out[0..4].try_into().unwrap()),
        0,
        "bind and search"
    );
    assert_eq!(i32::from_be_bytes(out[4..8].try_into().unwrap()), 2);
    assert_eq!(
        ccsid::from_ebcdic(&out[8..48]).trim_end(),
        "cn=alice,dc=example"
    );
    assert_eq!(ccsid::from_ebcdic(&out[48..88]).trim_end(), "Not supported");
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("search dc=example Sub"))
    );
}
