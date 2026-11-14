// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Searches and compares against a directory in memory, checked where
//! the primary was observed (`tools/conformance/oracle/dirsrv/
//! ldaptest-primary.txt`) against its answers.

mod harness;

use harness::{T, dns, done, names, text, vals};
use ldap::ResultCode;
use ldap::control::{
    Control, ControlValue, PagedResults, SortKey, SortRequest, SortResponse,
};
use ldap::msg::{ProtocolOp, Scope};

#[test]
fn seeded_entries_and_scopes() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    let r = t.search(&anon, "cn=localhost", Scope::Sub, "(objectClass=*)", &[]);
    assert_eq!(dns(&r), ["cn=localhost", "CN=REPLICATION,cn=localhost"]);
    let r = t.search(&anon, "cn=ibmpolicies", Scope::One, "(objectClass=*)", &[]);
    assert_eq!(dns(&r), ["cn=pwdpolicy,CN=IBMPOLICIES", "CN=REPLICATION,CN=IBMPOLICIES"]);
    let r = t.search(&anon, "cn=ibmpolicies", Scope::Base, "(objectClass=*)", &[]);
    assert_eq!(dns(&r), ["CN=IBMPOLICIES"]);
    // A subtree search of "" covers every suffix of the store.
    let r = t.search(&anon, "", Scope::Sub, "(objectClass=*)", &["1.1"]);
    let d = dns(&r);
    assert!(d.contains(&"cn=pwdpolicy,CN=IBMPOLICIES".to_string()));
    assert!(d.contains(&"o=Acme".to_string()));
    let r = t.search(&anon, "", Scope::One, "(objectClass=*)", &[]);
    assert_eq!(done(&r).code, ResultCode::UnwillingToPerform);
    let r = t.search(&anon, "cn=nothing,cn=localhost", Scope::Base, "(objectClass=*)", &[]);
    assert_eq!(done(&r).code, ResultCode::NoSuchObject);
    assert_eq!(done(&r).matched_dn, "CN=LOCALHOST");
    let r = t.search(&anon, "dc=nowhere", Scope::Sub, "(objectClass=*)", &[]);
    assert_eq!(done(&r).code, ResultCode::NoSuchObject);
    let r = t.search(&anon, "not a dn", Scope::Base, "(objectClass=*)", &[]);
    assert_eq!(done(&r).code, ResultCode::InvalidDnSyntax);
}

#[test]
fn special_entries() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    let r = t.search(&anon, "cn=schema", Scope::Base, "(objectClass=*)", &["objectClass"]);
    assert_eq!(dns(&r), ["cn=schema"]);
    let r = t.search(&anon, "cn=schema", Scope::One, "(objectClass=*)", &[]);
    assert_eq!(done(&r).code, ResultCode::ProtocolError);
    let r = t.search(&anon, "cn=monitor", Scope::Base, "(objectClass=*)", &[]);
    assert!(names(&r).contains(&"opsinitiated".to_string()));
    let r = t.search(&anon, "cn=monitor", Scope::One, "(objectClass=*)", &[]);
    assert_eq!(done(&r).code, ResultCode::NoSuchObject);
    let r = t.search(&anon, "cn=connections,cn=monitor", Scope::Base, "(objectClass=*)", &[]);
    assert_eq!(done(&r).code, ResultCode::UnwillingToPerform);
    let r = t.search(&anon, "cn=changelog", Scope::Base, "(objectClass=*)", &[]);
    assert_eq!(done(&r).code, ResultCode::NoSuchObject);
}

