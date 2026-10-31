// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Installed status, DSPSFWRSC and GO LICPGM 10 rows, CHKPRDOPT.

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines
)]

mod common;

use common::{seeded, sys, vendor};
use licpgm::directory::check::{Finding, chkprdopt};
use licpgm::directory::{InstalledStatus, installed_programs, option_status, software_resources};
use licpgm::model::load::ObjEntry;
use licpgm::{LoadState, LoadType, MemStore, ObjStatus, OptionNo, ProductLoad, ProductStore};

fn loads(s: &MemStore) -> Vec<ProductLoad> {
    s.loads().into_iter().map(|(_, l)| l).collect()
}

fn set(s: &mut MemStore, product: &str, opt: u8, t: LoadType, st: LoadState) {
    let (q, mut l) = s
        .loads()
        .into_iter()
        .find(|(_, l)| {
            l.key.product.as_str() == product && l.key.option.0 == opt && l.key.load_type == t
        })
        .unwrap();
    l.state = st;
    s.update_load(&q, &l).unwrap();
}

#[test]
fn shipped_system_rows() {
    let s = seeded();
    let rows = installed_programs(&s, &sys());
    let os: Vec<(String, String, &str)> = rows
        .iter()
        .filter(|r| r.product.as_str() == "QS00SS1")
        .map(|r| (r.option.to_string(), r.release.to_string(), r.status.name()))
        .collect();
    assert_eq!(os.len(), 6);
    assert!(
        os.iter()
            .all(|(_, rel, st)| rel == "V1R0M0" && *st == "*COMPATIBLE")
    );
    assert_eq!(os[0].0, "*BASE");
    assert!(
        rows.iter()
            .all(|r| !r.product.as_str().starts_with("QS00D")),
        "reserved products are not installed"
    );
    let res = software_resources(&s);
    let wds: Vec<(String, String)> = res
        .iter()
        .filter(|r| r.product.as_str() == "QS00WDS")
        .map(|r| (r.option.to_string(), r.feature.to_string()))
        .collect();
    assert_eq!(
        wds[..4],
        [
            ("*BASE".into(), "5001".into()),
            ("*BASE".into(), "2924".into()),
            ("21".into(), "5101".into()),
            ("21".into(), "2924".into())
        ]
    );
    assert_eq!(
        res.iter().find(|r| r.option.0 == 31).unwrap().text,
        "ILE RPG compiler"
    );
}

#[test]
fn status_follows_the_loads() {
    let mut s = seeded();
    let def = |s: &MemStore, p: &str| {
        s.defs
            .values()
            .find(|d| d.product.as_str() == p)
            .unwrap()
            .clone()
    };
    let os = def(&s, "QS00SS1");
    let st = |s: &MemStore, o: u8| option_status(&loads(s), &os, OptionNo(o), &sys());
    assert_eq!(st(&s, 30), Some(InstalledStatus::Compatible));
    set(
        &mut s,
        "QS00SS1",
        30,
        LoadType::Lng,
        LoadState::InstalledDamaged,
    );
    assert_eq!(st(&s, 30), Some(InstalledStatus::Damaged));
    set(&mut s, "QS00SS1", 30, LoadType::Lng, LoadState::Packaged);
    assert_eq!(st(&s, 30), Some(InstalledStatus::NoPrimary));
    set(
        &mut s,
        "QS00SS1",
        30,
        LoadType::Code,
        LoadState::PostExitFailed,
    );
    assert_eq!(st(&s, 30), Some(InstalledStatus::Error));
    set(&mut s, "QS00SS1", 30, LoadType::Code, LoadState::Packaged);
    assert_eq!(st(&s, 30), None);
    // An option without its *BASE is in error.
    set(&mut s, "QS00SS1", 0, LoadType::Code, LoadState::Packaged);
    assert_eq!(st(&s, 33), Some(InstalledStatus::Error));
    // Back level: an option older than *BASE when releases may not mix.
    let mut s = seeded();
    let (q, mut l) = s
        .loads()
        .into_iter()
        .find(|(_, l)| {
            l.key.product.as_str() == "QS00SS1"
                && l.key.option.is_base()
                && l.key.load_type == LoadType::Code
        })
        .unwrap();
    l.key.release = release::Release { v: 1, r: 1, m: 0 };
    s.update_load(&q, &l).unwrap();
    assert_eq!(st(&s, 12), Some(InstalledStatus::BklvlOpt));
    // A vendor product's compatibility is not known.
    let mut s = seeded();
    s.libs.insert("INVPRD".into());
    let p = vendor();
    s.create_definition(&p.definition_name, &p.definition)
        .unwrap();
    for (q, l) in &p.loads {
        let mut l = l.clone();
        l.state = LoadState::Installed;
        s.create_load(q, &l).unwrap();
    }
    assert_eq!(
        option_status(&loads(&s), &p.definition, OptionNo::BASE, &sys()),
        Some(InstalledStatus::Installed)
    );
}

