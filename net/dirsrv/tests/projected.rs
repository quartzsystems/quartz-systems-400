// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The projected backend over a profile source in memory (docs/DIRSRV.md
//! §9): who sees which profile, the primary's anonymous answers, groups,
//! read-only, paging through a large set.

mod harness;

use dirsrv::backend::projected::{ProfileInfo, Profiles, ProjectedBackend};
use dirsrv::ops::error::OpResult;
use dirsrv::ops::{Method, Session};
use harness::{T, dns, done, vals};
use ldap::msg::{Attribute, ProtocolOp, Scope};
use ldap::{Dn, ResultCode};
use store::txn::Txn;

const SUFFIX: &str = "os400-sys=SYSTEM";

/// Profiles: `QSECOFR` (`*ALLOBJ`), `ANN` and `BOB` in group `STAFF`,
/// `CAROL` whom `ANN` may read, and `PADnn` to page through.
struct Fake {
    pads: usize,
}

impl Fake {
    fn all(&self) -> Vec<String> {
        let mut v: Vec<String> = ["QSECOFR", "ANN", "BOB", "CAROL", "STAFF"]
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        v.extend((0..self.pads).map(|i| format!("PAD{i:03}")));
        v
    }
}

impl Profiles for Fake {
    fn names(&self, _txn: &mut Txn<'_>) -> OpResult<Vec<String>> {
        Ok(self.all())
    }

    fn profile(&self, _txn: &mut Txn<'_>, name: &str) -> OpResult<Option<ProfileInfo>> {
        if !self.all().iter().any(|n| n == name) {
            return Ok(None);
        }
        let group = name == "STAFF";
        Ok(Some(ProfileInfo {
            name: name.into(),
            group,
            members: if group {
                vec!["ANN".into(), "BOB".into()]
            } else {
                Vec::new()
            },
            attrs: vec![
                ("os400-text".into(), vec![format!("{name} text")]),
                ("os400-status".into(), vec!["*ENABLED".into()]),
                ("os400-spcaut".into(), Vec::new()),
            ],
        }))
    }

    fn sees_all(&self, _txn: &mut Txn<'_>, viewer: &str) -> bool {
        viewer == "QSECOFR"
    }

    fn may_read(&self, _txn: &mut Txn<'_>, viewer: &str, name: &str) -> bool {
        viewer == "ANN" && name == "CAROL"
    }
}

fn setup(pads: usize) -> T {
    let mut t = T::new();
    t.dir.add_backend(Box::new(
        ProjectedBackend::new(SUFFIX, Fake { pads }).unwrap(),
    ));
    t
}

fn profile_dn(n: &str) -> String {
    format!("os400-profile={n},cn=accounts,{SUFFIX}")
}

fn as_profile(t: &T, n: &str) -> Session {
    Session::bound(
        &t.dir.schema,
        &Dn::parse(&profile_dn(n)).unwrap(),
        Method::Simple,
    )
}

#[test]
fn anonymous_sees_the_root_only() {
    let mut t = setup(0);
    let anon = Session::anonymous();
    let r = t.search(&anon, SUFFIX, Scope::Base, "(objectClass=*)", &[]);
    assert_eq!(dns(&r), [SUFFIX]);
    assert_eq!(vals(&r, "objectClass"), ["top", "os400-root"]);
    let r = t.search(&anon, SUFFIX, Scope::One, "(objectClass=*)", &[]);
    assert_eq!((done(&r).code, dns(&r).len()), (ResultCode::Success, 0));
    let r = t.search(&anon, SUFFIX, Scope::Sub, "(objectClass=*)", &[]);
    assert_eq!(dns(&r), [SUFFIX]);
    let accounts = format!("cn=accounts,{SUFFIX}");
    for (base, scope) in [
        (accounts.as_str(), Scope::Base),
        (accounts.as_str(), Scope::One),
        (&profile_dn("QSECOFR"), Scope::Base),
        (&profile_dn("NOSUCH"), Scope::Base),
        (&format!("cn=x,{accounts}"), Scope::Base),
    ] {
        let r = t.search(&anon, base, scope, "(objectClass=*)", &[]);
        assert_eq!(
            (done(&r).code, dns(&r).len()),
            (ResultCode::Success, 0),
            "{base}"
        );
    }
    let o = t.run(
        &anon,
        ProtocolOp::CompareRequest {
            dn: SUFFIX.into(),
            attr: "objectClass".into(),
            value: b"os400-root".to_vec(),
        },
        Vec::new(),
    );
    assert_eq!(T::code(&o), ResultCode::CompareTrue);
}

