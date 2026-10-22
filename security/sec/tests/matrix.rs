// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The authority matrix: every operation in `required.toml`, every
//! source of authority, a range of granted sets; each answer and the
//! step that decided it is what the search order says it must be. Then
//! the precedence rules one by one.

mod common;

use common::{NOW, create, lib, open_new};
use sec::auth::bits::Auth;
use sec::auth::principal::{Adopted, Principal};
use sec::auth::required::{self, Need};
use sec::auth::search::{self, Step};
use sec::auth::{autl, grant};
use sec::cmd::{Env, Keywords, usrprf};
use sec::profile::{SpecialAuth, shipped};
use store::obj::header::Authority;
use store::obj::{ObjectId, types};
use store::txn::Txn;

fn sysval(n: &str) -> String {
    if n == "QSECURITY" {
        "40".into()
    } else {
        String::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Source {
    Owner,
    Private,
    Autl,
    GroupPrivate,
    PrimaryGroup,
    GroupAutl,
    Public,
    PublicAutl,
    Adopted,
    AllObj,
    GroupAllObj,
}

const SOURCES: [Source; 11] = [
    Source::Owner,
    Source::Private,
    Source::Autl,
    Source::GroupPrivate,
    Source::PrimaryGroup,
    Source::GroupAutl,
    Source::Public,
    Source::PublicAutl,
    Source::Adopted,
    Source::AllObj,
    Source::GroupAllObj,
];

fn grants() -> Vec<Auth> {
    let mut v = vec![
        Auth::EXCLUDE,
        Auth::USE,
        Auth::CHANGE,
        Auth::ALL,
        Auth::ALL_AUTL,
    ];
    for (_, b) in sec::auth::bits::NAMES {
        v.push(Auth(b));
    }
    v
}

/// The type an operation's entry is for.
fn type_of(key: &str) -> (String, u16) {
    if let Some((_, t)) = key.split_once('@') {
        let info = types::type_by_name(t).unwrap();
        return (t.into(), info.code);
    }
    if key.starts_with("lib.") {
        ("*LIB".into(), types::LIB)
    } else if key.starts_with("usrprf.") {
        ("*USRPRF".into(), types::USRPRF)
    } else if key.starts_with("autl.") {
        ("*AUTL".into(), types::AUTL)
    } else {
        ("*DTAARA".into(), types::DTAARA)
    }
}

struct World {
    qsys: ObjectId,
    app: ObjectId,
    n: usize,
}

impl World {
    /// An object of `type_code` set up so that `source` grants `bits` to
    /// USERA (member of GRPA and GRPB) and nothing else does.
    fn object(&mut self, t: &mut Txn<'_>, type_code: u16, source: Source, bits: Auth) -> ObjectId {
        self.n += 1;
        let name = format!("O{:05}", self.n);
        let owner = if source == Source::Owner {
            "USERA"
        } else {
            "QSECOFR"
        };
        let id = if type_code == types::LIB {
            create(t, self.qsys, &name, types::LIB, owner)
        } else if type_code == types::AUTL {
            autl::create(t, self.qsys, &name, "", owner, Authority::EXCLUDE, NOW).unwrap()
        } else if type_code == types::USRPRF {
            create(t, self.qsys, &name, types::USRPRF, owner)
        } else {
            create(t, self.app, &name, type_code, owner)
        };
        grant::set_public(t, id, Some(Auth::EXCLUDE)).unwrap();
        let list = format!("L{:05}", self.n);
        let secure = |t: &mut Txn<'_>, who: Option<&str>, public: Option<Auth>| {
            autl::create(t, self.qsys, &list, "", "QSECOFR", Authority::EXCLUDE, NOW).unwrap();
            let lid = autl::find(t, self.qsys, &list).unwrap();
            if let Some(w) = who {
                grant::set_private(t, self.qsys, lid, w, Some(bits)).unwrap();
            }
            if let Some(p) = public {
                grant::set_public(t, lid, Some(p)).unwrap();
            }
            grant::set_autl(t, self.qsys, id, Some(&list)).unwrap();
        };
        match source {
            Source::Owner => grant::set_owner_authority(t, id, bits).unwrap(),
            Source::Private => grant::set_private(t, self.qsys, id, "USERA", Some(bits)).unwrap(),
            Source::GroupPrivate => {
                grant::set_private(t, self.qsys, id, "GRPB", Some(bits)).unwrap()
            }
            Source::PrimaryGroup => {
                grant::set_group(t, self.qsys, id, Some("GRPA"), true).unwrap();
                grant::set_group_authority(t, self.qsys, id, bits).unwrap();
            }
            Source::Public => grant::set_public(t, id, Some(bits)).unwrap(),
            Source::Autl if type_code != types::AUTL => secure(t, Some("USERA"), None),
            Source::GroupAutl if type_code != types::AUTL => secure(t, Some("GRPA"), None),
            Source::PublicAutl if type_code != types::AUTL => {
                secure(t, None, Some(bits));
                grant::set_public(t, id, None).unwrap();
            }
            Source::Adopted => grant::set_private(t, self.qsys, id, "OWNERX", Some(bits)).unwrap(),
            _ => {}
        }
        id
    }
}

fn principal(source: Source) -> Principal {
    let mut p = Principal {
        user: "USERA".into(),
        groups: vec!["GRPA".into(), "GRPB".into()],
        spcaut: 0,
        group_spcaut: 0,
        qsecurity: 40,
        adopted: Vec::new(),
    };
    match source {
        Source::AllObj => p.spcaut = SpecialAuth::ALLOBJ.0,
        Source::GroupAllObj => p.group_spcaut = SpecialAuth::ALLOBJ.0,
        Source::Adopted => p.adopted.push(Adopted {
            program: "APPLIB/PAYPGM".into(),
            owner: "OWNERX".into(),
            groups: Vec::new(),
            spcaut: 0,
        }),
        _ => {}
    }
    p
}

fn step_of(source: Source) -> Step {
    match source {
        Source::Owner => Step::Owner,
        Source::Private => Step::Private,
        Source::Autl => Step::Autl,
        Source::GroupPrivate => Step::GroupPrivate,
        Source::PrimaryGroup => Step::PrimaryGroup,
        Source::GroupAutl => Step::GroupAutl,
        Source::Public => Step::Public,
        Source::PublicAutl => Step::PublicAutl,
        Source::Adopted => Step::Adopted,
        Source::AllObj => Step::AllObj,
        Source::GroupAllObj => Step::GroupAllObj,
    }
}

fn setup() -> (store::Store, World) {
    let (mut store, qsys) = open_new();
    let mut t = store.begin().unwrap();
    let app = lib(&mut t, qsys, "APPLIB");
    shipped::install(&mut t, qsys, 40, 3, NOW).unwrap();
    let secofr = Principal::load(&mut t, qsys, "QSECOFR", 40).unwrap();
    let env = Env {
        qsys,
        actor: &secofr,
        now: NOW,
        sysval: &sysval,
    };
    for u in ["GRPA", "GRPB", "OWNERX"] {
        usrprf::crtusrprf(&mut t, &env, &Keywords::new(&[("USRPRF", u)])).unwrap();
    }
    usrprf::crtusrprf(
        &mut t,
        &env,
        &Keywords::new(&[
            ("USRPRF", "USERA"),
            ("GRPPRF", "GRPA"),
            ("SUPGRPPRF", "GRPB"),
        ]),
    )
    .unwrap();
    t.commit().unwrap();
    (store, World { qsys, app, n: 0 })
}

#[test]
fn every_operation_source_and_grant() {
    let (mut store, mut w) = setup();
    let mut t = store.begin().unwrap();
    let mut checked = 0;
    let ops = required::names();
    for key in &ops {
        let (type_name, code) = type_of(key);
        let op = key.split('@').next().unwrap();
        let need = required::need(op, &type_name).unwrap();
        for source in SOURCES {
            let list_source = matches!(
                source,
                Source::Autl | Source::GroupAutl | Source::PublicAutl
            );
            if list_source && code == types::AUTL {
                continue;
            }
            for bits in grants() {
                let id = w.object(&mut t, code, source, bits);
                let h = grant::header(&mut t, id).unwrap();
                let p = principal(source);
                let d = search::check(&mut t, w.qsys, &p, &h, need).unwrap();
                // A list entry never carries *AUTLMGT to the objects it secures.
                let given = if list_source {
                    Auth(bits.0 & Auth::ALL.0)
                } else {
                    bits
                };
                let want = match source {
                    Source::AllObj | Source::GroupAllObj => true,
                    _ => search::satisfies(given, need, false),
                };
                assert_eq!(d.allowed, want, "{key} {source:?} {}", bits.display(false));
                // Needing nothing, the user's *PUBLIC *EXCLUDE already suffices.
                let step = if source == Source::Adopted && need == Need::Bits(Auth::EXCLUDE) {
                    Step::Public
                } else {
                    step_of(source)
                };
                if want || source != Source::Adopted {
                    assert_eq!(d.step, step, "{key} {source:?} {}", bits.display(false));
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 5000, "{checked} checks");
}

#[test]
fn precedence() {
    let (mut store, mut w) = setup();
    let mut t = store.begin().unwrap();
    let use_ = Need::Bits(Auth::USE);
    let p = principal(Source::Private);
    let qsys = w.qsys;
    let check = |t: &mut Txn<'_>, id, need| {
        let h = grant::header(t, id).unwrap();
        search::check(t, qsys, &p, &h, need).unwrap()
    };

    // A private *EXCLUDE beats *PUBLIC *ALL.
    let id = w.object(&mut t, types::DTAARA, Source::Private, Auth::EXCLUDE);
    grant::set_public(&mut t, id, Some(Auth::ALL)).unwrap();
    let d = check(&mut t, id, use_);
    assert!(!d.allowed);
    assert_eq!(d.step, Step::Private);

    // The user's own entry beats a group's better one.
    grant::set_private(&mut t, w.qsys, id, "GRPA", Some(Auth::ALL)).unwrap();
    assert!(!check(&mut t, id, use_).allowed);

    // Groups add up: *USE from one, *ADD from the other.
    let id = w.object(&mut t, types::DTAARA, Source::Public, Auth::EXCLUDE);
    grant::set_private(&mut t, w.qsys, id, "GRPA", Some(Auth::USE)).unwrap();
    grant::set_private(&mut t, w.qsys, id, "GRPB", Some(Auth(Authority::ADD))).unwrap();
    let need = Need::Bits(Auth::USE.union(Auth(Authority::ADD)));
    let d = check(&mut t, id, need);
    assert!(d.allowed);
    assert_eq!(d.step, Step::GroupPrivate);

    // A group entry, even *EXCLUDE, hides *PUBLIC.
    let id = w.object(&mut t, types::DTAARA, Source::GroupPrivate, Auth::EXCLUDE);
    grant::set_public(&mut t, id, Some(Auth::ALL)).unwrap();
    assert!(!check(&mut t, id, use_).allowed);

    // The user's list entry beats the group's private authority.
    let id = w.object(&mut t, types::DTAARA, Source::Autl, Auth::EXCLUDE);
    grant::set_private(&mut t, w.qsys, id, "GRPA", Some(Auth::ALL)).unwrap();
    let d = check(&mut t, id, use_);
    assert!(!d.allowed);
    assert_eq!(d.step, Step::Autl);

    // Adopted authority adds to what the user has.
    let id = w.object(
        &mut t,
        types::DTAARA,
        Source::Public,
        Auth(Authority::OBJOPR | Authority::READ),
    );
    grant::set_private(&mut t, w.qsys, id, "OWNERX", Some(Auth(Authority::EXECUTE))).unwrap();
    let mut pa = principal(Source::Adopted);
    pa.groups.clear();
    let h = grant::header(&mut t, id).unwrap();
    assert!(
        !search::check(&mut t, w.qsys, &principal(Source::Public), &h, use_)
            .unwrap()
            .allowed
    );
    let d = search::check(&mut t, w.qsys, &pa, &h, use_).unwrap();
    assert!(d.allowed);
    assert_eq!(d.step, Step::Adopted);

    // Level 20: everyone has *ALLOBJ.
    let mut low = principal(Source::Private);
    low.qsecurity = 20;
    let id = w.object(&mut t, types::DTAARA, Source::Private, Auth::EXCLUDE);
    let h = grant::header(&mut t, id).unwrap();
    assert!(
        search::check(&mut t, w.qsys, &low, &h, use_)
            .unwrap()
            .allowed
    );
}
