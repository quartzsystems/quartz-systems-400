// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RSTLICPGM, DLTLICPGM and SAVLICPGM planning and execution.

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines
)]

mod common;

use common::{seeded, sys, vendor};
use licpgm::model::load::ObjEntry;
use licpgm::plan::delete::{DltLicPgm, Remover, execute_delete, plan_delete};
use licpgm::plan::restore::{RstLicPgm, execute_restore, plan_restore};
use licpgm::plan::save::{SavLicPgm, plan_save};
use licpgm::plan::{
    ExitOp, ExitRunner, Media, MediaError, MediaLoad, NoExits, Objects, Part, PlanError,
    RestoreError, Restorer,
};
use licpgm::store::find_load;
use licpgm::{
    Date, LoadKey, LoadState, LoadType, MemStore, OptionNo, ProductId, ProductStore, QualName,
};

struct MemMedia {
    loads: Vec<MediaLoad>,
}

impl MemMedia {
    fn vendor() -> Self {
        let p = vendor();
        Self {
            loads: p
                .loads
                .iter()
                .map(|(q, l)| MediaLoad {
                    definition_name: p.definition_name.clone(),
                    definition: p.definition.clone(),
                    load_name: q.clone(),
                    load: {
                        let mut l = l.clone();
                        l.state = LoadState::Packaged;
                        l
                    },
                })
                .collect(),
        }
    }
}

impl Media for MemMedia {
    fn loads(&self) -> Vec<MediaLoad> {
        self.loads.clone()
    }

    fn save_data(&self, key: &LoadKey, part: &Part) -> Result<Vec<u8>, MediaError> {
        Ok(format!("{key}|{part:?}").into_bytes())
    }
}

#[derive(Default)]
struct MemRestorer {
    calls: Vec<(String, String)>,
    fail_on: Option<String>,
}

impl Restorer for MemRestorer {
    fn restore(&mut self, stream: &[u8], library: &str) -> Result<Vec<ObjEntry>, RestoreError> {
        if self.fail_on.as_deref() == Some(library) {
            return Err(RestoreError("media read error".into()));
        }
        self.calls
            .push((String::from_utf8_lossy(stream).into(), library.into()));
        Ok(vec![ObjEntry::new(library, "RESTORED", "*PGM")])
    }

    fn restore_directory(
        &mut self,
        stream: &[u8],
        path: &str,
    ) -> Result<Vec<String>, RestoreError> {
        self.calls
            .push((String::from_utf8_lossy(stream).into(), path.into()));
        Ok(vec![format!("{path}/bin")])
    }
}

#[derive(Default)]
struct Exits {
    calls: Vec<(String, ExitOp)>,
    fail: Option<String>,
}

impl ExitRunner for Exits {
    fn call(&mut self, pgm: &QualName, op: ExitOp, _: &LoadKey) -> Result<(), String> {
        self.calls.push((pgm.to_string(), op));
        if self.fail.as_deref() == Some(pgm.name.as_str()) {
            return Err("ended in error".into());
        }
        Ok(())
    }
}

fn pid(s: &str) -> ProductId {
    s.parse().unwrap()
}

fn with_vendor_libs() -> MemStore {
    let mut s = seeded();
    for l in ["INVPRD", "INVDTA", "INVRPT"] {
        s.libs.insert(l.into());
    }
    s
}

fn accept(s: &mut MemStore) {
    s.agr.accept(
        &pid("1AB1234"),
        release::RELEASE,
        "QSECOFR",
        Date::new(2026, 10, 30).unwrap(),
    );
}

fn order(plan: &licpgm::plan::restore::InstallPlan) -> Vec<(u8, LoadType)> {
    plan.steps
        .iter()
        .map(|s| (s.key().option.0, s.key().load_type))
        .collect()
}

