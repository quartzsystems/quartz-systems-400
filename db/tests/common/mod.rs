// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Shared test setup: a store with QSYS and QSTEST, DDS helpers, and
//! record builders for the customer file.

#![allow(dead_code)]

use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use dds::{
    CompileOpts, LfDefinition, MapRefs, NoRefs, PfDefinition, Source, compile_lf, compile_pf,
};
use qsname::Name;
use store::obj::{CreateParams, ObjectId, ops, types};
use store::{FormatParams, Store, StoreConfig};

pub fn name(s: &str) -> Name {
    Name::parse(s).unwrap()
}

pub fn open_new() -> (Arc<MemBlockDev>, Store, ObjectId) {
    let d = Arc::new(MemBlockDev::with_seed(16384, 7));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: 1_767_225_600,
    };
    let mut store = Store::format(
        vec![Box::new(d.clone()) as Box<dyn BlockDev>],
        &params,
        StoreConfig::default(),
    )
    .unwrap();
    let lib = {
        let mut t = store.begin().unwrap();
        let qsys = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
        let lib = ops::create(
            &mut t,
            &CreateParams {
                library: qsys,
                name: name("QSTEST"),
                type_code: types::LIB,
                attribute: String::new(),
                text: "Test library".into(),
                owner: "QSECOFR".into(),
                system: "QS400".into(),
                now: 1_767_225_600,
                by_system: false,
            },
        )
        .unwrap();
        t.commit().unwrap();
        lib
    };
    (d, store, lib)
}

pub fn reopen(d: &Arc<MemBlockDev>) -> Store {
    Store::open(
        vec![Box::new(d.clone()) as Box<dyn BlockDev>],
        StoreConfig::default(),
    )
    .unwrap()
    .0
}

pub fn at(parts: &[(usize, &str)]) -> String {
    let mut s = vec![' '; 80];
    for (col, text) in parts {
        for (i, ch) in text.chars().enumerate() {
            s[col - 1 + i] = ch;
        }
    }
    s.into_iter().collect::<String>().trim_end().to_string()
}

pub fn file_kw(kw: &str) -> String {
    at(&[(6, "A"), (45, kw)])
}

pub fn rec(name: &str, kw: &str) -> String {
    at(&[(6, "A"), (17, "R"), (19, name), (45, kw)])
}

pub fn key(name: &str, kw: &str) -> String {
    at(&[(6, "A"), (17, "K"), (19, name), (45, kw)])
}

pub fn sel(name: &str, kw: &str) -> String {
    at(&[(6, "A"), (17, "S"), (19, name), (45, kw)])
}

pub fn omit(name: &str, kw: &str) -> String {
    at(&[(6, "A"), (17, "O"), (19, name), (45, kw)])
}

pub fn fld(name: &str, len: &str, ty: &str, dec: &str, kw: &str) -> String {
    at(&[
        (6, "A"),
        (19, name),
        (35 - len.len(), len),
        (35, ty),
        (38 - dec.len(), dec),
        (45, kw),
    ])
}

pub fn pf(lines: &[String]) -> PfDefinition {
    let (source, _) = Source::from_text(&lines.join("\n"));
    let c = compile_pf(&source, &NoRefs, &CompileOpts::default());
    c.definition.unwrap_or_else(|| panic!("{}", c.listing))
}

pub fn lf(lines: &[String], refs: &MapRefs) -> LfDefinition {
    let (source, _) = Source::from_text(&lines.join("\n"));
    let c = compile_lf(&source, refs, &CompileOpts::default());
    c.definition.unwrap_or_else(|| panic!("{}", c.listing))
}

/// A customer file: packed key, text, a null-capable date, a varlen note.
pub fn customer_def() -> PfDefinition {
    pf(&[
        file_kw("UNIQUE"),
        rec("CUSREC", "TEXT('Customer')"),
        fld("CUSNO", "7", "P", "0", "COLHDG('Customer')"),
        fld("NAME", "30", "A", "", "TEXT('Name')"),
        fld("BAL", "9", "P", "2", "DFT(0)"),
        fld("SINCE", "", "L", "", "ALWNULL"),
        fld("NOTE", "100", "A", "", "VARLEN(20) ALWNULL"),
        key("CUSNO", ""),
    ])
}

/// A customer record: `BAL` 12.50, `SINCE` 2026-10-10, `NOTE` "hello".
pub fn record(cusno: i64, who: &str) -> Vec<u8> {
    record_with(cusno, who, "12.50")
}

pub fn record_with(cusno: i64, who: &str, bal: &str) -> Vec<u8> {
    let mut r = Vec::new();
    let p = qdecimal::Packed::new(7, 0).unwrap();
    r.extend(
        p.encode(
            &qdecimal::Decimal::new(i128::from(cusno), 0).unwrap(),
            qdecimal::Rounding::Truncate,
        )
        .unwrap(),
    );
    let mut nm = [ccsid::SPACE; 30];
    ccsid::encode_fixed(who, &mut nm).unwrap();
    r.extend(nm);
    r.extend(
        qdecimal::Packed::new(9, 2)
            .unwrap()
            .encode(
                &qdecimal::Decimal::parse(bal).unwrap(),
                qdecimal::Rounding::Truncate,
            )
            .unwrap(),
    );
    r.extend(ccsid::to_ebcdic("2026-10-10").unwrap());
    let note = ccsid::to_ebcdic("hello").unwrap();
    r.extend(u16::try_from(note.len()).unwrap().to_be_bytes());
    r.extend(&note);
    r.extend(std::iter::repeat_n(ccsid::SPACE, 100 - note.len()));
    r
}
