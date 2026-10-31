// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The products in a real store: seeded, saved, deleted, restored.

use std::cell::RefCell;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use licpgm::model::load::ObjEntry;
use licpgm::plan::delete::{DltLicPgm, Remover, execute_delete, plan_delete};
use licpgm::plan::restore::{RstLicPgm, execute_restore, plan_restore};
use licpgm::plan::save::{SavLicPgm, plan_save};
use licpgm::plan::{NoExits, Objects};
use licpgm::{Date, LoadState, LoadType, OptionNo, ProductId, ProductStore, SystemInfo};
use lpstore::{Products, SaveRestorer, SavfMedia, save_product, stamp};
use qsname::Name;
use saverst::AllowAll;
use savfmt::{Compression, MediaHeader, SaveKind};
use store::obj::{CreateParams, JobHandle, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;

fn system() -> (Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(32768, 7));
    let fp = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 512,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &fp,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    for lib in ["QUSRSYS", "QSYS2", "QGPL"] {
        ops::create(
            &mut t,
            &CreateParams {
                library: qsys,
                name: Name::parse(lib).unwrap(),
                type_code: types::LIB,
                attribute: String::new(),
                text: String::new(),
                owner: "QSYS".into(),
                system: "QS400".into(),
                now: NOW,
                by_system: true,
            },
        )
        .unwrap();
    }
    t.commit().unwrap();
    (store, qsys)
}

fn wds() -> ProductId {
    "QS00WDS".parse().unwrap()
}

struct NoRemove;

impl Remover for NoRemove {
    fn delete_library(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn delete_object(&mut self, _: &ObjEntry) -> Result<(), String> {
        Ok(())
    }
    fn delete_path(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn seed_save_delete_restore() {
    let (mut store, qsys) = system();
    assert!(lpstore::seed(&mut store, qsys, NOW).unwrap());
    assert!(
        !lpstore::seed(&mut store, qsys, NOW).unwrap(),
        "seeding twice makes nothing"
    );
    let cell = RefCell::new(&mut store);
    let mut p = Products::new(&cell, qsys, stamp(NOW));
    assert_eq!(p.definitions().len(), 8);
    let installed = p
        .loads()
        .iter()
        .filter(|(_, l)| l.state == LoadState::Installed)
        .count();
    assert_eq!(installed, 2 * (6 + 4 + 1 + 1));
    assert!(!p.licenses().info.is_empty());

    // SAVLICPGM QS00WDS into a stream.
    let plan = plan_save(
        &p,
        &SavLicPgm {
            product: wds(),
            option: None,
            release: None,
            lng: None,
            objects: Objects::All,
        },
    )
    .unwrap();
    let header = MediaHeader {
        release: (1, 0, 0),
        target: (1, 0, 0),
        compression: Compression::None,
        kind: SaveKind::LicPgm,
        created: NOW,
        stream_id: [1; 16],
        system: "QS400".into(),
        serial: "QS40001".into(),
        command: "SAVLICPGM".into(),
        user: "QSECOFR".into(),
    };
    let stream = save_product(
        &cell,
        qsys,
        JobHandle(1),
        &AllowAll,
        &plan,
        &header,
        "SAVLICPGM LICPGM(QS00WDS)",
    )
    .unwrap();

    // DLTLICPGM QS00WDS.
    let d = plan_delete(
        &p,
        &DltLicPgm {
            product: wds(),
            option: None,
            release: None,
            lng: None,
        },
    )
    .unwrap();
    execute_delete(&d, &mut p, &mut NoRemove, &mut NoExits).unwrap();
    assert!(p.loads().iter().all(|(_, l)| l.key.product != wds()));
    assert!(p.definitions().iter().all(|(_, d)| d.product != wds()));

    // RSTLICPGM from the stream brings it back.
    let media = SavfMedia::read(stream).unwrap();
    assert_eq!(licpgm::plan::Media::loads(&media).len(), 8);
    let sys = SystemInfo::current("QS40001", "P05");
    let reqs: Vec<RstLicPgm> = [0, 21, 31, 61]
        .iter()
        .map(|o| RstLicPgm::new(wds(), OptionNo(*o)))
        .collect();
    let plan = plan_restore(&p, &media, &sys, &reqs).unwrap();
    assert_eq!(plan.agreements, [(wds(), release::RELEASE)]);
    let mut book = p.agreements();
    book.accept(
        &wds(),
        release::RELEASE,
        "QSECOFR",
        Date::new(2026, 10, 30).unwrap(),
    );
    p.save_agreements(&book).unwrap();
    let mut r = SaveRestorer::new(&cell, qsys, JobHandle(1), &AllowAll, NOW);
    execute_restore(&plan, &mut p, &media, &mut r, &mut NoExits, NOW).unwrap();
    let back: Vec<_> = p
        .loads()
        .into_iter()
        .filter(|(_, l)| l.key.product == wds() && l.key.load_type == LoadType::Code)
        .map(|(_, l)| (l.key.option.0, l.state))
        .collect();
    assert_eq!(back.len(), 4);
    assert!(back.iter().all(|(_, s)| *s == LoadState::Installed));
}

/// Records, at each library deletion, whether the product's loads were
/// already gone: they live in its principal library.
struct LoadsFirst<'c, 's> {
    cell: &'c RefCell<&'s mut Store>,
    qsys: ObjectId,
    product: ProductId,
    libraries: Vec<(String, bool)>,
}

impl Remover for LoadsFirst<'_, '_> {
    fn delete_library(&mut self, lib: &str) -> Result<(), String> {
        let p = Products::new(self.cell, self.qsys, stamp(NOW));
        let gone = p.loads().iter().all(|(_, l)| l.key.product != self.product);
        self.libraries.push((lib.to_string(), gone));
        Ok(())
    }
    fn delete_object(&mut self, _: &ObjEntry) -> Result<(), String> {
        Ok(())
    }
    fn delete_path(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

#[test]
fn dltlicpgm_drops_the_records_before_the_libraries() {
    let (mut store, qsys) = system();
    lpstore::seed(&mut store, qsys, NOW).unwrap();
    let mut t = store.begin().unwrap();
    ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: Name::parse("VNDLIB").unwrap(),
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSYS".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: true,
        },
    )
    .unwrap();
    t.commit().unwrap();
    let pkg = licpgm::pkg::parse(
        r#"
[product]
id = "1AB7001"
text = "Vendor product"
library = "VNDLIB"

[[option]]
option = "*BASE"
text = "Vendor product base"
msgid = "VND0001"
libraries = [{ name = "VNDLIB" }]
"#,
        licpgm::pkg::Options::default(),
    )
    .unwrap();
    let cell = RefCell::new(&mut store);
    let mut p = Products::new(&cell, qsys, stamp(NOW));
    licpgm::api::crtpd::create_definition(&mut p, &pkg.definition_name, &pkg.definition).unwrap();
    for (q, l) in &pkg.loads {
        let mut l = l.clone();
        l.state = LoadState::Installed;
        licpgm::api::crtpl::create_load(&mut p, q, &l).unwrap();
    }
    let product: ProductId = "1AB7001".parse().unwrap();
    let d = plan_delete(
        &p,
        &DltLicPgm {
            product: product.clone(),
            option: None,
            release: None,
            lng: None,
        },
    )
    .unwrap();
    let mut rm = LoadsFirst {
        cell: &cell,
        qsys,
        product,
        libraries: Vec::new(),
    };
    execute_delete(&d, &mut p, &mut rm, &mut NoExits).unwrap();
    assert_eq!(rm.libraries, [("VNDLIB".to_string(), true)]);
}