#[test]
fn restore_orders_base_then_options_code_then_language() {
    let s = with_vendor_libs();
    let m = MemMedia::vendor();
    // Asked for in the wrong order: the plan puts it right.
    let reqs = [
        RstLicPgm::new(pid("1AB1234"), OptionNo(1)),
        RstLicPgm::new(pid("1AB1234"), OptionNo::BASE),
    ];
    let plan = plan_restore(&s, &m, &sys(), &reqs).unwrap();
    assert_eq!(
        order(&plan),
        [
            (0, LoadType::Code),
            (0, LoadType::Lng),
            (1, LoadType::Code),
            (1, LoadType::Lng)
        ]
    );
    assert_eq!(plan.agreements, [(pid("1AB1234"), release::RELEASE)]);
    let mut pgm = RstLicPgm::new(pid("1AB1234"), OptionNo::BASE);
    pgm.objects = Objects::Pgm;
    assert_eq!(
        order(&plan_restore(&s, &m, &sys(), &[pgm]).unwrap()),
        [(0, LoadType::Code)]
    );
}

#[test]
fn restore_prerequisites() {
    let s = with_vendor_libs();
    let m = MemMedia::vendor();
    let r = plan_restore(
        &s,
        &m,
        &sys(),
        &[RstLicPgm::new(pid("1AB1234"), OptionNo(1))],
    );
    assert_eq!(r.unwrap_err(), PlanError::BaseMissing(pid("1AB1234")));
    let mut lng = RstLicPgm::new(pid("1AB1234"), OptionNo::BASE);
    lng.objects = Objects::Lng;
    assert!(matches!(
        plan_restore(&s, &m, &sys(), &[lng]),
        Err(PlanError::CodeMissing(..))
    ));
    let mut other = RstLicPgm::new(pid("1AB1234"), OptionNo::BASE);
    other.lng = Some(licpgm::LoadId::nlv("2928").unwrap());
    assert!(matches!(
        plan_restore(&s, &m, &sys(), &[other]),
        Err(PlanError::NotOnMedia { .. })
    ));
    // Without the operating system the base's prerequisite is missing.
    let bare = MemStore::with_libs(&["INVPRD", "INVDTA"]);
    let r = plan_restore(
        &bare,
        &m,
        &sys(),
        &[RstLicPgm::new(pid("1AB1234"), OptionNo::BASE)],
    );
    assert!(matches!(r, Err(PlanError::Prereq { .. })));
    let r = plan_restore(
        &s,
        &m,
        &sys(),
        &[RstLicPgm::new(pid("2AB1234"), OptionNo::BASE)],
    );
    assert!(matches!(r, Err(PlanError::NotOnMedia { .. })));
}

#[test]
fn restore_executes_with_exits_and_states() {
    let mut s = with_vendor_libs();
    let m = MemMedia::vendor();
    let reqs = [
        RstLicPgm::new(pid("1AB1234"), OptionNo::BASE),
        RstLicPgm::new(pid("1AB1234"), OptionNo(1)),
    ];
    let plan = plan_restore(&s, &m, &sys(), &reqs).unwrap();
    let mut r = MemRestorer::default();
    let mut x = Exits::default();
    let e = execute_restore(&plan, &mut s, &m, &mut r, &mut x, 7).unwrap_err();
    assert!(matches!(e, PlanError::Agreement(_)));
    accept(&mut s);
    let done = execute_restore(&plan, &mut s, &m, &mut r, &mut x, 7).unwrap();
    assert_eq!(done.len(), 4);
    for st in &plan.steps {
        let (_, l) = find_load(&s, st.key()).unwrap();
        assert_eq!(l.state, LoadState::Installed, "{}", st.key());
        assert_eq!(l.installed_at, 7);
    }
    let libs: Vec<&str> = r.calls.iter().map(|(_, l)| l.as_str()).collect();
    assert_eq!(
        libs[..3],
        ["INVPRD", "INVDTA", "/QIBM/ProdData/INV"],
        "the base code load's parts in order"
    );
    assert!(r.calls[0].0.contains("Library(\"INVPRD\")"));
    assert_eq!(x.calls[0], ("INVPRD/INVPRE".into(), ExitOp::Restore));
    assert!(
        x.calls
            .contains(&("INVPRD/INVPOST".into(), ExitOp::Restore))
    );
    // The product's license information now exists.
    assert_eq!(
        s.lic
            .info
            .iter()
            .filter(|i| i.product == pid("1AB1234"))
            .count(),
        2
    );
    // Installed again: it replaces itself.
    let again = plan_restore(&s, &m, &sys(), &reqs).unwrap();
    assert!(again.steps.iter().all(|st| st.replaces.is_some()));
    assert!(again.agreements.is_empty());
}

