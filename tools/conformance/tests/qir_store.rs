// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The kit through the store: every module created as a `*MODULE`,
//! bound with `crtpgm` into a `*PGM` object, loaded again and run, as
//! the service console does it; the outcomes must match the in-memory
//! run's.

use std::fs;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use conformance::qir::{CLOCK, Kit, call, job_for};
use ile::store::{Ctx, ObjSpec};
use ile::{ActGrpAttr, BindOptions, CreateData, ModuleObject, PgmKind};
use mi::MemSource;
use qsname::{LibSpec, Name};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, types};
use store::{FormatParams, Store, StoreConfig};

fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

#[test]
fn cases_survive_the_store_round_trip() {
    let kit = Kit::locate();
    let d = Arc::new(MemBlockDev::with_seed(32768, 3));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: CLOCK,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, CLOCK, "QS400").unwrap();
    let qgpl = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: name("QGPL"),
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSYS".into(),
            system: "QS400".into(),
            now: CLOCK,
            by_system: false,
        },
    )
    .unwrap();
    let ctx = Ctx {
        qsys,
        names: NameContext {
            curlib: Some(qgpl),
            libl: vec![qsys, qgpl],
        },
        now: CLOCK,
        owner: "QSYS".into(),
        system: "QS400".into(),
    };
    let mut differ = Vec::new();
    for case in kit.cases().unwrap() {
        if !case.qemu {
            continue;
        }
        let mut src = MemSource::default();
        let mut names = Vec::new();
        for m in &case.modules {
            let ir = qir::parse(m).unwrap();
            let actgrp = match &ir.actgrp {
                qir::ActGrp::New => ActGrpAttr::New,
                qir::ActGrp::Caller => ActGrpAttr::Caller,
                qir::ActGrp::Named(n) => ActGrpAttr::Named(n.clone()),
            };
            let mo = ModuleObject::compile(ir, CreateData::default(), CLOCK).unwrap();
            let mname = mo.name.clone();
            ile::store::crtmod(&mut t, &ctx, qgpl, name(&mname), &mo, "").unwrap();
            let mut opts = BindOptions::new(&mname, PgmKind::Pgm);
            opts.attr.actgrp = actgrp;
            opts.allow_unresolved = true;
            let (id, _) = ile::store::crtpgm(
                &mut t,
                &ctx,
                qgpl,
                &[ObjSpec {
                    lib: LibSpec::Name(name("QGPL")),
                    name: name(&mname),
                }],
                &[],
                opts,
                "",
            )
            .unwrap();
            let loaded = ile::store::load_program(&mut t, id).unwrap();
            names.push(loaded.name.clone());
            src.add("QGPL", loaded);
        }
        let main = names[0].clone();
        let job = job_for(&case, src, &names).unwrap();
        let got = call(&case, job, &main).unwrap();
        let want =
            fs::read_to_string(kit.expected_dir().join(format!("{}.out", case.name))).unwrap();
        if got != want.replace("\r\n", "\n") {
            differ.push(format!(
                "{}:\n--- store\n{got}--- memory\n{want}",
                case.name
            ));
        }
    }
    t.commit().unwrap();
    assert!(differ.is_empty(), "{}", differ.join("\n"));
}