#[test]
fn filters_as_the_primary_answers() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    let cases: &[(&str, &[&str])] = &[
        ("(cn=pwdpolicy)", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(CN=PWDPOLICY)", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(cn=pwd*)", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(cn=*policy)", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(cn=*dpo*)", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(cn=p*d*y)", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(cn>=m)", &["cn=pwdpolicy,CN=IBMPOLICIES", "CN=REPLICATION,CN=IBMPOLICIES"]),
        ("(cn<=m)", &["CN=IBMPOLICIES"]),
        ("(cn~=pwdpolicy)", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(cn~=ibmpolicy)", &["CN=IBMPOLICIES"]),
        ("(objectClass=pwdPolicy)", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(!(cn=pwdpolicy))", &["CN=IBMPOLICIES", "CN=REPLICATION,CN=IBMPOLICIES"]),
        ("(&(objectClass=*)(cn=pwdpolicy))", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(|(cn=nothing)(cn=pwdpolicy))", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(&(|(cn=a*)(cn=p*))(!(cn=a*)))", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(nosuchattribute=x)", &[]),
        ("(!(nosuchattribute=x))", &[]),
        ("(cn=\\2a)", &[]),
        ("(cn=pwd\\70olicy)", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(&)", &[]),
        ("(|)", &[]),
        ("(createTimestamp<=19700101000000Z)", &[]),
        // QS/400 runs extensible match (IBM's matches nothing).
        ("(cn:caseExactMatch:=pwdPolicy)", &["cn=pwdpolicy,CN=IBMPOLICIES"]),
        ("(cn:dn:=ibmpolicies)", &["CN=IBMPOLICIES", "cn=pwdpolicy,CN=IBMPOLICIES", "CN=REPLICATION,CN=IBMPOLICIES"]),
    ];
    for (f, want) in cases {
        let r = t.search(&anon, "cn=ibmpolicies", Scope::Sub, f, &["1.1"]);
        assert_eq!(done(&r).code, ResultCode::Success, "{f}");
        assert_eq!(dns(&r), *want, "{f}");
    }
    let r = t.search(&anon, "cn=ibmpolicies", Scope::Sub, "(createTimestamp>=19700101000000Z)", &[]);
    assert_eq!(dns(&r).len(), 3);
    for f in ["(aclEntry=*)", "(entryOwner=*)"] {
        let r = t.search(&anon, "cn=localhost", Scope::Sub, f, &[]);
        assert_eq!(done(&r).code, ResultCode::UnwillingToPerform, "{f}");
    }
    let a = t.admin.clone();
    let r = t.search(&a, "cn=localhost", Scope::Sub, "(aclEntry=*)", &[]);
    assert_eq!(dns(&r).len(), 2);
}

#[test]
fn attribute_selection() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    let base = |t: &mut T, attrs: &[&str]| t.search(&anon, "cn=ibmpolicies", Scope::Base, "(objectClass=*)", attrs);
    assert_eq!(names(&base(&mut t, &[])), ["cn", "objectclass"]);
    assert_eq!(names(&base(&mut t, &["*"])), ["cn", "objectclass"]);
    assert_eq!(names(&base(&mut t, &["1.1"])), Vec::<String>::new());
    assert_eq!(names(&base(&mut t, &["CN"])), ["cn"]);
    assert_eq!(names(&base(&mut t, &["1.1", "cn"])), ["cn"]);
    assert_eq!(names(&base(&mut t, &["nosuchattribute"])), Vec::<String>::new());
    assert_eq!(names(&base(&mut t, &["*", "aclEntry"])), ["aclentry", "cn", "objectclass"]);
    let plus = names(&base(&mut t, &["+"]));
    for n in [
        "aclentry",
        "aclpropagate",
        "aclsource",
        "createtimestamp",
        "creatorsname",
        "entryowner",
        "ibm-effectiveacl",
        "ibm-entryuuid",
        "ibm-replicationisquiesced",
        "ibm-replicationthisserverismaster",
        "modifiersname",
        "modifytimestamp",
        "ownerpropagate",
        "ownersource",
        "subschemasubentry",
    ] {
        assert!(plus.contains(&n.to_string()), "{n} in {plus:?}");
    }
    let r = base(&mut t, &["+"]);
    assert_eq!(vals(&r, "aclSource"), ["default"]);
    assert_eq!(vals(&r, "ibm-effectiveAcl"), ["group:CN=ANYBODY:restricted:rsc:system:rsc:normal:rsc"]);
    assert_eq!(vals(&r, "aclEntry"), ["group:CN=ANYBODY:normal:rsc:system:rsc:restricted:rsc"]);
    assert_eq!(vals(&r, "entryOwner"), ["access-id:cn=Administrator"]);
    let r = base(&mut t, &["entryDN", "hasSubordinates", "numSubordinates"]);
    assert_eq!(vals(&r, "entryDN"), ["CN=IBMPOLICIES"]);
    assert_eq!(vals(&r, "hasSubordinates"), ["TRUE"]);
    assert_eq!(vals(&r, "numSubordinates"), ["2"]);
    let r = base(&mut t, &["+ibmaci"]);
    assert!(names(&r).contains(&"aclsource".to_string()) && !names(&r).contains(&"creatorsname".to_string()));
    // typesOnly.
    let mut req = T::req("cn=localhost", Scope::Base, "(objectClass=*)", &[]);
    req.types_only = true;
    let r = t.search_req(&anon, req, Vec::new());
    assert_eq!(names(&r), ["cn", "objectclass"]);
    assert!(vals(&r, "cn").is_empty());
}

#[test]
fn root_dse() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    let r = t.search(&anon, "", Scope::Base, "(objectClass=*)", &[]);
    assert_eq!(dns(&r), [""]);
    assert_eq!(
        vals(&r, "namingContexts"),
        ["CN=SCHEMA", "CN=LOCALHOST", "CN=IBMPOLICIES", "O=ACME"]
    );
    assert_eq!(vals(&r, "vendorName"), ["Quartz Systems"]);
    assert_eq!(vals(&r, "supportedSASLMechanisms"), ["EXTERNAL"]);
    assert!(vals(&r, "supportedExtension").contains(&"1.3.6.1.4.1.4203.1.11.3".to_string()));
    let r = t.search(&anon, "", Scope::Base, "(objectClass=*)", &["+"]);
    assert!(names(&r).is_empty());
    let r = t.search(&anon, "", Scope::Base, "(objectClass=*)", &["namingContexts"]);
    assert_eq!(names(&r), ["namingcontexts"]);
    let r = t.search(&anon, "", Scope::Base, "(cn=nothing)", &[]);
    assert!(dns(&r).is_empty());
}

/// The root DSE against the primary's: the same attribute names but for
/// the documented differences (DIRSRV.md §15).
#[test]
fn root_dse_against_the_primary() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    let r = t.search(&anon, "", Scope::Base, "(objectClass=*)", &[]);
    let ours: std::collections::BTreeSet<String> = names(&r).into_iter().collect();
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tools/conformance/oracle/dirsrv/rootdse.ldif"
    ))
    .unwrap();
    let theirs: std::collections::BTreeSet<String> = text
        .lines()
        .filter_map(|l| l.split_once(':'))
        .map(|(k, _)| k.trim().to_ascii_lowercase())
        .filter(|k| k != "dn")
        .collect();
    // §15: no DIGEST-MD5, so no realm; audit goes to QAUDJRN.
    let only_theirs = ["ibm-sasldigestrealmname", "ibm-supportedauditversion"];
    for k in &theirs {
        if only_theirs.contains(&k.as_str()) || (k == "ibm-serverid" && !ours.contains(k)) {
            continue;
        }
        assert!(ours.contains(k), "missing {k}");
    }
    for k in &ours {
        assert!(theirs.contains(k), "extra {k}");
    }
    // Every control and extension of ours the primary has is the same
    // OID; ours adds whoami, password modify, assertion and the reads.
    let primary_ctls: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("supportedcontrol: "))
        .collect();
    let additions = ["1.3.6.1.1.12", "1.3.6.1.1.13.1", "1.3.6.1.1.13.2"];
    for c in vals(&r, "supportedControl") {
        assert!(primary_ctls.contains(&c.as_str()) || additions.contains(&c.as_str()), "{c}");
    }
}

