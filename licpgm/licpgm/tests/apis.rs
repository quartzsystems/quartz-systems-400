// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSZCRTPD, QSZCRTPL, QSZPKGPO, QSZDLTPD, QSZDLTPL, QSZSPTPR and
//! QSZCHKTG over parameter bytes.

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines
)]

mod common;

use common::{e, sys};
use licpgm::api::chktg::qszchktg;
use licpgm::api::crtpd::{qszcrtpd, qszdltpd};
use licpgm::api::crtpl::{CrtPl, qszcrtpl, qszdltpl};
use licpgm::api::pkgpo::qszpkgpo;
use licpgm::api::sptpr::qszsptpr;
use licpgm::{Date, LoadState, MemStore, ObjStatus, ProductStore, QualName};

fn today() -> Date {
    Date::new(2026, 10, 30).unwrap()
}

fn qual(n: &str, l: &str) -> Vec<u8> {
    let mut b = e(n, 10);
    b.extend(e(l, 10));
    b
}

fn pd_info(product: &str) -> Vec<u8> {
    let mut b = e(product, 7);
    b.extend(e("V1R0M0", 6));
    b.extend(e("APPMSG", 10));
    b.extend(e("2025", 10));
    b.extend(e("*CURRENT", 10));
    b.extend(e("261030", 6));
    b.extend(e("*NO", 4));
    b.extend(e("*PHONE", 10));
    b.extend(e("15551234", 14));
    b.extend(e("", 1));
    b.extend(e("", 28));
    assert_eq!(b.len(), 106);
    b
}

fn option(o: &str, msg: &str, feature: &str) -> Vec<u8> {
    let mut b = e(o, 4);
    b.extend(e(msg, 7));
    b.extend(e("*NODYNNAM", 10));
    b.extend(e(feature, 4));
    b.extend(e("", 16));
    b
}

fn lng(id: &str, o: &str) -> Vec<u8> {
    let mut b = e(id, 8);
    b.extend(e(o, 4));
    b.extend(e("", 8));
    b
}

fn create(s: &mut MemStore) -> Result<(), licpgm::api::ApiError> {
    let mut opts = option("0000", "APP0001", "5001");
    opts.extend(option("0001", "APP0002", "5101"));
    let mut l = lng("2924", "0000");
    l.extend(lng("*BASEOPT", "0001"));
    qszcrtpd(
        s,
        &qual("APPDFN", "*CURLIB"),
        &pd_info("2AP0001"),
        &opts,
        2,
        &l,
        2,
        &e("An application", 50),
        &e("*USE", 10),
        "APPLIB",
        today(),
    )
}

fn pl_info(option: &str, t: &str, id: &str) -> Vec<u8> {
    let mut b = e("2AP0001", 7);
    b.extend(e("V1R0M0", 6));
    b.extend(e(option, 4));
    b.extend(e(t, 10));
    b.extend(e(id, 8));
    b.extend(e("*PRDDFN", 10));
    b.extend(e("", 14));
    b.extend(e("*CURRENT", 10));
    b.extend(e("", 18));
    assert_eq!(b.len(), 87);
    b
}

fn principal(dev: &str) -> Vec<u8> {
    let mut b = e(dev, 10);
    b.extend(e("*DVLLIB", 10));
    b.extend(e("*NONE", 10));
    b
}