#[test]
fn restore_failures_set_ibm_states() {
    let m = MemMedia::vendor();
    let reqs = [RstLicPgm::new(pid("1AB1234"), OptionNo::BASE)];
    let base_code = |s: &MemStore| {
        s.lods
            .values()
            .find(|l| l.key.option.is_base() && l.key.load_type == LoadType::Code)
            .unwrap()
            .state
    };

    let mut s = with_vendor_libs();
    accept(&mut s);
    let plan = plan_restore(&s, &m, &sys(), &reqs).unwrap();
    let mut x = Exits {
        fail: Some("INVPRE".into()),
        ..Exits::default()
    };
    let e = execute_restore(&plan, &mut s, &m, &mut MemRestorer::default(), &mut x, 0).unwrap_err();
    assert!(matches!(e, PlanError::Exit(..)));
    assert_eq!(base_code(&s), LoadState::PreExitFailedPackaged);

    let mut s = with_vendor_libs();
    accept(&mut s);
    let mut r = MemRestorer {
        fail_on: Some("INVDTA".into()),
        ..MemRestorer::default()
    };
    let e = execute_restore(&plan, &mut s, &m, &mut r, &mut NoExits, 0).unwrap_err();
    assert!(matches!(e, PlanError::Restore(..)));
    assert_eq!(base_code(&s), LoadState::RestoreFailedPackaged);

    let mut s = with_vendor_libs();
    accept(&mut s);
    let mut x = Exits {
        fail: Some("INVPOST".into()),
        ..Exits::default()
    };
    execute_restore(&plan, &mut s, &m, &mut MemRestorer::default(), &mut x, 0).unwrap_err();
    assert_eq!(base_code(&s), LoadState::PostExitFailed);

    // Over an installed load, the failure codes are the 6x ones.
    let plan = plan_restore(&s, &m, &sys(), &reqs).unwrap();
    let mut r = MemRestorer {
        fail_on: Some("INVPRD".into()),
        ..MemRestorer::default()
    };
    execute_restore(&plan, &mut s, &m, &mut r, &mut NoExits, 0).unwrap_err();
    assert_eq!(base_code(&s), LoadState::RestoreFailedInstalled);
}

fn installed_vendor() -> MemStore {
    let mut s = with_vendor_libs();
    accept(&mut s);
    let m = MemMedia::vendor();
    let reqs = [
        RstLicPgm::new(pid("1AB1234"), OptionNo::BASE),
        RstLicPgm::new(pid("1AB1234"), OptionNo(1)),
    ];
    let plan = plan_restore(&s, &m, &sys(), &reqs).unwrap();
    execute_restore(
        &plan,
        &mut s,
        &m,
        &mut MemRestorer::default(),
        &mut NoExits,
        1,
    )
    .unwrap();
    s
}

#[derive(Default)]
struct MemRemover {
    gone: Vec<String>,
}

impl Remover for MemRemover {
    fn delete_library(&mut self, lib: &str) -> Result<(), String> {
        self.gone.push(format!("lib {lib}"));
        Ok(())
    }

    fn delete_object(&mut self, o: &ObjEntry) -> Result<(), String> {
        self.gone.push(format!("obj {}/{}", o.lib, o.name));
        Ok(())
    }

    fn delete_path(&mut self, path: &str) -> Result<(), String> {
        self.gone.push(format!("path {path}"));
        Ok(())
    }
}

