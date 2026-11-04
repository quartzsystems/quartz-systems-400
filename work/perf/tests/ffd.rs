// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped `QAPM*` model files against the primary's: each is
//! created from its DDS in a store in memory and our `DSPFFD` is diffed,
//! field for field, with `DSPFFD OUTPUT(*OUTFILE)` captured on the
//! V7R1M0 oracle (`tools/conformance/oracle/perf/qapm-ffd.txt`): the
//! format name, the record length, and each field's name, type, buffer
//! position, length in bytes, digits and decimals, in order.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use blockdev::{BlockDev, MemBlockDev};
use qsname::Name;
use store::obj::{CreateParams, ops, types};
use store::{FormatParams, Store, StoreConfig};

/// One captured field: name, type letter, position, bytes, digits, decimals.
type Fld = (String, char, u32, u32, u32, u32);

/// The capture, by file: format, record length, fields in order.
#[allow(clippy::many_single_char_names)] // one per column
fn oracle() -> BTreeMap<String, (String, u32, Vec<Fld>)> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/oracle/perf/qapm-ffd.txt");
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let mut out: BTreeMap<String, (String, u32, Vec<Fld>)> = BTreeMap::new();
    for l in text.lines().map(str::trim_end) {
        if l.starts_with('#') || l.is_empty() {
            continue;
        }
        let f: Vec<&str> = l.split('|').collect();
        let n = |i: usize| f[i].trim().parse::<u32>().unwrap();
        // QADSPFFD writes a hex field's type as 5.
        let t = match f[8] {
            "5" => 'H',
            t => t.chars().next().unwrap(),
        };
        let e = out
            .entry(f[0].to_string())
            .or_insert_with(|| (f[1].to_string(), n(2), Vec::new()));
        e.2.push((f[3].to_string(), t, n(4), n(5), n(6), n(7)));
    }
    out
}

#[test]
fn qapm_files_match_the_oracle() {
    let d = Arc::new(MemBlockDev::with_seed(16384, 7));
    let params = FormatParams {
        asp_number: 1,
        asp_uuid: [4; 16],
        unit_uuids: vec![[5; 16]],
        unit_serials: Vec::new(),
        wal_pages: 128,
        now: 1_767_225_600,
    };
    let mut store = Store::format(vec![Box::new(d) as Box<dyn BlockDev>], &params, StoreConfig::default()).unwrap();
    let mut t = store.begin().unwrap();
    let qsys = ops::bootstrap(&mut t, 1_767_225_600, "QS400").unwrap();
    let lib = ops::create(
        &mut t,
        &CreateParams {
            library: qsys,
            name: Name::parse("QSTEST").unwrap(),
            type_code: types::LIB,
            attribute: String::new(),
            text: String::new(),
            owner: "QSECOFR".into(),
            system: "QS400".into(),
            now: 1_767_225_600,
            by_system: false,
        },
    )
    .unwrap();
    let want = oracle();
    assert_eq!(
        want.keys().collect::<Vec<_>>(),
        perf::qapm::FILES.iter().map(|(n, _)| n).collect::<Vec<_>>(),
        "the shipped files are the captured ones"
    );
    let mut diffs = Vec::new();
    for (file, (format, reclen, fields)) in &want {
        let id = perf::qapm::create(&mut t, lib, file, 1).unwrap();
        let ffd = db::dsp::dspffd(&mut t, id).unwrap();
        assert_eq!(ffd.formats.len(), 1, "{file}");
        let f = &ffd.formats[0];
        if f.name.to_string() != *format || f.record_len != *reclen {
            diffs.push(format!("{file}: format {} {} want {format} {reclen}", f.name, f.record_len));
        }
        if f.fields.len() != fields.len() {
            diffs.push(format!("{file}: {} fields, want {}", f.fields.len(), fields.len()));
        }
        for (got, w) in f.fields.iter().zip(fields) {
            let numeric = matches!(w.1, 'P' | 'S' | 'B');
            let ours = (
                got.name.to_string(),
                got.data_type,
                got.position,
                got.byte_len,
                if numeric { got.length } else { 0 },
                u32::from(got.decimals),
            );
            if &ours != w {
                diffs.push(format!("{file}: got {ours:?}\n          want {w:?}"));
            }
        }
    }
    assert!(diffs.is_empty(), "{}", diffs.join("\n"));
}