#[test]
fn size_limits() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    let lim = |t: &mut T, n: i32, base: &str| {
        let mut req = T::req(base, Scope::Sub, "(objectClass=*)", &["1.1"]);
        req.size_limit = n;
        let r = t.search_req(&anon, req, Vec::new());
        (done(&r).code, dns(&r).len())
    };
    assert_eq!(lim(&mut t, 1, "cn=localhost"), (ResultCode::SizeLimitExceeded, 1));
    assert_eq!(lim(&mut t, 2, "cn=localhost"), (ResultCode::Success, 2));
    assert_eq!(lim(&mut t, 0, "cn=localhost"), (ResultCode::Success, 2));
    assert_eq!(lim(&mut t, 2, "cn=ibmpolicies"), (ResultCode::SizeLimitExceeded, 2));
    // The server's limit binds others, not the administrator.
    let mut s = harness::settings();
    s.size_limit = 1;
    let mut t = T::with(s);
    assert_eq!(lim(&mut t, 0, "cn=localhost"), (ResultCode::SizeLimitExceeded, 1));
    let a = t.admin.clone();
    let r = t.search(&a, "cn=localhost", Scope::Sub, "(objectClass=*)", &[]);
    assert_eq!(dns(&r).len(), 2);
}

#[test]
fn time_limit() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    for i in 0..30 {
        let a = t.admin.clone();
        t.add_ok(&a, &format!("ou=u{i},o=Acme"), &[text("objectClass", &["organizationalUnit"]), text("ou", &[&format!("u{i}")])]);
    }
    let mut req = T::req("o=Acme", Scope::Sub, "(objectClass=*)", &["1.1"]);
    req.time_limit = 1;
    let msg = T::msg(ProtocolOp::SearchRequest(req), Vec::new());
    let mut txn = t.sys.store.begin().unwrap();
    let dirsrv::ops::Started::Cursor(id) = t.dir.search(&mut txn, &anon, &msg, &mut t.cursors) else {
        panic!()
    };
    let p = t.dir.next(&mut txn, &mut t.cursors, id, dirsrv::ops::Budget { entries: 5, bytes: 1 << 20 });
    assert_eq!(p.replies.len(), 5);
    t.clock.fetch_add(5_000_000, std::sync::atomic::Ordering::SeqCst);
    let mut last = None;
    for _ in 0..20 {
        let p = t.dir.next(&mut txn, &mut t.cursors, id, dirsrv::ops::Budget { entries: 5, bytes: 1 << 20 });
        last = p.replies.last().cloned();
        if p.state != dirsrv::ops::PageState::More {
            break;
        }
    }
    assert_eq!(last.unwrap().op.result().unwrap().code, ResultCode::TimeLimitExceeded);
}

