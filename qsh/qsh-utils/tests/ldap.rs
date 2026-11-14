// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The LDAP tools against a fake directory on the loopback: searches
//! (text, LDIF, paging, sorting, persistent, from a file, `LDAP_BASEDN`),
//! LDIF updates, deletes, renames, password changes, extended
//! operations, compares, a diff of two servers, TLS, and the refusals.

#![allow(clippy::too_many_lines)]

mod common;
mod ldapfake;

use std::sync::Arc;

use common::{run_on, sys};
use ldapfake::{HOST, server, tls_pair};

fn tls_once() -> Arc<tls::rustls::ServerConfig> {
    let (s, c) = tls_pair();
    qsh_utils::net::ldap::set_tls(Some(Arc::new(move |ask: &qsh_utils::net::ldap::TlsAsk| {
        if ask.label.is_some() {
            Err("client certificates by label are not available".into())
        } else {
            Ok(c.clone())
        }
    })));
    s
}

fn at(addr: std::net::SocketAddr) -> String {
    format!("-h 127.0.0.1 -p {}", addr.port())
}

#[test]
fn ldapsearch_prints_entries_as_text_and_ldif() {
    let (addr, seen) = server(None, false);
    let s = sys();
    let h = at(addr);
    let o = run_on(
        &s,
        &format!("ldapsearch {h} -D cn=admin,dc=example -w secret -b dc=example 'cn=*' cn mail"),
        "",
    );
    assert_eq!(o.status, 0, "{}", o.err);
    assert_eq!(
        o.out,
        "cn=alice,dc=example\nobjectClass=person\ncn=alice\nsn=Ärger\nmail=alice@example.com\nmail=a@example.com\n\ncn=bob,dc=example\ncn=bob\nsn=B\n"
    );
    let o = run_on(
        &s,
        &format!(
            "LDAP_BASEDN=dc=example ldapsearch {h} -L -s one -z 5 -l 9 -a always objectclass=*"
        ),
        "",
    );
    assert_eq!(o.status, 0, "{}", o.err);
    assert!(
        o.out
            .starts_with("dn: cn=alice,dc=example\nobjectClass: person\ncn: alice\nsn:: "),
        "{}",
        o.out
    );
    assert!(o.out.contains("\n\ndn: cn=bob,dc=example\n"));
    let o = run_on(
        &s,
        &format!("ldapsearch {h} -A -F : -b dc=example cn=*"),
        "",
    );
    assert!(
        o.out.starts_with("cn=alice,dc=example\nobjectClass\ncn\n"),
        "{}",
        o.out
    );
    // Filters from a file, with the pattern.
    let o = run_on(
        &s,
        &format!("ldapsearch {h} -b dc=example -f - '(cn=%s)' cn"),
        "alice
bob
",
    );
    assert_eq!(o.status, 0, "{}", o.err);
    std::thread::sleep(std::time::Duration::from_millis(100));
    let l = seen.lock().unwrap();
    assert!(
        l.iter()
            .any(|x| x.starts_with("search dc=example Sub 0 0 (cn=*) cn,mail")),
        "{l:?}"
    );
    assert!(
        l.iter()
            .any(|x| x.starts_with("search dc=example One 5 9 (objectclass=*)")),
        "{l:?}"
    );
    assert!(
        l.iter()
            .any(|x| x.starts_with("search dc=example Sub 0 0 (cn=alice) cn")),
        "{l:?}"
    );
    assert!(
        l.iter()
            .any(|x| x.starts_with("search dc=example Sub 0 0 (cn=bob) cn")),
        "{l:?}"
    );
}

#[test]
fn ldapsearch_pages_sorts_and_persists() {
    let (addr, _) = server(None, false);
    let s = sys();
    let h = at(addr);
    let o = run_on(
        &s,
        &format!("ldapsearch {h} -q 1 -b dc=example cn=* cn"),
        "",
    );
    assert_eq!(o.status, 0, "{}", o.err);
    assert!(
        o.out.starts_with("cn=alice,dc=example\n") && o.out.contains("\n\ncn=bob,dc=example\n"),
        "{}",
        o.out
    );
    let o = run_on(
        &s,
        &format!("ldapsearch {h} -o -cn -b dc=example cn=* cn"),
        "",
    );
    assert!(o.out.starts_with("cn=bob,dc=example"), "{}", o.out);
    let o = run_on(
        &s,
        &format!("ldapsearch {h} -c modify -b dc=example cn=* cn"),
        "",
    );
    assert_eq!(o.status, 0, "{}", o.err);
    assert!(
        o.out.starts_with("change=modify\ncn=alice,dc=example\n"),
        "{}",
        o.out
    );
    let o = run_on(&s, &format!("ldapsearch {h} -b dc=refs cn=*"), "");
    assert_eq!(o.out, "reference=ldap://other:389/dc=refs\n");
    let o = run_on(&s, &format!("ldapsearch {h} -n -b dc=example cn=*"), "");
    assert!(o.out.contains("would search"), "{}", o.out);
    let o = run_on(&s, "ldapsearch -e", "");
    assert!(o.out.contains("QS/400 LDAP tools"));
}