#[test]
fn chkprdopt_finds_missing_and_damaged_and_recovers() {
    let mut s = seeded();
    s.libs.insert("INVPRD".into());
    s.libs.insert("INVDTA".into());
    let p = vendor();
    s.create_definition(&p.definition_name, &p.definition)
        .unwrap();
    for (q, l) in &p.loads {
        let mut l = l.clone();
        l.state = LoadState::Installed;
        s.create_load(q, &l).unwrap();
    }
    let pid = p.definition.product.clone();
    s.add_object("INVPRD", "INVMAIN", "*PGM");
    s.objects.insert(
        ObjEntry::new("INVDTA", "INVFILE", "*FILE"),
        ObjStatus::Damaged,
    );
    s.add_object("INVPRD", "INVMSG", "*MSGF");
    s.add_object("INVRPT", "RPT01", "*PGM");
    let objs = s.clone();
    let r = chkprdopt(&mut s, &objs, &pid, Some(OptionNo::BASE), None).unwrap();
    let base = r
        .loads
        .iter()
        .find(|(k, _, _)| k.load_type == LoadType::Code)
        .unwrap();
    assert_eq!(base.1, LoadState::InstalledDamaged);
    assert!(base.2.contains(&Finding::Damaged(ObjEntry::new(
        "INVDTA", "INVFILE", "*FILE"
    ))));
    assert!(
        base.2
            .contains(&Finding::PathMissing("/QIBM/ProdData/INV/bin".into()))
    );
    // Repaired: the check puts the load back to 90.
    s.objects.insert(
        ObjEntry::new("INVDTA", "INVFILE", "*FILE"),
        ObjStatus::Present(None),
    );
    s.paths
        .insert("/QIBM/ProdData/INV/bin".into(), ObjStatus::Present(None));
    let objs = s.clone();
    let r = chkprdopt(&mut s, &objs, &pid, None, None).unwrap();
    assert!(r.clean(), "{r:#?}");
    assert!(r.loads.iter().all(|(_, st, _)| *st == LoadState::Installed));
    // A wrong release and a missing object: state 64.
    s.objects.insert(
        ObjEntry::new("INVPRD", "INVMAIN", "*PGM"),
        ObjStatus::Present(Some(release::Release { v: 1, r: 1, m: 0 })),
    );
    let objs = s.clone();
    let r = chkprdopt(&mut s, &objs, &pid, Some(OptionNo::BASE), None).unwrap();
    assert!(
        r.loads
            .iter()
            .any(|(_, st, f)| *st == LoadState::InstalledMissing
                && matches!(f[0], Finding::WrongRelease(..)))
    );
    // Nothing installed.
    let none = "9ZZ9999".parse().unwrap();
    assert!(chkprdopt(&mut s, &objs, &none, None, None).is_err());
}
