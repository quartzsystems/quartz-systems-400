// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client API through its functions' Rust entry points, against a
//! fake directory on the loopback: sessions, binds, every search form,
//! the asynchronous face, entries and values, updates, controls,
//! options, extended operations, TLS, DNs, URLs, code pages, LBER and
//! the refusals.

#![allow(clippy::too_many_lines, clippy::many_single_char_names, clippy::similar_names)]

mod common;

use std::rc::Rc;

use common::{T, server, tls_pair};
use ldapclnt::api::{
    bind, control, dn, entry, err, exop, lber, option, refused, result, search, session, update,
    url, xlate,
};
use ldapclnt::consts::*;

fn connect(t: &mut T, addr: std::net::SocketAddr) -> u128 {
    let h = t.s(&format!("127.0.0.1:{}", addr.port()));
    let ld = session::init(t.j(), h, 0);
    assert!(!T::null(ld));
    ld
}

fn bound(t: &mut T, addr: std::net::SocketAddr) -> u128 {
    let ld = connect(t, addr);
    let (d, p) = (t.s("cn=admin,dc=example"), t.s("secret"));
    assert_eq!(bind::simple_bind_s(t.j(), ld, d, p), 0);
    ld
}

#[test]
fn bind_search_and_walk_the_entries() {
    let (addr, seen) = server(None, false);
    let mut t = T::new();
    let ld = bound(&mut t, addr);
    let (base, filter) = (t.s("dc=example"), t.s("(objectClass=*)"));
    let res = t.buf(16);
    let rc = search::search_s(
        t.j(),
        ld,
        base,
        i64::from(LDAP_SCOPE_SUBTREE),
        filter,
        0,
        0,
        res,
    );
    assert_eq!(rc, 0);
    let chain = t.ptr_at(res);
    assert_eq!(entry::count_entries(t.j(), ld, chain), 2);
    assert_eq!(entry::count_messages(t.j(), ld, chain), 3);
    assert_eq!(
        result::msgtype(t.j(), chain),
        i64::from(LDAP_RES_SEARCH_ENTRY)
    );
    let e = entry::first_entry(t.j(), ld, chain);
    let dnp = entry::get_dn(t.j(), ld, e);
    assert_eq!(t.str_at(dnp), "cn=alice,dc=example");
    entry::memfree(t.j(), dnp);
    // Attributes through the BerElement cursor.
    let berp = t.buf(16);
    let mut names = Vec::new();
    let mut a = entry::first_attribute(t.j(), ld, e, berp);
    let ber = t.ptr_at(berp);
    while !T::null(a) {
        names.push(t.str_at(a));
        entry::memfree(t.j(), a);
        a = entry::next_attribute(t.j(), ld, e, ber);
    }
    entry::ber_free(t.j(), ber, 0);
    assert_eq!(names, ["objectClass", "cn", "sn", "mail"]);
    assert_eq!(entry::count_attributes(t.j(), ld, e), 4);
    // Values in the job's CCSID, and as they came.
    let mail = t.s("MAIL");
    let v = entry::get_values(t.j(), ld, e, mail);
    assert_eq!(t.strings_at(v), ["alice@example.com", "a@example.com"]);
    assert_eq!(entry::count_values(t.j(), v), 2);
    entry::value_free(t.j(), v);
    let sn = t.s("sn");
    let v = entry::get_values(t.j(), ld, e, sn);
    assert_eq!(t.strings_at(v), ["Ärger"]);
    let vl = entry::get_values_len(t.j(), ld, e, sn);
    let first = t.ptr_at(vl);
    assert_eq!(t.berval_at(first), "Ärger".as_bytes());
    entry::value_free_len(t.j(), vl);
    let none = t.s("telephoneNumber");
    assert!(T::null(entry::get_values(t.j(), ld, e, none)));
    let e2 = entry::next_entry(t.j(), ld, e);
    let dnp = entry::get_dn(t.j(), ld, e2);
    assert_eq!(t.str_at(dnp), "cn=bob,dc=example");
    assert!(T::null(entry::next_entry(t.j(), ld, e2)));
    // The result of the chain.
    let (code, m, msg) = (t.buf(4), t.buf(16), t.buf(16));
    assert_eq!(
        result::parse_result(t.j(), ld, chain, code, m, msg, 0, 0, 0),
        0
    );
    assert_eq!(t.int_at(code), 0);
    assert!(T::null(t.ptr_at(m)));
    assert_eq!(
        result::msgfree(t.j(), chain),
        i64::from(LDAP_RES_SEARCH_ENTRY)
    );
    assert_eq!(session::unbind(t.j(), ld), 0);
    std::thread::sleep(std::time::Duration::from_millis(100));
    let s = seen.lock().unwrap();
    assert_eq!(s[0], "bind v3 cn=admin,dc=example secret");
    assert!(
        s[1].starts_with("search dc=example Sub 0 0 (objectClass=*)"),
        "{}",
        s[1]
    );
    assert_eq!(s.last().unwrap(), "unbind");
}