#[test]
fn ldapadd_and_ldapmodify_send_ldif() {
    let (addr, seen) = server(None, false);
    let s = sys();
    let h = at(addr);
    let ldif = "dn: cn=carol,dc=example\nobjectClass: person\ncn: carol\nsn: C\n\n\
dn: cn=dave,dc=example\nchangetype: add\nobjectClass: person\ncn: dave\n\n\
dn: cn=alice,dc=example\nchangetype: modify\nreplace: sn\nsn: A\n-\ndelete: mail\n-\n\n\
dn: cn=bob,dc=example\nchangetype: modrdn\nnewrdn: cn=robert\ndeleteoldrdn: 1\n\n\
dn: cn=old,dc=example\nchangetype: delete\n";
    s.write_file("/home/QSHUSER/in.ldif", ldif.as_bytes());
    let o = run_on(
        &s,
        &format!("ldapadd {h} -D cn=admin,dc=example -w secret -f /home/QSHUSER/in.ldif"),
        "",
    );
    assert_eq!(o.status, 0, "{}", o.err);
    assert_eq!(
        o.out,
        "adding entry cn=carol,dc=example\nadding entry cn=dave,dc=example\nmodifying entry cn=alice,dc=example\nrenaming entry cn=bob,dc=example\ndeleting entry cn=old,dc=example\n"
    );
    // Without -a a content record adds values; -r replaces them.
    let o = run_on(
        &s,
        &format!("ldapmodify {h} -r"),
        "dn: cn=alice,dc=example\nmail: new@example.com\n",
    );
    assert_eq!(o.status, 0, "{}", o.err);
    // An error stops the run; -c goes on and -e keeps what failed.
    let bad = "dn: cn=missing,dc=example\nchangetype: delete\n\ndn: cn=x,dc=example\nchangetype: delete\n";
    let o = run_on(&s, &format!("ldapmodify {h}"), bad);
    assert_eq!(o.status, 32, "{}", o.err);
    assert!(
        o.err.contains("cn=missing,dc=example (line 1)"),
        "{}",
        o.err
    );
    let o = run_on(
        &s,
        &format!("ldapmodify {h} -c -e failed.ldif; cat failed.ldif"),
        bad,
    );
    assert!(
        o.out.contains("deleting entry cn=x,dc=example"),
        "{}",
        o.out
    );
    assert!(
        o.out
            .contains("version: 1\n\ndn: cn=missing,dc=example\nchangetype: delete"),
        "{}",
        o.out
    );
    // -n sends nothing.
    let o = run_on(
        &s,
        &format!("ldapmodify {h} -n"),
        "dn: cn=y,dc=example\nchangetype: delete\n",
    );
    assert!(o.out.contains("would send a delete for cn=y,dc=example"));
    // -t is refused.
    let o = run_on(&s, &format!("ldapadd {h} -t -f /home/QSHUSER/in.ldif"), "");
    assert_eq!(o.status, 92);
    std::thread::sleep(std::time::Duration::from_millis(100));
    let l = seen.lock().unwrap();
    assert!(
        l.iter()
            .any(|x| x == "add cn=carol,dc=example objectClass=person cn=carol sn=C ctl="),
        "{l:?}"
    );
    assert!(
        l.iter()
            .any(|x| x == "modify cn=alice,dc=example Replace:sn=1 Delete:mail=0 ctl="),
        "{l:?}"
    );
    assert!(
        l.iter()
            .any(|x| x == "modify cn=alice,dc=example Replace:mail=1 ctl="),
        "{l:?}"
    );
    assert!(
        l.iter()
            .any(|x| x.starts_with("moddn cn=bob,dc=example cn=robert true None")),
        "{l:?}"
    );
    assert!(!l.iter().any(|x| x.contains("cn=y,dc=example")));
}