#[test]
fn budget_in_bytes() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    let msg = T::msg(ProtocolOp::SearchRequest(T::req("cn=ibmpolicies", Scope::Sub, "(objectClass=*)", &[])), Vec::new());
    let mut txn = t.sys.store.begin().unwrap();
    let dirsrv::ops::Started::Cursor(id) = t.dir.search(&mut txn, &anon, &msg, &mut t.cursors) else {
        panic!()
    };
    let p = t.dir.next(&mut txn, &mut t.cursors, id, dirsrv::ops::Budget { entries: 100, bytes: 1 });
    assert_eq!(p.replies.len(), 1);
    assert_eq!(p.state, dirsrv::ops::PageState::More);
}

fn paged(size: i32, cookie: Vec<u8>, critical: bool) -> Control {
    PagedResults { size, cookie }.to_control(critical)
}

fn cookie_of(r: &[dirsrv::ops::Reply]) -> Vec<u8> {
    PagedResults::find(&r.last().unwrap().controls).unwrap().unwrap().cookie
}

#[test]
fn paged_results() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    let req = || T::req("cn=ibmpolicies", Scope::Sub, "(objectClass=*)", &["1.1"]);
    let mut seen = Vec::new();
    let mut cookie = Vec::new();
    let mut pages = 0;
    loop {
        let r = t.search_req(&anon, req(), vec![paged(1, cookie.clone(), false)]);
        pages += 1;
        seen.extend(dns(&r));
        cookie = cookie_of(&r);
        if cookie.is_empty() {
            break;
        }
    }
    assert_eq!(seen.len(), 3);
    assert_eq!(pages, 4);
    // Size 0 with the cookie ends it.
    let r = t.search_req(&anon, req(), vec![paged(1, Vec::new(), false)]);
    let c = cookie_of(&r);
    let r = t.search_req(&anon, req(), vec![paged(0, c.clone(), false)]);
    assert_eq!(done(&r).code, ResultCode::Success);
    assert!(cookie_of(&r).is_empty());
    let r = t.search_req(&anon, req(), vec![paged(1, c, false)]);
    assert_eq!(done(&r).code, ResultCode::UnwillingToPerform);
    // A cookie the server did not give.
    let r = t.search_req(&anon, req(), vec![paged(1, b"bogus".to_vec(), false)]);
    assert_eq!(done(&r).code, ResultCode::UnwillingToPerform);
    // Two open at once, the third refused.
    let r1 = t.search_req(&anon, req(), vec![paged(1, Vec::new(), false)]);
    let _r2 = t.search_req(&anon, req(), vec![paged(1, Vec::new(), false)]);
    let r3 = t.search_req(&anon, req(), vec![paged(1, Vec::new(), false)]);
    assert_eq!(done(&r3).code, ResultCode::UnwillingToPerform);
    let r = t.search_req(&anon, req(), vec![paged(0, cookie_of(&r1), false)]);
    assert_eq!(done(&r).code, ResultCode::Success);
    let r = t.search_req(&anon, req(), vec![paged(1, Vec::new(), true)]);
    assert_eq!(done(&r).code, ResultCode::Success);
}

