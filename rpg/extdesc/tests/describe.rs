// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Database files described from a store: a keyed DDS physical file
//! with an alias and a null-capable field, a logical file over it and
//! an SQL table keyed on its primary key; and a program that CHAINs
//! both by key compiles against them.

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use db::file::CreateFileOpts;
use dds::{CompileOpts, FileRef, MapRefs, Source, compile_lf, compile_pf};
use qsname::Name;
use rpgc::files::{ExtFile, ExtFiles};
use rpgfront::source::{Member, Record};
use sql::{Naming, RunOpts, SqlContext};
use store::obj::ops::{self, NameContext};
use store::obj::{CreateParams, JobHandle, ObjectId, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_792_713_600;

const PF: &str = concat!(
    "     A          R CUSTR\n",
    "     A            CUSTNO         5P 0\n",
    "     A            CNAME         20A         ALIAS(CUSTOMER_NAME)\n",
    "     A            NOTE          10A         ALWNULL\n",
    "     A          K CUSTNO",
);

const LF: &str = concat!(
    "     A          R CUSTR                     PFILE(CUSTPF)\n",
    "     A          K CNAME",
);

const TABLE: &str = "CREATE TABLE QGPL.ORDERS (ORDER_ID INTEGER NOT NULL PRIMARY KEY, \
                     AMOUNT DECIMAL(7, 2) NOT NULL, MEMO VARCHAR(20))";

fn store() -> (Store, ObjectId, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(32768, 13));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 256,
        now: NOW,
    };
    let mut store = Store::format(
        vec![Box::new(d) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, NOW, "QS400").unwrap();
    let lib = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: Name::parse("QGPL").unwrap(),
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: false,
        },
    )
    .unwrap();
    t.commit().unwrap();
    (store, qsys, lib)
}

/// The store with the three files.
fn files() -> (Store, ObjectId, ObjectId) {
    let (mut store, qsys, lib) = store();
    let opts = CreateFileOpts {
        now: NOW,
        ..CreateFileOpts::default()
    };
    let compile = |n: &str| CompileOpts {
        file: Some(n.into()),
        ..CompileOpts::default()
    };
    let mut refs = MapRefs::default();
    let mut t = store.begin().unwrap();
    let (src, _) = Source::from_text(PF);
    let c = compile_pf(&src, &refs, &compile("CUSTPF"));
    let pf = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
    let pf_id =
        db::file::create_pf(&mut t, lib, Name::parse("CUSTPF").unwrap(), &pf, &opts).unwrap();
    refs.insert_pf("QGPL", "CUSTPF", &pf);
    let (src, _) = Source::from_text(LF);
    let lf = compile_lf(&src, &refs, &compile("CUSTLF"))
        .definition
        .unwrap();
    let mut resolve = |_: &FileRef| -> db::Result<ObjectId> { Ok(pf_id) };
    db::file::create_lf(
        &mut t,
        lib,
        Name::parse("CUSTLF").unwrap(),
        &lf,
        &mut resolve,
        &opts,
    )
    .unwrap();
    t.commit().unwrap();
    let mut ctx = SqlContext::new(
        qsys,
        JobHandle(1),
        NameContext {
            curlib: Some(lib),
            libl: vec![lib],
        },
    );
    ctx.now = Some(NOW);
    let run = RunOpts {
        naming: Naming::Sql,
        ..RunOpts::default()
    };
    let r = sql::runsqlstm(&mut store, &mut ctx, TABLE, &run).unwrap();
    assert_eq!(r.errors, 0, "{}", r.listing);
    (store, qsys, lib)
}

/// Each file's description, read back as the compile commands do.
struct StoreExt(Store, ObjectId);

impl ExtFiles for StoreExt {
    fn describe(&mut self, name: &str, _lib: &str) -> Option<ExtFile> {
        let lib = self.1;
        let mut t = self.0.begin().ok()?;
        let id = store::obj::library::Library::load(&mut t, lib)
            .ok()?
            .lookup(&mut t, types::FILE, Name::parse(name).ok()?)
            .ok()??;
        let d = db::file::describe(&mut t, id).ok();
        t.abort();
        d.as_ref().map(rpgextdesc::database_desc)
    }
}

#[test]
fn keys_nulls_aliases_and_level_ids_are_described() {
    let (store, _, lib) = files();
    let mut ext = StoreExt(store, lib);

    let pf = ext.describe("CUSTPF", "*LIBL").unwrap();
    assert_eq!(pf.lib, "QGPL");
    let f = &pf.formats[0];
    assert_eq!(f.keys, ["CUSTNO"]);
    assert_eq!(f.level_id.len(), 13, "{f:?}");
    let by = |n: &str| f.fields.iter().find(|x| x.name == n).unwrap();
    assert_eq!(by("CNAME").alias, "CUSTOMER_NAME");
    assert!(by("NOTE").nullable);
    assert!(!by("CUSTNO").nullable);

    let lf = ext.describe("CUSTLF", "*LIBL").unwrap();
    assert_eq!(lf.formats[0].keys, ["CNAME"]);
    assert_eq!(lf.formats[0].level_id, f.level_id);

    let t = ext.describe("ORDERS", "*LIBL").unwrap();
    let o = &t.formats[0];
    assert_eq!(o.keys, ["ORDER_ID"]);
    assert_eq!(o.level_id.len(), 13);
    let memo = o.fields.iter().find(|x| x.name == "MEMO").unwrap();
    assert!(memo.nullable);
    assert!(
        matches!(memo.ty, rpgc::types::Ty::VarChar(20, 2)),
        "{memo:?}"
    );
}

#[test]
fn chain_by_key_compiles_against_them() {
    let (store, _, lib) = files();
    let mut ext = StoreExt(store, lib);
    let text = [
        "**FREE",
        "ctl-opt alwnull(*usrctl);",
        "dcl-f CUSTPF keyed;",
        "dcl-f ORDERS keyed rename(ORDERS:ORDERR);",
        "dcl-s cust packed(5) inz(1);",
        "dcl-s oid int(10) inz(7);",
        "chain cust CUSTPF;",
        "if %found(CUSTPF) and not %nullind(NOTE);",
        "  dsply CNAME;",
        "endif;",
        "chain (oid) ORDERS;",
        "if %found(ORDERS);",
        "  dsply %char(AMOUNT);",
        "endif;",
        "*inlr = *on;",
    ];
    let member = Member {
        lib: "QGPL".into(),
        file: "QRPGLESRC".into(),
        name: "CHAINS".into(),
        ccsid: 37,
        records: text
            .iter()
            .enumerate()
            .map(|(i, l)| Record {
                seq: format!("{:06}", (i + 1) * 100),
                date: "000000".into(),
                text: (*l).to_string(),
            })
            .collect(),
    };
    let opts = rpgc::Options {
        command: rpgc::Command::CrtBndRpg,
        name: "CHAINS".into(),
        lib: "QGPL".into(),
        ..rpgc::Options::default()
    };
    let c = rpgc::compile(member, &mut rpgfront::NoIncludes, &mut ext, &opts);
    assert!(c.created, "{:?}", c.diags);
}
