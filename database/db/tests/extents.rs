// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A keyed member of a million records: its record space and its key
//! tree grow in turns, and the space keeps few extents (an object holds
//! at most `MAX_EXTENTS`; growing by a fixed step ran out near 853,000
//! records with the ASP mostly free).

mod common;

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use common::{fld, key, name, pf, rec};
use db::Member;
use db::file::{self, CreateFileOpts, Size};
use qsname::MemberSpec;
use store::obj::space::{MAX_EXTENTS, Space};
use store::obj::{CreateParams, ops, types};
use store::{FormatParams, Store, StoreConfig};

const NOW: u64 = 1_767_225_600;
const ROWS: u32 = 1_000_000;
const BATCH: u32 = 2_000;

fn record(k: u32) -> Vec<u8> {
    let mut r = k.to_be_bytes().to_vec();
    let mut text = [ccsid::SPACE; 30];
    ccsid::encode_fixed(&format!("N{}", k % 9973), &mut text).unwrap();
    r.extend(text);
    r
}

#[test]
fn a_keyed_member_takes_a_million_records() {
    // 256 MiB of 4 KiB pages.
    let d = Arc::new(MemBlockDev::with_seed(65_536, 11));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 1024,
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
            name: name("QSTEST"),
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
    // Keyed on K, descending, so each record's key lands at the tree's
    // front and the tree takes pages between every growth of the space.
    let def = pf(&[
        rec("BIGREC", ""),
        fld("K", "9", "B", "0", ""),
        fld("N", "30", "A", "", ""),
        key("K", "DESCEND"),
    ]);
    let size = Size {
        initial: 0,
        increment: 0,
        max_increments: 0,
    };
    let id = file::create_pf(
        &mut t,
        lib,
        name("BIG"),
        &def,
        &CreateFileOpts {
            now: NOW,
            size,
            ..CreateFileOpts::default()
        },
    )
    .unwrap();
    t.commit().unwrap();

    let mut k = 0;
    while k < ROWS {
        let mut t = store.begin().unwrap();
        let mut m = Member::open_by(&mut t, id, MemberSpec::First).unwrap();
        for _ in 0..BATCH {
            k += 1;
            m.write_new(&mut t, &record(k), &[])
                .unwrap_or_else(|e| panic!("record {k}: {e}"));
        }
        drop(m);
        t.commit().unwrap();
    }

    let mut t = store.begin().unwrap();
    let m = Member::open_by(&mut t, id, MemberSpec::First).unwrap();
    assert_eq!(m.core.attr.record_count, u64::from(ROWS));
    let (space, _) = Space::load(&mut t, m.core.id).unwrap();
    assert!(
        space.extents.len() < MAX_EXTENTS / 4,
        "{} extents",
        space.extents.len()
    );
    // The first and last records read back by RRN.
    assert_eq!(m.read(&mut t, 1).unwrap().unwrap().record, record(1));
    assert_eq!(
        m.read(&mut t, u64::from(ROWS)).unwrap().unwrap().record,
        record(ROWS)
    );
    t.abort();
}