#[test]
fn an_administrator_sees_every_profile() {
    let mut t = setup(0);
    let a = t.admin.clone();
    let r = t.search(&a, SUFFIX, Scope::Sub, "(objectClass=*)", &[]);
    assert_eq!(dns(&r).len(), 2 + 5);
    let r = t.search(
        &a,
        SUFFIX,
        Scope::Sub,
        "(os400-profile=staff)",
        &["os400-groupmember", "os400-text", "os400-spcaut"],
    );
    assert_eq!(dns(&r), [profile_dn("STAFF")]);
    assert_eq!(vals(&r, "os400-groupmember"), ["ANN", "BOB"]);
    assert_eq!(vals(&r, "os400-text"), ["STAFF text"]);
    assert!(vals(&r, "os400-spcaut").is_empty());
    // Under every profile's eye, a missing one is noSuchObject.
    let r = t.search(
        &a,
        &profile_dn("NOSUCH"),
        Scope::Base,
        "(objectClass=*)",
        &[],
    );
    assert_eq!(done(&r).code, ResultCode::NoSuchObject);
    assert_eq!(
        done(&r).matched_dn,
        format!("CN=ACCOUNTS,{SUFFIX}").to_uppercase()
    );
    let s = as_profile(&t, "QSECOFR");
    let r = t.search(
        &s,
        &format!("cn=accounts,{SUFFIX}"),
        Scope::One,
        "(objectClass=*)",
        &[],
    );
    assert_eq!(dns(&r).len(), 5);
}

#[test]
fn a_profile_sees_itself_and_what_it_may_read() {
    let mut t = setup(0);
    let ann = as_profile(&t, "ANN");
    let r = t.search(
        &ann,
        &format!("cn=accounts,{SUFFIX}"),
        Scope::Sub,
        "(objectClass=*)",
        &["1.1"],
    );
    let mut got = dns(&r);
    got.sort();
    assert_eq!(
        got,
        [
            format!("cn=accounts,{SUFFIX}"),
            profile_dn("ANN"),
            profile_dn("CAROL")
        ]
    );
    let r = t.search(
        &ann,
        &profile_dn("BOB"),
        Scope::Base,
        "(objectClass=*)",
        &[],
    );
    assert_eq!((done(&r).code, dns(&r).len()), (ResultCode::Success, 0));
    let bob = as_profile(&t, "bob");
    let r = t.search(&bob, SUFFIX, Scope::Sub, "(objectClass=*)", &["1.1"]);
    assert_eq!(dns(&r).len(), 3);
}

#[test]
fn writes_are_refused() {
    let mut t = setup(0);
    let a = t.admin.clone();
    let o = t.add(
        &a,
        &profile_dn("NEW"),
        &[Attribute::new("objectClass", &["os400-usrprf"])],
    );
    assert_eq!(T::code(&o), ResultCode::UnwillingToPerform);
    let o = t.run(&a, ProtocolOp::DelRequest(profile_dn("ANN")), Vec::new());
    assert_eq!(T::code(&o), ResultCode::UnwillingToPerform);
}

#[test]
fn many_profiles_come_in_pages() {
    let mut t = setup(300);
    let a = t.admin.clone();
    let r = t.search(
        &a,
        &format!("cn=accounts,{SUFFIX}"),
        Scope::One,
        "(objectClass=os400-usrprf)",
        &["1.1"],
    );
    assert_eq!(done(&r).code, ResultCode::Success);
    assert_eq!(dns(&r).len(), 305);
}