fn sort_ctl(keys: &[(&str, bool)], critical: bool) -> Control {
    SortRequest(
        keys.iter()
            .map(|(a, rev)| SortKey {
                reverse: *rev,
                ..SortKey::new(a)
            })
            .collect(),
    )
    .to_control(critical)
}

#[test]
fn sorting() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    let req = || T::req("cn=ibmpolicies", Scope::Sub, "(objectClass=*)", &["cn"]);
    let r = t.search_req(&anon, req(), vec![sort_ctl(&[("cn", false)], true)]);
    assert_eq!(vals_all(&r), ["IBMpolicies", "pwdPolicy", "REPLICATION"]);
    let sr = SortResponse::find(&r.last().unwrap().controls).unwrap().unwrap();
    assert_eq!(sr.result, ResultCode::Success);
    let r = t.search_req(&anon, req(), vec![sort_ctl(&[("cn", true)], true)]);
    assert_eq!(vals_all(&r), ["REPLICATION", "pwdPolicy", "IBMpolicies"]);
    let r = t.search_req(&anon, req(), vec![sort_ctl(&[("nosuchattribute", false)], true)]);
    assert_eq!(done(&r).code, ResultCode::UnavailableCriticalExtension);
    let sr = SortResponse::find(&r.last().unwrap().controls).unwrap().unwrap();
    assert_eq!(sr.result, ResultCode::NoSuchAttribute);
    let r = t.search_req(&anon, req(), vec![sort_ctl(&[("nosuchattribute", false)], false)]);
    assert_eq!(done(&r).code, ResultCode::Success);
    assert_eq!(dns(&r).len(), 3);
    let r = t.search_req(&anon, req(), vec![sort_ctl(&[("cn", false), ("sn", false), ("o", false), ("ou", false)], false)]);
    let sr = SortResponse::find(&r.last().unwrap().controls).unwrap().unwrap();
    assert_eq!(sr.result, ResultCode::AdminLimitExceeded);
    // Sorted and paged.
    let mut order = Vec::new();
    let mut cookie = Vec::new();
    loop {
        let r = t.search_req(&anon, req(), vec![sort_ctl(&[("cn", false)], false), paged(1, cookie.clone(), false)]);
        order.extend(vals_all(&r));
        cookie = cookie_of(&r);
        if cookie.is_empty() {
            break;
        }
    }
    assert_eq!(order, ["IBMpolicies", "pwdPolicy", "REPLICATION"]);
}