#[test]
fn a_failed_bind_and_its_error() {
    let (addr, _) = server(None, false);
    let mut t = T::new();
    let ld = connect(&mut t, addr);
    let (d, p) = (t.s("cn=admin,dc=example"), t.s("wrong"));
    assert_eq!(
        bind::simple_bind_s(t.j(), ld, d, p),
        i64::from(LDAP_INVALID_CREDENTIALS)
    );
    let (m, s) = (t.buf(16), t.buf(16));
    assert_eq!(
        err::get_lderrno(t.j(), ld, m, s),
        i64::from(LDAP_INVALID_CREDENTIALS)
    );
    assert_eq!(t.str_at(t.ptr_at(s)), "wrong password");
    assert_eq!(
        err::get_errno(t.j(), ld),
        i64::from(LDAP_INVALID_CREDENTIALS)
    );
    let txt = err::err2string(t.j(), 49);
    assert_eq!(t.str_at(txt), ldap::ResultCode::InvalidCredentials.text());
    assert_eq!(err::err2string(t.j(), 49), txt, "kept");
    let pre = t.s("bind");
    err::perror(t.j(), ld, pre);
    assert!(t.job.console.last().unwrap().starts_with("bind: "));
    // The default DN and password stand in for null ones.
    let (d, p) = (t.s("cn=admin,dc=example"), t.s("secret"));
    bind::default_dn_set(t.j(), ld, d);
    bind::password_set(t.j(), ld, p);
    assert_eq!(bind::simple_bind_s(t.j(), ld, 0, 0), 0);
    let out = t.buf(16);
    bind::default_dn_get(t.j(), ld, out);
    assert_eq!(t.str_at(t.ptr_at(out)), "cn=admin,dc=example");
    // SASL: DIGEST-MD5 is refused here, EXTERNAL goes to the server.
    let (mech, idp) = (t.s("DIGEST-MD5"), t.buf(4));
    assert_eq!(
        bind::sasl_bind(t.j(), ld, 0, mech, 0, 0, 0, idp),
        i64::from(LDAP_NOT_SUPPORTED)
    );
    let ext = t.s("EXTERNAL");
    assert_eq!(
        bind::sasl_bind_s(t.j(), ld, 0, ext, 0, 0, 0, 0),
        i64::from(LDAP_INVALID_CREDENTIALS)
    );
    session::unbind(t.j(), ld);
}