#[test]
fn create_package_support_delete() {
    let mut s = MemStore::with_libs(&["APPLIB"]);
    create(&mut s).unwrap();
    let (q, d) = s.definitions().pop().unwrap();
    assert_eq!(q, QualName::new("APPDFN", "APPLIB"));
    assert_eq!(d.current_copyright, Some(2026));
    assert_eq!(d.release_date, Date::new(2026, 10, 30));
    assert_eq!(d.options[1].feature.as_str(), "5101");
    assert_eq!(create(&mut s).unwrap_err().id, "CPF0C17");

    // The code load of *BASE, with an extra library and an exit.
    let mut add = e("APPDTA", 10);
    add.extend(e("*DVLLIB", 10));
    add.extend(e("*NONE", 10));
    let mut pre = e("APPPRE", 10);
    pre.extend(e("APPLIB", 10));
    let info = pl_info("0000", "*CODE", "*CODEDFT");
    let p = CrtPl {
        name: &e("APPCODE", 10),
        info: &info,
        secondary_lib: &e("*NONE", 10),
        principal: &principal("*PRDDFN"),
        additional: &add,
        n_additional: 1,
        pre_exits: &pre,
        n_pre_exits: 1,
        folders: &[],
        n_folders: 0,
        text: &e("Application code", 50),
        public_aut: &e("*USE", 10),
        n_directories: None,
    };
    s.libs.insert("APPDTA".into());
    qszcrtpl(&mut s, &p).unwrap();
    let (lq, l) = s.loads().pop().unwrap();
    assert_eq!(lq, QualName::new("APPCODE", "APPLIB"));
    assert_eq!(l.state, LoadState::Created);
    assert_eq!(l.libraries.len(), 2);
    assert_eq!(l.pre_exits[0], QualName::new("APPPRE", "APPLIB"));
    // A load ID that is not the option's feature.
    let bad = pl_info("0000", "*CODE", "5101");
    assert_eq!(
        qszcrtpl(
            &mut s,
            &CrtPl {
                info: &bad,
                ..p.clone()
            }
        )
        .unwrap_err()
        .id,
        "CPF0C84"
    );
    // A language load the definition does not name.
    let bad = pl_info("0000", "*LNG", "2928");
    assert_eq!(
        qszcrtpl(
            &mut s,
            &CrtPl {
                info: &bad,
                ..p.clone()
            }
        )
        .unwrap_err()
        .id,
        "CPF0C84"
    );

    // Packaging takes the libraries' objects and checks them.
    s.add_object("APPLIB", "APPMAIN", "*PGM");
    s.objects.insert(
        licpgm::model::load::ObjEntry::new("APPDTA", "APPFILE", "*FILE"),
        ObjStatus::Damaged,
    );
    let mut po = e("0000", 4);
    po.extend(e("2AP0001", 7));
    po.extend(e("*ONLY", 6));
    po.extend(e("*ALL", 8));
    po.extend(e("", 10));
    let objs = s.clone();
    assert_eq!(
        qszpkgpo(&mut s, &objs, &po, &e("*NO", 4), &e("*SAME", 5))
            .unwrap_err()
            .id,
        "CPF0C19"
    );
    s.objects.insert(
        licpgm::model::load::ObjEntry::new("APPDTA", "APPFILE", "*FILE"),
        ObjStatus::Present(None),
    );
    let objs = s.clone();
    let done = qszpkgpo(&mut s, &objs, &po, &e("*NO", 4), &e("*SAME", 5)).unwrap();
    assert_eq!(done, std::slice::from_ref(&lq));
    let (_, l) = s.loads().pop().unwrap();
    assert_eq!(l.state, LoadState::Packaged);
    assert_eq!(l.objects.len(), 2, "every object of the libraries");
    // Already packaged, not asked to repackage: nothing to do.
    assert!(
        qszpkgpo(&mut s, &objs, &po, &e("*NO", 4), &e("*SAME", 5))
            .unwrap()
            .is_empty()
    );

    // QSZSPTPR.
    let mut sp = e("2AP0001", 7);
    sp.extend(e("V1R0M0", 6));
    sp.extend(e("0000", 4));
    sp.extend(e("*CODE", 10));
    sp.extend(e("", 10));
    sp.extend(e("08", 2));
    sp.extend(e("US1234567", 14));
    qszsptpr(&mut s, &sp, 53, "SPTP0100", 0).unwrap();
    assert!(!s.loads()[0].1.supported);
    qszsptpr(&mut s, &sp, 53, "SPTP0100", 1).unwrap();
    let l = &s.loads()[0].1;
    assert!(l.supported);
    assert_eq!(l.registration.as_ref().unwrap().value, "US1234567");
    assert_eq!(
        qszsptpr(&mut s, &sp, 53, "SPTP0200", 1).unwrap_err().id,
        "CPF3C21"
    );

    // Delete: not the definition while it has loads.
    assert_eq!(
        qszdltpd(&mut s, &qual("APPDFN", "APPLIB")).unwrap_err().id,
        "CPF0C52"
    );
    qszdltpl(&mut s, &qual("APPCODE", "APPLIB")).unwrap();
    assert_eq!(
        qszdltpl(&mut s, &qual("APPCODE", "APPLIB")).unwrap_err().id,
        "CPF2105"
    );
    qszdltpd(&mut s, &qual("APPDFN", "APPLIB")).unwrap();
    assert!(s.defs.is_empty());
}