fn vals_all(r: &[dirsrv::ops::Reply]) -> Vec<String> {
    r.iter()
        .filter_map(|x| match &x.op {
            ProtocolOp::SearchResultEntry { attrs, .. } => attrs
                .iter()
                .find(|a| a.ty.eq_ignore_ascii_case("cn"))
                .map(|a| String::from_utf8_lossy(&a.vals[0]).to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn compare() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    let cmp = |t: &mut T, dn: &str, a: &str, v: &str| {
        let o = t.run(
            &anon,
            ProtocolOp::CompareRequest {
                dn: dn.into(),
                attr: a.into(),
                value: v.as_bytes().to_vec(),
            },
            Vec::new(),
        );
        o.replies[0].op.result().unwrap().clone()
    };
    assert_eq!(cmp(&mut t, "cn=localhost", "cn", "LOCALHOST").code, ResultCode::CompareTrue);
    assert_eq!(cmp(&mut t, "cn=localhost", "cn", "nothing").code, ResultCode::CompareFalse);
    assert_eq!(cmp(&mut t, "cn=localhost", "objectClass", "top").code, ResultCode::CompareTrue);
    assert_eq!(cmp(&mut t, "cn=localhost", "nosuchattribute", "x").code, ResultCode::NoSuchAttribute);
    assert_eq!(cmp(&mut t, "cn=localhost", "description", "x").code, ResultCode::NoSuchAttribute);
    let r = cmp(&mut t, "cn=nothing,cn=localhost", "cn", "x");
    assert_eq!((r.code, r.matched_dn.as_str()), (ResultCode::NoSuchObject, "CN=LOCALHOST"));
    assert_eq!(cmp(&mut t, "not a dn", "cn", "x").code, ResultCode::InvalidDnSyntax);
    assert_eq!(cmp(&mut t, "cn=localhost", "entryDN", "CN=LOCALHOST").code, ResultCode::CompareTrue);
    // userPassword is critical: no compare without the right.
    t.person("joe", "pw1");
    assert_eq!(cmp(&mut t, "cn=joe,o=Acme", "userPassword", "pw1").code, ResultCode::InsufficientAccessRights);
}

#[test]
fn controls_unknown_and_misplaced() {
    let mut t = T::new();
    let anon = dirsrv::ops::Session::anonymous();
    let req = || T::req("cn=localhost", Scope::Base, "(objectClass=*)", &[]);
    let r = t.search_req(&anon, req(), vec![Control::new("1.2.3.4.5", true, None)]);
    assert_eq!(done(&r).code, ResultCode::UnavailableCriticalExtension);
    let r = t.search_req(&anon, req(), vec![Control::new("1.2.3.4.5", false, None)]);
    assert_eq!(done(&r).code, ResultCode::Success);
    // IBM's controls are not built: critical is 12.
    let r = t.search_req(&anon, req(), vec![Control::new("1.3.18.0.2.10.15", true, None)]);
    assert_eq!(done(&r).code, ResultCode::UnavailableCriticalExtension);
    let o = t.run(
        &anon,
        ProtocolOp::CompareRequest {
            dn: "cn=localhost".into(),
            attr: "cn".into(),
            value: b"localhost".to_vec(),
        },
        vec![paged(1, Vec::new(), true)],
    );
    assert_eq!(T::code(&o), ResultCode::UnavailableCriticalExtension);
    // Anonymous proxied authorization is 50.
    let r = t.search_req(&anon, req(), vec![ldap::control::ProxiedAuthz("dn:cn=Administrator".into()).to_control(true)]);
    assert_eq!(done(&r).code, ResultCode::InsufficientAccessRights);
    // The assertion control (built here; IBM's lacks it).
    let r = t.search_req(&anon, req(), vec![ldap::control::Assertion(ldap::Filter::parse("(cn=localhost)").unwrap()).to_control(true)]);
    assert_eq!(dns(&r).len(), 1);
    let r = t.search_req(&anon, req(), vec![ldap::control::Assertion(ldap::Filter::parse("(cn=no)").unwrap()).to_control(true)]);
    assert_eq!(done(&r).code, ResultCode::Other(122));
}