#[test]
fn delete_one_option_then_everything() {
    let mut s = installed_vendor();
    let req = DltLicPgm {
        product: pid("1AB1234"),
        option: Some(OptionNo(1)),
        release: None,
        lng: None,
    };
    let plan = plan_delete(&s, &req).unwrap();
    let keys: Vec<(u8, LoadType)> = plan
        .loads
        .iter()
        .map(|(_, l)| (l.key.option.0, l.key.load_type))
        .collect();
    assert_eq!(keys, [(1, LoadType::Lng), (1, LoadType::Code)]);
    assert_eq!(plan.libraries, ["INVRPT"]);
    assert!(plan.definitions.is_empty());

    let req = DltLicPgm {
        product: pid("1AB1234"),
        option: None,
        release: None,
        lng: None,
    };
    let plan = plan_delete(&s, &req).unwrap();
    let opts: Vec<u8> = plan.loads.iter().map(|(_, l)| l.key.option.0).collect();
    assert_eq!(
        opts,
        [1, 0, 1, 0],
        "language loads first, options before *BASE"
    );
    assert_eq!(plan.libraries, ["INVRPT", "INVPRD", "INVDTA"]);
    assert_eq!(plan.directories, ["/QIBM/ProdData/INV"]);
    assert_eq!(plan.paths, ["/QIBM/ProdData/INV/bin"]);
    assert_eq!(plan.definitions, [QualName::new("P1AB1234", "INVPRD")]);
    let mut rm = MemRemover::default();
    execute_delete(&plan, &mut s, &mut rm, &mut NoExits).unwrap();
    assert!(rm.gone.contains(&"lib INVPRD".to_string()));
    assert!(s.lods.values().all(|l| l.key.product != pid("1AB1234")));
    assert!(s.defs.values().all(|d| d.product != pid("1AB1234")));
    assert!(s.agr.answer(&pid("1AB1234"), release::RELEASE).is_none());
}

#[test]
fn delete_refuses_what_others_need_and_spares_system_libraries() {
    let s = installed_vendor();
    let req = DltLicPgm {
        product: pid("QS00SS1"),
        option: None,
        release: None,
        lng: None,
    };
    let Err(PlanError::Dependents(d)) = plan_delete(&s, &req) else {
        panic!("dependents expected")
    };
    assert!(d.contains(&(pid("1AB1234"), OptionNo::BASE)));
    assert!(d.contains(&(pid("QS00WDS"), OptionNo::BASE)));
    // Qshell needs nothing gone and lives in QSYS: only its records go.
    let req = DltLicPgm {
        product: pid("QS00SS1"),
        option: Some(OptionNo(30)),
        release: None,
        lng: None,
    };
    let plan = plan_delete(&s, &req).unwrap();
    assert!(plan.libraries.is_empty());
    assert_eq!(plan.directories, ["/QIBM/ProdData/qsh"]);
    let req = DltLicPgm {
        product: pid("QS00SS1"),
        option: Some(OptionNo(30)),
        release: None,
        lng: Some(licpgm::LoadId::nlv("2924").unwrap()),
    };
    let plan = plan_delete(&s, &req).unwrap();
    assert_eq!(plan.loads.len(), 1);
    assert_eq!(plan.loads[0].1.key.load_type, LoadType::Lng);
    let req = DltLicPgm {
        product: pid("QS00DG1"),
        option: None,
        release: None,
        lng: None,
    };
    assert_eq!(
        plan_delete(&s, &req).unwrap_err(),
        PlanError::NotInstalled(pid("QS00DG1"))
    );
}

#[test]
fn save_plans() {
    let mut s = installed_vendor();
    let req = SavLicPgm {
        product: pid("1AB1234"),
        option: None,
        release: None,
        lng: None,
        objects: Objects::All,
    };
    let plan = plan_save(&s, &req).unwrap();
    assert_eq!(plan.definitions, [QualName::new("P1AB1234", "INVPRD")]);
    assert_eq!(plan.loads.len(), 4);
    assert_eq!(plan.loads[0].libraries, ["INVPRD", "INVDTA"]);
    assert_eq!(plan.loads[0].directories, ["/QIBM/ProdData/INV"]);
    let req = SavLicPgm {
        product: pid("QS00SS1"),
        option: Some(OptionNo::BASE),
        release: None,
        lng: None,
        objects: Objects::Pgm,
    };
    let plan = plan_save(&s, &req).unwrap();
    assert_eq!(plan.loads.len(), 1);
    assert!(
        plan.loads[0].libraries.is_empty(),
        "system libraries are not saved whole"
    );
    // A damaged load cannot be saved.
    let (q, mut l) = s
        .lods
        .iter()
        .find(|(_, l)| l.key.product == pid("1AB1234"))
        .map(|(q, l)| (q.clone(), l.clone()))
        .unwrap();
    l.state = LoadState::InstalledDamaged;
    s.update_load(&q, &l).unwrap();
    let req = SavLicPgm {
        product: pid("1AB1234"),
        option: None,
        release: None,
        lng: None,
        objects: Objects::All,
    };
    assert!(matches!(
        plan_save(&s, &req),
        Err(PlanError::NotSavable(..))
    ));
}