#[test]
fn crtpd_checks() {
    let mut s = MemStore::with_libs(&["APPLIB"]);
    let l = lng("2924", "0000");
    let run = |s: &mut MemStore, info: &[u8], opts: &[u8], n: i32, l: &[u8], nl: i32| {
        qszcrtpd(
            s,
            &qual("APPDFN", "APPLIB"),
            info,
            opts,
            n,
            l,
            nl,
            &e("x", 50),
            &e("*USE", 10),
            "APPLIB",
            today(),
        )
        .unwrap_err()
        .id
    };
    let one = option("0001", "APP0001", "5001");
    assert_eq!(run(&mut s, &pd_info("2AP0001"), &one, 1, &l, 1), "CPF0CAA");
    let mut two = option("0000", "APP0001", "5001");
    two.extend(option("0000", "APP0001", "5001"));
    assert_eq!(run(&mut s, &pd_info("2AP0001"), &two, 2, &l, 1), "CPF0CAF");
    let base = option("0000", "APP0001", "5001");
    assert_eq!(run(&mut s, &pd_info("2AP0001"), &base, 0, &l, 1), "CPF0CBA");
    assert_eq!(run(&mut s, &pd_info("2AP0001"), &base, 1, &l, 0), "CPF0CB9");
    assert_eq!(run(&mut s, &pd_info("A123456"), &base, 1, &l, 1), "CPF0CB2");
    let bad_feature = option("0000", "APP0001", "1234");
    assert_eq!(
        run(&mut s, &pd_info("2AP0001"), &bad_feature, 1, &l, 1),
        "CPF0CBD"
    );
    let baseopt = lng("*BASEOPT", "0000");
    assert_eq!(
        run(&mut s, &pd_info("2AP0001"), &base, 1, &baseopt, 1),
        "CPF0CAB"
    );
    let mut info = pd_info("2AP0001");
    info[100] = 0xC1;
    assert_eq!(run(&mut s, &info, &base, 1, &l, 1), "CPF0CB3");
    let mut info = pd_info("2AP0001");
    info[23..33].copy_from_slice(&e("2030", 10));
    assert_eq!(run(&mut s, &info, &base, 1, &l, 1), "CPF0CB0");
    let r = qszcrtpd(
        &mut s,
        &qual("APPDFN", "NOLIB"),
        &pd_info("2AP0001"),
        &base,
        1,
        &l,
        1,
        &e("x", 50),
        &e("*USE", 10),
        "",
        today(),
    );
    assert_eq!(r.unwrap_err().id, "CPF9810");
}

#[test]
fn chktg() {
    let mut s = sys();
    let list = |v: &[&str]| v.iter().map(|x| (*x).to_string()).collect::<Vec<_>>();
    let r = qszchktg(&s, "*CURRENT", &list(&["V1R0M0"])).unwrap();
    assert_eq!(
        (r.validated.as_str(), r.equivalent.as_str()),
        ("V1R0M0", "V1R0M0")
    );
    assert_eq!(
        qszchktg(&s, "*PRV", &list(&["V1R0M0"])).unwrap_err().id,
        "CPF0C34"
    );
    assert_eq!(
        qszchktg(&s, "V7R4M0", &list(&["V1R0M0"])).unwrap_err().id,
        "CPF0C34"
    );
    let v11 = release::Release { v: 1, r: 1, m: 0 };
    s.release = v11;
    s.known_releases = vec![release::RELEASE, v11];
    let r = qszchktg(&s, "*PRV", &list(&["V1R0M0", "V1R1M0"])).unwrap();
    assert_eq!(
        (r.validated.as_str(), r.equivalent.as_str()),
        ("V1R0M0", "V1R0M0")
    );
    let r = qszchktg(&s, "V1R1M0", &list(&["V1R0M0"])).unwrap();
    assert_eq!(r.equivalent, "V1R0M0");
    assert_eq!(
        qszchktg(&s, "V1R0M0", &list(&["V1R1M0"])).unwrap_err().id,
        "CPF0C35"
    );
    assert_eq!(
        qszchktg(&s, "V1R0M0", &list(&["V1R1M0", "V1R0M0"]))
            .unwrap_err()
            .id,
        "CPF0C36"
    );
    let r = qszchktg(&s, "V1R0M0", &list(&["*SAV"])).unwrap();
    assert_eq!(r.equivalent, "V1R0M0");
    assert_eq!(
        qszchktg(&s, "V1R0M0", &list(&["*SAV", "V1R0M0"]))
            .unwrap_err()
            .id,
        "CPF0C36"
    );
}