#[test]
fn delete_modrdn_compare_passwords_and_exops() {
    let (addr, seen) = server(None, false);
    let s = sys();
    let h = at(addr);
    let o = run_on(
        &s,
        &format!("ldapdelete {h} -s cn=a,dc=example cn=b,dc=example"),
        "",
    );
    assert_eq!(o.status, 0, "{}", o.err);
    let o = run_on(
        &s,
        &format!("ldapdelete {h} cn=missing,dc=example cn=c,dc=example"),
        "",
    );
    assert_eq!(o.status, 32);
    let o = run_on(&s, &format!("ldapmodrdn {h} -r cn=a,dc=example cn=z"), "");
    assert_eq!(o.status, 0, "{}", o.err);
    let o = run_on(
        &s,
        &format!("ldapcompare {h} cn=alice,dc=example cn=alice"),
        "",
    );
    assert_eq!((o.status, o.out.as_str()), (0, "compare TRUE\n"));
    let o = run_on(
        &s,
        &format!("ldapcompare {h} cn=alice,dc=example cn=zed"),
        "",
    );
    assert_eq!((o.status, o.out.as_str()), (5, "compare FALSE\n"));
    let o = run_on(
        &s,
        &format!("ldapchangepwd {h} -D cn=admin,dc=example -w secret -n newpw"),
        "",
    );
    assert_eq!(o.status, 0, "{}", o.err);
    let o = run_on(
        &s,
        &format!("ldapchangepwd {h} -D cn=admin,dc=example -w wrong -n newpw"),
        "",
    );
    assert_eq!(o.status, 49);
    let o = run_on(
        &s,
        &format!("ldapexop {h} -D cn=admin,dc=example -w secret -op whoami"),
        "",
    );
    assert_eq!((o.status, o.out.as_str()), (0, "dn:cn=admin,dc=example\n"));
    let o = run_on(&s, &format!("ldapexop {h} -op whoami"), "");
    assert_eq!(o.out, "anonymous\n");
    let o = run_on(&s, &format!("ldapexop {h} -op passwd -a old uid=me"), "");
    assert_eq!((o.status, o.out.as_str()), (0, "New password: made\n"));
    let o = run_on(&s, &format!("ldapexop {h} -op onlineBackup"), "");
    assert_eq!(o.status, 92);
    assert!(o.err.contains("not supported"));
    // The password typed at the prompt.
    let o = run_on(
        &s,
        &format!("ldapexop {h} -D cn=admin,dc=example -w '?' -op whoami"),
        "secret\n",
    );
    assert_eq!(o.out, "dn:cn=admin,dc=example\n", "{}", o.err);
    std::thread::sleep(std::time::Duration::from_millis(100));
    let l = seen.lock().unwrap();
    assert!(
        l.iter()
            .any(|x| x == &format!("delete cn=a,dc=example ctl={}", ldap::control::TREE_DELETE)),
        "{l:?}"
    );
    assert!(
        l.iter()
            .any(|x| x == "modify cn=admin,dc=example Replace:userPassword=1 ctl="),
        "{l:?}"
    );
    assert!(
        l.iter()
            .any(|x| x.starts_with("passwd Some(\"uid=me\") None")),
        "{l:?}"
    );
    assert!(
        !l.iter().any(|x| x == "delete cn=c,dc=example ctl="),
        "stopped at the error"
    );
}

#[test]
fn ldapdiff_two_servers() {
    let (a, _) = server(None, false);
    let (b, seen) = server(None, false);
    let s = sys();
    // The same directory on both: no differences.
    let o = run_on(
        &s,
        &format!(
            "ldapdiff -b dc=example -sh 127.0.0.1 -sp {} -ch 127.0.0.1 -cp {}",
            a.port(),
            b.port()
        ),
        "",
    );
    assert_eq!(o.status, 0, "{}", o.err);
    assert_eq!(o.out, "version: 1\n");
    let o = run_on(&s, "ldapdiff -sh x -ch y -a -b dc=example", "");
    assert_eq!(o.status, 92);
    let o = run_on(&s, "ldapdiff -sh x", "");
    assert_eq!(o.status, 89);
    drop(seen);
}

#[test]
fn tls_and_refusals() {
    let scfg = tls_once();
    let (plain, seen) = server(Some(scfg.clone()), false);
    let (secure, _) = server(Some(scfg), true);
    let s = sys();
    let o = run_on(
        &s,
        &format!(
            "ldapsearch -h {HOST} -p {} -Y -b dc=example cn=* cn",
            plain.port()
        ),
        "",
    );
    assert_eq!(o.status, 0, "{}", o.err);
    assert!(o.out.starts_with("cn=alice,dc=example"));
    let o = run_on(
        &s,
        &format!(
            "ldapsearch -h {HOST} -p {} -Z -m EXTERNAL -b dc=example cn=* cn",
            secure.port()
        ),
        "",
    );
    assert_eq!(o.status, 0, "{}", o.err);
    let o = run_on(
        &s,
        &format!(
            "ldapsearch -h {HOST} -p {} -Z -N mycert cn=*",
            secure.port()
        ),
        "",
    );
    assert_eq!(o.status, 0x71, "{}", o.err);
    let o = run_on(
        &s,
        &format!(
            "ldapsearch -h {HOST} -p {} -Z -K /home/me.kdb cn=*",
            secure.port()
        ),
        "",
    );
    assert_eq!(o.status, 92);
    for (opt, word) in [
        ("-m DIGEST-MD5", "DIGEST-MD5"),
        ("-m GSSAPI", "GSSAPI"),
        ("-G realm", "DIGEST-MD5"),
        ("-U me", "EXTERNAL"),
        ("-d 1", "debug"),
        ("-k", "administration"),
        ("-j 5", "limit"),
    ] {
        let o = run_on(&s, &format!("ldapsearch {} {opt} cn=*", at(plain)), "");
        assert_eq!(o.status, 92, "{opt}: {}", o.err);
        assert!(o.err.contains(word), "{opt}: {}", o.err);
    }
    let o = run_on(&s, "ldapsearch -h 127.0.0.1 -p 1 cn=*", "");
    assert_eq!(o.status, 81);
    let o = run_on(&s, "ldapsearch -Q cn=*", "");
    assert_eq!(o.status, 89);
    assert!(o.err.contains("usage: ldapsearch"));
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .any(|x| x.starts_with("exop 1.3.6.1.4.1.1466.20037"))
    );
}