#[test]
fn the_asynchronous_face() {
    let (addr, seen) = server(None, false);
    let mut t = T::new();
    let ld = bound(&mut t, addr);
    let base = t.s("dc=example");
    let id = search::search(t.j(), ld, base, i64::from(LDAP_SCOPE_ONELEVEL), 0, 0, 1);
    assert!(id > 0);
    // One message at a time.
    let res = t.buf(16);
    let mut types = Vec::new();
    loop {
        let ty = result::result(t.j(), ld, id, i64::from(LDAP_MSG_ONE), 0, res);
        types.push(ty);
        let m = t.ptr_at(res);
        assert_eq!(result::msgid(t.j(), m), id);
        result::msgfree(t.j(), m);
        if ty == i64::from(LDAP_RES_SEARCH_RESULT) {
            break;
        }
    }
    assert_eq!(types, [0x64, 0x64, 0x65]);
    // All at once, any message ID.
    let id = search::search(t.j(), ld, base, 0, 0, 0, 0);
    let ty = result::result(
        t.j(),
        ld,
        i64::from(LDAP_RES_ANY),
        i64::from(LDAP_MSG_ALL),
        0,
        res,
    );
    assert_eq!(ty, i64::from(LDAP_RES_SEARCH_RESULT));
    let m = t.ptr_at(res);
    assert_eq!(result::msgid(t.j(), m), id);
    assert_eq!(entry::count_entries(t.j(), ld, m), 2);
    assert_eq!(result::result2error(t.j(), ld, m, 1), 0);
    // A search that never ends: a timeout, then abandoned.
    let slow = t.s("dc=slow");
    let id = search::search(t.j(), ld, slow, 0, 0, 0, 0);
    let tv = t.timeval(0, 200_000);
    assert_eq!(result::result(t.j(), ld, id, 1, tv, res), 0);
    assert_eq!(update::abandon(t.j(), ld, id), 0);
    let st = search::search_st(t.j(), ld, slow, 0, 0, 0, 0, tv, res);
    assert_eq!(st, i64::from(LDAP_TIMEOUT));
    // References come back to the program.
    let refs = t.s("dc=refs");
    assert_eq!(search::search_s(t.j(), ld, refs, 0, 0, 0, 0, res), 0);
    let m = t.ptr_at(res);
    assert_eq!(entry::count_references(t.j(), ld, m), 1);
    let r = entry::first_reference(t.j(), ld, m);
    let rp = t.buf(16);
    assert_eq!(result::parse_reference(t.j(), ld, r, rp, 0, 0), 0);
    assert_eq!(t.strings_at(t.ptr_at(rp)), ["ldap://other:389/dc=refs"]);
    // A bad filter and a bad scope.
    let bad = t.s("(cn=");
    assert_eq!(
        search::search_s(t.j(), ld, base, 0, bad, 0, 0, res),
        i64::from(LDAP_FILTER_ERROR)
    );
    assert_eq!(
        search::search_s(t.j(), ld, base, 7, 0, 0, 0, res),
        i64::from(LDAP_PARAM_ERROR)
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(
        seen.lock()
            .unwrap()
            .iter()
            .any(|l| l.starts_with("abandon "))
    );
    session::unbind(t.j(), ld);
}

#[test]
fn updates_in_every_form() {
    let (addr, seen) = server(None, false);
    let mut t = T::new();
    let ld = bound(&mut t, addr);
    let dnp = t.s("cn=carol,dc=example");
    let m1 = t.modp(LDAP_MOD_ADD, "objectClass", &["person"], false);
    let m2 = t.modp(LDAP_MOD_ADD, "cn", &["carol"], false);
    let m3 = t.modp(LDAP_MOD_ADD, "jpegPhoto", &["\u{1}bin"], true);
    let mods = t.ptrs(&[m1, m2, m3]);
    assert_eq!(update::add_s(t.j(), ld, dnp, mods), 0);
    let r1 = t.modp(LDAP_MOD_REPLACE, "sn", &["C"], false);
    let r2 = t.modp(LDAP_MOD_DELETE, "mail", &[], false);
    let rm = t.ptrs(&[r1, r2]);
    assert_eq!(update::modify_ext_s(t.j(), ld, dnp, rm, 0, 0), 0);
    // A server control goes with the request; a critical client one is refused.
    let oid = t.s(ldap::control::TREE_DELETE);
    let out = t.buf(16);
    let ctl_list = t.buf(16);
    assert_eq!(control::add_control(t.j(), oid, 0, 0, 1, ctl_list), 0);
    let ctls = t.ptr_at(ctl_list);
    assert_eq!(update::delete_ext_s(t.j(), ld, dnp, ctls, 0), 0);
    assert_eq!(
        update::delete_ext_s(t.j(), ld, dnp, 0, ctls),
        i64::from(LDAP_NOT_SUPPORTED)
    );
    let _ = out;
    let missing = t.s("cn=missing,dc=example");
    assert_eq!(
        update::delete_s(t.j(), ld, missing),
        i64::from(LDAP_NO_SUCH_OBJECT)
    );
    let (m, s) = (t.buf(16), t.buf(16));
    err::get_lderrno(t.j(), ld, m, s);
    assert_eq!(t.str_at(t.ptr_at(m)), "dc=example");
    let rdn = t.s("cn=carla");
    let sup = t.s("ou=people,dc=example");
    assert_eq!(update::rename_s(t.j(), ld, dnp, rdn, sup, 1, 0, 0), 0);
    assert_eq!(update::modrdn_s(t.j(), ld, dnp, rdn, 0), 0);
    let (alice, cn, v) = (t.s("cn=alice,dc=example"), t.s("cn"), t.s("alice"));
    assert_eq!(
        update::compare_s(t.j(), ld, alice, cn, v),
        i64::from(LDAP_COMPARE_TRUE)
    );
    let bv = t.berval(b"bob");
    assert_eq!(
        update::compare_ext_s(t.j(), ld, alice, cn, bv, 0, 0),
        i64::from(LDAP_COMPARE_FALSE)
    );
    // The asynchronous forms answer through ldap_result.
    let idp = t.buf(4);
    assert_eq!(update::add_ext(t.j(), ld, dnp, mods, 0, 0, idp), 0);
    let id = t.int_at(idp);
    let res = t.buf(16);
    assert_eq!(
        result::result(t.j(), ld, i64::from(id), 1, 0, res),
        i64::from(LDAP_RES_ADD)
    );
    let mid = update::modify(t.j(), ld, dnp, rm);
    assert_eq!(
        result::result(t.j(), ld, mid, 1, 0, res),
        i64::from(LDAP_RES_MODIFY)
    );
    let did = update::delete(t.j(), ld, dnp);
    assert_eq!(
        result::result(t.j(), ld, did, 1, 0, res),
        i64::from(LDAP_RES_DELETE)
    );
    let cid = update::compare(t.j(), ld, alice, cn, v);
    assert_eq!(
        result::result(t.j(), ld, cid, 1, 0, res),
        i64::from(LDAP_RES_COMPARE)
    );
    let p_res = t.ptr_at(res);
    assert_eq!(
        result::result2error(t.j(), ld, p_res, 1),
        i64::from(LDAP_COMPARE_TRUE)
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    let s = seen.lock().unwrap();
    assert!(
        s.contains(
            &"add cn=carol,dc=example objectClass=person cn=carol jpegPhoto=\u{1}bin ctl="
                .to_string()
        ),
        "{s:?}"
    );
    assert!(
        s.iter()
            .any(|l| l == "modify cn=carol,dc=example Replace:sn=1 Delete:mail=0 ctl=")
    );
    assert!(s.iter().any(|l| l
        == &format!(
            "delete cn=carol,dc=example ctl={}",
            ldap::control::TREE_DELETE
        )));
    assert!(s.iter().any(|l| {
        l.starts_with("moddn cn=carol,dc=example cn=carla true Some(\"ou=people,dc=example\")")
    }));
    session::unbind(t.j(), ld);
}

#[test]
fn paging_sorting_and_control_lists() {
    let (addr, _) = server(None, false);
    let mut t = T::new();
    let ld = bound(&mut t, addr);
    let base = t.s("dc=example");
    let res = t.buf(16);
    // Page one of size 1, then the cookie for page two.
    let out = t.buf(16);
    assert_eq!(control::create_page_control(t.j(), ld, 1, 0, 1, out), 0);
    let page = t.ptr_at(out);
    let list = t.ptrs(&[page]);
    let rc = search::search_ext_s(t.j(), ld, base, 2, 0, 0, 0, list, 0, 0, -1, res);
    assert_eq!(rc, 0);
    let chain = t.ptr_at(res);
    assert_eq!(entry::count_entries(t.j(), ld, chain), 1);
    let (code, sc) = (t.buf(4), t.buf(16));
    result::parse_result(t.j(), ld, chain, code, 0, 0, 0, sc, 0);
    let rctl = t.ptr_at(sc);
    let (total, cookie) = (t.buf(4), t.buf(16));
    assert_eq!(
        control::parse_page_control(t.j(), ld, rctl, total, cookie),
        0
    );
    let ck = t.ptr_at(cookie);
    assert_eq!(t.berval_at(ck), b"1");
    control::controls_free(t.j(), rctl);
    control::control_free(t.j(), page);
    assert_eq!(control::create_page_control(t.j(), ld, 1, ck, 1, out), 0);
    let list = t.ptrs(&[t.ptr_at(out)]);
    assert_eq!(
        search::search_ext_s(t.j(), ld, base, 2, 0, 0, 0, list, 0, 0, 0, res),
        0
    );
    let chain = t.ptr_at(res);
    let e = entry::first_entry(t.j(), ld, chain);
    let r_ = entry::get_dn(t.j(), ld, e);
    assert_eq!(t.str_at(r_), "cn=bob,dc=example");
    // Sorting, reversed.
    let (kl, keys) = (t.buf(16), t.s("-cn:caseIgnoreOrderingMatch sn"));
    assert_eq!(control::create_sort_keylist(t.j(), kl, keys), 0);
    let keylist = t.ptr_at(kl);
    assert_eq!(control::create_sort_control(t.j(), ld, keylist, 0, out), 0);
    control::free_sort_keylist(t.j(), keylist);
    let list = t.ptrs(&[t.ptr_at(out)]);
    assert_eq!(
        search::search_ext_s(t.j(), ld, base, 2, 0, 0, 0, list, 0, 0, 0, res),
        0
    );
    let chain = t.ptr_at(res);
    let e = entry::first_entry(t.j(), ld, chain);
    let r_ = entry::get_dn(t.j(), ld, e);
    assert_eq!(t.str_at(r_), "cn=bob,dc=example");
    result::parse_result(t.j(), ld, chain, code, 0, 0, 0, sc, 0);
    let (rc, attr) = (t.buf(4), t.buf(16));
    let p_sc = t.ptr_at(sc);
    assert_eq!(control::parse_sort_control(t.j(), ld, p_sc, rc, attr), 0);
    assert_eq!(t.int_at(rc), 0);
    let p_sc = t.ptr_at(sc);
    assert_eq!(
        control::parse_page_control(t.j(), ld, p_sc, rc, attr),
        i64::from(LDAP_CONTROL_NOT_FOUND)
    );
    // Persistent search: the entry carries an entry change control.
    assert_eq!(
        control::create_persistentsearch_control(t.j(), ld, 15, 1, 1, 1, out),
        0
    );
    let list = t.ptrs(&[t.ptr_at(out)]);
    let idp = t.buf(4);
    assert_eq!(
        search::search_ext(t.j(), ld, base, 2, 0, 0, 0, list, 0, 0, 0, idp),
        0
    );
    let id = t.int_at(idp);
    assert_eq!(result::result(t.j(), ld, i64::from(id), 0, 0, res), 0x64);
    let ectl = t.buf(16);
    let p_res = t.ptr_at(res);
    assert_eq!(entry::get_entry_controls(t.j(), ld, p_res, ectl), 0);
    let (ty, prev, has, num) = (t.buf(4), t.buf(16), t.buf(4), t.buf(4));
    let p_ectl = t.ptr_at(ectl);
    assert_eq!(
        control::parse_entrychange_control(t.j(), ld, p_ectl, ty, prev, has, num),
        0
    );
    assert_eq!((t.int_at(ty), t.int_at(has), t.int_at(num)), (4, 1, 7));
    update::abandon(t.j(), ld, i64::from(id));
    // Lists: add, insert, copy, merge, remove.
    let lp = t.buf(16);
    let (o1, o2) = (t.s("1.2.3"), t.s("1.2.4"));
    let val = t.bytes(b"xy");
    control::add_control(t.j(), o1, 2, val, 0, lp);
    control::add_control(t.j(), o2, 0, 0, 1, lp);
    let l = t.ptr_at(lp);
    let copy = t.buf(16);
    assert_eq!(control::copy_controls(t.j(), copy, l), 0);
    let merged = t.buf(16);
    let p_copy = t.ptr_at(copy);
    assert_eq!(control::merge_control_lists(t.j(), l, p_copy, merged), 0);
    assert!(!T::null(t.ptr_at(t.ptr_at(merged))));
    let first = t.ptr_at(l);
    assert_eq!(control::remove_control(t.j(), first, lp, 0), 0);
    assert_eq!(
        control::remove_control(t.j(), first, lp, 0),
        i64::from(LDAP_CONTROL_NOT_FOUND)
    );
    control::control_free(t.j(), first);
    let proxy = t.s("cn=bob,dc=example");
    assert_eq!(
        control::create_proxyauth_control(t.j(), ld, proxy, 1, out),
        0
    );
    let c = t.ptr_at(out);
    assert_eq!(t.str_at(t.ptr_at(c)), ldap::control::PROXIED_AUTHZ);
    let f = t.s("(cn=alice)");
    assert_eq!(control::create_assertion_control(t.j(), ld, f, 1, out), 0);
    let bad = t.s("(cn=");
    assert_eq!(
        control::create_assertion_control(t.j(), ld, bad, 1, out),
        i64::from(LDAP_FILTER_ERROR)
    );
    session::unbind(t.j(), ld);
}

#[test]
fn options() {
    let (addr, seen) = server(None, false);
    let mut t = T::new();
    // Defaults through a null handle reach new handles.
    let n = t.int(5);
    assert_eq!(
        option::set_option(t.j(), 0, i64::from(LDAP_OPT_SIZELIMIT), n),
        0
    );
    let ld = bound(&mut t, addr);
    let out = t.buf(16);
    assert_eq!(
        option::get_option(t.j(), ld, i64::from(LDAP_OPT_SIZELIMIT), out),
        0
    );
    assert_eq!(t.int_at(out), 5);
    let v2 = t.int(2);
    assert_eq!(
        option::set_option(t.j(), ld, i64::from(LDAP_OPT_PROTOCOL_VERSION), v2),
        0
    );
    let v9 = t.int(9);
    assert_eq!(
        option::set_option(t.j(), ld, i64::from(LDAP_OPT_PROTOCOL_VERSION), v9),
        i64::from(LDAP_PARAM_ERROR)
    );
    let tl = t.int(30);
    option::set_option(t.j(), ld, i64::from(LDAP_OPT_TIMELIMIT), tl);
    let alw = t.int(3);
    option::set_option(t.j(), ld, i64::from(LDAP_OPT_DEREF), alw);
    // LDAP_OPT_ON as a pointer value that addresses nothing.
    let on = u128::from_le_bytes(mi::FatPtr::from_halves(1, 0).to_bytes());
    assert_eq!(
        option::set_option(t.j(), ld, i64::from(LDAP_OPT_REFERRALS), on),
        0
    );
    option::get_option(t.j(), ld, i64::from(LDAP_OPT_REFERRALS), out);
    assert_eq!(t.int_at(out), 1);
    option::set_option(t.j(), ld, i64::from(LDAP_OPT_REFERRALS), 0);
    // What is not built is refused.
    for o in [
        LDAP_OPT_DEBUG,
        LDAP_OPT_REBIND_FN,
        LDAP_OPT_SSL_CIPHER,
        LDAP_OPT_API_INFO,
        0x7777,
    ] {
        assert_eq!(
            option::set_option(t.j(), ld, i64::from(o), n),
            i64::from(LDAP_NOT_SUPPORTED),
            "{o}"
        );
    }
    assert_eq!(
        option::get_option(t.j(), ld, i64::from(LDAP_OPT_DESC), out),
        i64::from(LDAP_NOT_SUPPORTED)
    );
    let off = t.int(0);
    assert_eq!(
        option::set_option(t.j(), ld, i64::from(LDAP_OPT_RESTART), off),
        i64::from(LDAP_NOT_SUPPORTED)
    );
    assert_eq!(
        option::get_option(t.j(), ld, i64::from(LDAP_OPT_HOST_NAME), out),
        0
    );
    assert_eq!(t.str_at(t.ptr_at(out)), "127.0.0.1");
    // UTF-8 at the interface.
    assert_eq!(
        option::set_option(t.j(), ld, i64::from(LDAP_OPT_UTF8_IO), on),
        0
    );
    let (base, filter) = (t.u("dc=example"), t.u("(sn=Ärger)"));
    let res = t.buf(16);
    assert_eq!(search::search_s(t.j(), ld, base, 2, filter, 0, 0, res), 0);
    let p_res = t.ptr_at(res);
    let e = entry::first_entry(t.j(), ld, p_res);
    let sn = t.u("sn");
    let v = entry::get_values(t.j(), ld, e, sn);
    let first = t.ptr_at(v);
    assert_eq!(t.raw_str(first), "Ärger".as_bytes());
    // Server controls set on the handle go with every request.
    let tree = t.u(ldap::control::TREE_DELETE);
    let lp = t.buf(16);
    control::add_control(t.j(), tree, 0, 0, 0, lp);
    let p_lp = t.ptr_at(lp);
    assert_eq!(
        option::set_option(t.j(), ld, i64::from(LDAP_OPT_SERVER_CONTROLS), p_lp),
        0
    );
    let d = t.u("cn=x,dc=example");
    assert_eq!(update::delete_s(t.j(), ld, d), 0);
    assert_eq!(
        option::get_option(t.j(), ld, i64::from(LDAP_OPT_SERVER_CONTROLS), out),
        0
    );
    assert!(!T::null(t.ptr_at(out)));
    // The error number and string.
    let en = t.int(LDAP_BUSY);
    option::set_option(t.j(), ld, i64::from(LDAP_OPT_ERROR_NUMBER), en);
    assert_eq!(err::get_errno(t.j(), ld), i64::from(LDAP_BUSY));
    session::unbind(t.j(), ld);
    std::thread::sleep(std::time::Duration::from_millis(100));
    let s = seen.lock().unwrap();
    assert!(
        s.iter()
            .any(|l| l.starts_with("search dc=example Sub 5 30 (sn=Ärger)")),
        "{s:?}"
    );
    assert!(
        s.iter()
            .any(|l| l.ends_with(&format!("ctl={}", ldap::control::TREE_DELETE)))
    );
}

#[test]
fn extended_operations() {
    let (addr, _) = server(None, false);
    let mut t = T::new();
    let ld = bound(&mut t, addr);
    let oid = t.s(ldap::ext::WHO_AM_I);
    let (ro, rd) = (t.buf(16), t.buf(16));
    assert_eq!(
        exop::extended_operation_s(t.j(), ld, oid, 0, 0, 0, ro, rd),
        0
    );
    assert_eq!(t.berval_at(t.ptr_at(rd)), b"dn:cn=admin,dc=example");
    let idp = t.buf(4);
    let bogus = t.s("1.2.3.4");
    assert_eq!(exop::extended_operation(t.j(), ld, bogus, 0, 0, 0, idp), 0);
    let res = t.buf(16);
    let id = t.int_at(idp);
    assert_eq!(
        result::result(t.j(), ld, i64::from(id), 1, 0, res),
        i64::from(LDAP_RES_EXTENDED)
    );
    let (o, d) = (t.buf(16), t.buf(16));
    let p_res = t.ptr_at(res);
    assert_eq!(result::parse_extended_result(t.j(), ld, p_res, o, d, 0), 0);
    let p_res = t.ptr_at(res);
    assert_eq!(
        result::result2error(t.j(), ld, p_res, 1),
        i64::from(LDAP_PROTOCOL_ERROR)
    );
    session::unbind(t.j(), ld);
}

#[test]
fn tls_ldaps_starttls_and_application_ids() {
    let (scfg, ccfg) = tls_pair();
    let c2 = ccfg.clone();
    ldapclnt::set_tls_source(Some(Rc::new(move |w: &ldapclnt::TlsWant| match w {
        ldapclnt::TlsWant::System { .. } => Ok(c2.clone()),
        ldapclnt::TlsWant::App(id) if id == "QIBM_GLD_DIRSRV_CLIENT" => Ok(c2.clone()),
        other => Err(format!("{other:?} is not known")),
    })));
    // StartTLS on a clear connection, then a bind over it.
    let (addr, seen) = server(Some(scfg.clone()), false);
    let mut t = T::new();
    let h = t.s(&format!("{}:{}", common::HOST, addr.port()));
    let ld = session::init(t.j(), h, 0);
    assert_eq!(session::start_tls_s(t.j(), ld, 0), 0);
    let ext = t.s("EXTERNAL");
    assert_eq!(bind::sasl_bind_s(t.j(), ld, 0, ext, 0, 0, 0, 0), 0);
    assert_eq!(session::stop_tls_s(t.j(), ld), 0);
    let (d, p) = (t.s("cn=admin,dc=example"), t.s("secret"));
    assert_eq!(bind::simple_bind_s(t.j(), ld, d, p), 0);
    session::unbind(t.j(), ld);
    // LDAPS without an environment, then with the *SYSTEM store.
    let (saddr, _) = server(Some(scfg), true);
    let h = t.s(&format!("{}:{}", common::HOST, saddr.port()));
    assert!(T::null(session::ssl_init(t.j(), h, 0, 0)));
    let reason = t.buf(4);
    assert_eq!(session::ssl_client_init(t.j(), 0, 0, 0, reason), 0);
    assert_eq!(
        session::ssl_client_init(t.j(), 0, 0, 0, reason),
        i64::from(LDAP_SSL_ALREADY_INITIALIZED)
    );
    let ld = session::ssl_init(t.j(), h, 0, 0);
    assert_eq!(bind::simple_bind_s(t.j(), ld, d, p), 0);
    session::unbind(t.j(), ld);
    // An application ID.
    let app = t.s("QIBM_GLD_DIRSRV_CLIENT");
    let ld = session::app_ssl_init(t.j(), h, 0, app);
    assert_eq!(bind::simple_bind_s(t.j(), ld, d, p), 0);
    session::unbind(t.j(), ld);
    let other = t.s("NOT_REGISTERED");
    let ld = session::app_ssl_init(t.j(), h, 0, other);
    assert_eq!(
        bind::simple_bind_s(t.j(), ld, d, p),
        i64::from(LDAP_SSL_INITIALIZE_FAILED)
    );
    let kr = t.s("/home/me/key.kdb");
    let ld2 = connect(&mut t, saddr);
    assert_eq!(session::ssl_start(t.j(), ld2, kr, 0, 0), 0);
    assert_eq!(
        bind::simple_bind_s(t.j(), ld2, d, p),
        i64::from(LDAP_SSL_INITIALIZE_FAILED)
    );
    std::thread::sleep(std::time::Duration::from_millis(100));
    let s = seen.lock().unwrap();
    assert!(s.iter().any(|l| l == "sasl EXTERNAL"));
    ldapclnt::set_tls_source(None);
}

#[test]
fn dns_urls_and_code_pages() {
    let mut t = T::new();
    let d = t.s("cn=Joe Smith,ou=Sales,dc=example,dc=com");
    let v = dn::explode_dn(t.j(), d, 0);
    assert_eq!(
        t.strings_at(v),
        ["cn=Joe Smith", "ou=Sales", "dc=example", "dc=com"]
    );
    let v = dn::explode_dn(t.j(), d, 1);
    assert_eq!(t.strings_at(v), ["Joe Smith", "Sales", "example", "com"]);
    let r = t.s("cn=a+uid=b");
    let r_ = dn::explode_rdn(t.j(), r, 0);
    assert_eq!(t.strings_at(r_), ["cn=a", "uid=b"]);
    let r_ = dn::dn2ufn(t.j(), d);
    assert_eq!(t.str_at(r_), "Joe Smith, Sales, example, com");
    let bad = t.s("not a dn");
    assert!(T::null(dn::explode_dn(t.j(), bad, 0)));
    let dns = t.s("a.b.example.com");
    let r_ = dn::explode_dns(t.j(), dns);
    assert_eq!(t.strings_at(r_), ["a", "b", "example", "com"]);
    let du = t.u("cn=Äh,dc=x");
    let v = dn::explode_dn_utf8(t.j(), du, 1);
    assert_eq!(t.raw_str(t.ptr_at(v)), "Äh".as_bytes());
    // URLs.
    let u = t.s("ldap://host:1389/dc=example?cn,sn?sub?(cn=a*)");
    assert_eq!(url::is_ldap_url(t.j(), u), 1);
    let no = t.s("http://x/");
    assert_eq!(url::is_ldap_url(t.j(), no), 0);
    let out = t.buf(16);
    assert_eq!(url::url_parse(t.j(), u, out), 0);
    let desc = t.ptr_at(out);
    let b = t.view(desc, 112).to_vec();
    let at = |o: usize| u128::from_le_bytes(b[o..o + 16].try_into().unwrap());
    assert_eq!(t.str_at(at(0)), "host");
    assert_eq!(i32::from_be_bytes(b[16..20].try_into().unwrap()), 1389);
    assert_eq!(t.str_at(at(32)), "dc=example");
    assert_eq!(t.strings_at(at(48)), ["cn", "sn"]);
    assert_eq!(
        i32::from_be_bytes(b[64..68].try_into().unwrap()),
        LDAP_SCOPE_SUBTREE
    );
    assert_eq!(t.str_at(at(80)), "(cn=a*)");
    url::free_urldesc(t.j(), desc);
    // Code pages.
    assert_eq!(xlate::get_codepage(t.j()), 37);
    let src = t.bytes(&ccsid::to_ebcdic("Hé").unwrap());
    let (il, ob, ol) = (t.int(2), t.buf(8), t.int(8));
    assert_eq!(xlate::local_to_utf8(t.j(), src, il, ob, ol), 0);
    assert_eq!(t.int_at(ol), 3);
    assert_eq!(t.view(ob, 3), "Hé".as_bytes());
    let small = t.int(1);
    assert_eq!(
        xlate::local_to_unicode(t.j(), src, il, ob, small),
        i64::from(LDAP_NO_MEMORY)
    );
    assert_eq!(t.int_at(small), 4);
    let cs = t.s("IBM-273");
    assert_eq!(xlate::set_charset(t.j(), cs), 0);
    assert_eq!(xlate::get_codepage(t.j()), 273);
    assert_eq!(xlate::set_codepage(t.j(), 1208), 0);
    let u8s = t.u("é");
    assert_eq!(xlate::char_size(t.j(), u8s), 2);
    assert_eq!(
        xlate::set_codepage(t.j(), 99999),
        i64::from(LDAP_PARAM_ERROR)
    );
    // The version and the refusals.
    let ver = t.buf(12);
    assert!(session::version(t.j(), ver) >= 110);
    assert_eq!(
        t.int_at(ver),
        i32::try_from(session::version(t.j(), ver)).unwrap()
    );
    assert_eq!(refused::int(t.j()), i64::from(LDAP_NOT_SUPPORTED));
    assert!(T::null(refused::pointer(t.j())));
    assert_eq!(err::get_errno(t.j(), 0), i64::from(LDAP_NOT_SUPPORTED));
}

#[test]
fn lber_round_trip() {
    use lber::Arg;
    let mut t = T::new();
    let ber = lber::alloc_t(t.j(), 1);
    let s = t.bytes(b"text\0");
    let o = t.bytes(b"abc");
    let bv = t.berval(b"xyz");
    let v1 = t.bytes(b"one\0");
    let v2 = t.bytes(b"two\0");
    let arr = t.ptrs(&[v1, v2]);
    let args = [
        Arg::Int(7),
        Arg::Ptr(s),
        Arg::Int(1),
        Arg::Ptr(o),
        Arg::Int(3),
        Arg::Ptr(bv),
        Arg::Int(0x83),
        Arg::Int(9),
        Arg::Ptr(arr),
    ];
    assert_eq!(lber::printf(t.j(), ber, "{isbo}{O}{te}{v}", &args), 0);
    let out = t.buf(16);
    assert_eq!(lber::flatten(t.j(), ber, out), 0);
    let flat = t.ptr_at(out);
    let bytes = t.berval_at(flat);
    assert_eq!(&bytes[..2], [0x30, 0x11]);
    // Decode it back.
    let rd = lber::init(t.j(), flat);
    let (n, sp, b, op) = (t.buf(4), t.buf(16), t.buf(4), t.buf(16));
    let tag = lber::scanf(
        t.j(),
        rd,
        "{iabO}",
        &[Arg::Ptr(n), Arg::Ptr(sp), Arg::Ptr(b), Arg::Ptr(op)],
    );
    assert_eq!(tag, 0x04);
    assert_eq!(t.int_at(n), 7);
    assert_eq!(t.raw_str(t.ptr_at(sp)), b"text");
    assert_eq!(t.int_at(b), 1);
    assert_eq!(t.berval_at(t.ptr_at(op)), b"abc");
    let len = t.buf(4);
    assert_eq!(lber::peek_tag(t.j(), rd, len), 0x30);
    let b_ = t.buf(16);
    assert_eq!(lber::first_element(t.j(), rd, len, b_), 0x04);
    let ov = t.buf(32);
    assert_eq!(lber::scanf(t.j(), rd, "o", &[Arg::Ptr(ov)]), 0x04);
    assert_eq!(t.berval_at(ov), b"xyz");
    assert_eq!(lber::next_element(t.j(), rd, len, 0), i64::from(LBER_ERROR));
    let (tg, e) = (t.buf(4), t.buf(4));
    assert_eq!(
        lber::scanf(t.j(), rd, "{te}", &[Arg::Ptr(tg), Arg::Ptr(e)]),
        0x83
    );
    assert_eq!((t.int_at(tg), t.int_at(e)), (0x83, 9));
    let vv = t.buf(16);
    assert_eq!(lber::scanf(t.j(), rd, "v", &[Arg::Ptr(vv)]), 0x30);
    let arr2 = t.ptr_at(vv);
    assert_eq!(t.raw_str(t.ptr_at(arr2)), b"one");
    assert_eq!(t.raw_str(t.ptr_n(arr2, 1)), b"two");
    assert_eq!(
        lber::scanf(t.j(), rd, "i", &[Arg::Ptr(n)]),
        i64::from(LBER_ERROR)
    );
    let dup = lber::bvdup(t.j(), flat);
    assert_eq!(t.berval_at(dup), bytes);
    lber::bvfree(t.j(), dup);
    // ldap_create_control takes the encoded value.
    let (oid, cp) = (t.s("1.3.6.1.4.1.42"), t.buf(16));
    assert_eq!(control::create_control(t.j(), oid, ber, 1, cp), 0);
    let c = t.ptr_at(cp);
    assert_eq!(t.view(c, 49)[48], 1);
    lber::free(t.j(), ber, 1);
    lber::free(t.j(), rd, 1);
    assert_eq!(lber::printf(t.j(), ber, "i", &[Arg::Int(1)]), -1, "freed");
}
