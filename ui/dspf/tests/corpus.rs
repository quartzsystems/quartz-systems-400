// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The corpus under `tests/dds`: every display file compiles, its
//! listing is compared with `tests/expected/<name>.lst`, and a file
//! that compiles round-trips through the payload. `QS400_UPDATE_DSPF=1`
//! rewrites the expected listings after a deliberate change.

use std::fs;
use std::path::{Path, PathBuf};

use dds::{DataType, FieldDef, RecordFormat};
use dspf::{CompileOpts, MapRefs, Source, compile_dspf, decode, encode};
use qsname::Name;

fn dir(sub: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(sub)
}

/// `QGPL/CUSTMAST` for `REF` and `REFFLD`.
fn refs() -> MapRefs {
    let mut fmt = RecordFormat::new(Name::parse("CUSTREC").unwrap());
    fmt.fields.push(FieldDef::new(
        Name::parse("CUSNO").unwrap(),
        DataType::Packed,
        7,
        0,
    ));
    fmt.fields.push(FieldDef::new(
        Name::parse("CUSNM").unwrap(),
        DataType::Char,
        30,
        0,
    ));
    fmt.lay_out();
    let mut m = MapRefs::default();
    m.insert(
        Name::parse("QGPL").unwrap(),
        Name::parse("CUSTMAST").unwrap(),
        vec![fmt],
    );
    m
}

fn cases() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = fs::read_dir(dir("dds"))
        .expect("tests/dds")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "dspf"))
        .map(|p| {
            let name = p.file_stem().unwrap().to_string_lossy().to_string();
            (name, fs::read_to_string(&p).unwrap())
        })
        .collect();
    v.sort();
    v
}

#[test]
fn corpus_matches_expected() {
    let update = std::env::var_os("QS400_UPDATE_DSPF").is_some();
    let expected = dir("expected");
    let refs = refs();
    let mut mismatched = Vec::new();
    let cases = cases();
    assert!(cases.len() >= 7, "the corpus holds {} files", cases.len());
    for (name, text) in &cases {
        let (source, diags) = Source::from_text(text);
        assert!(diags.is_empty(), "{name}: source text {diags:?}");
        let upper = name.to_ascii_uppercase();
        let opts = CompileOpts {
            file: Some(format!("QGPL/{upper}")),
            source: Some(format!("QGPL/QDDSSRC({upper})")),
            timestamp: None,
        };
        let c = compile_dspf(&source, &refs, &opts);
        if name.starts_with("errors") {
            assert!(c.definition.is_none(), "{name} must not compile");
        } else {
            let df = c
                .definition
                .as_ref()
                .unwrap_or_else(|| panic!("{name} did not compile:\n{}", c.listing));
            let back = decode(&encode(df)).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(&back, df, "{name}: payload round trip");
        }
        let path = expected.join(format!("{name}.lst"));
        if update {
            fs::create_dir_all(&expected).unwrap();
            fs::write(&path, &c.listing).unwrap();
            continue;
        }
        let want = fs::read_to_string(&path).unwrap_or_default();
        if want.replace("\r\n", "\n") != c.listing {
            mismatched.push(name.clone());
            eprintln!("---- {name}: ours ----\n{}", c.listing);
        }
    }
    assert!(
        mismatched.is_empty(),
        "listings differ from tests/expected: {mismatched:?}; QS400_UPDATE_DSPF=1 cargo test -p dspf --test corpus rewrites them"
    );
}

#[test]
fn errors_name_every_rule() {
    let text = fs::read_to_string(dir("dds").join("errors.dspf")).unwrap();
    let (source, _) = Source::from_text(&text);
    let c = compile_dspf(&source, &refs(), &CompileOpts::default());
    let ids = c.ids();
    for want in [
        "DDS6303", "DDS6205", "DDS0209", "DDS0207", "DDS6509", "DDS6508", "DDS6517", "DDS6106",
        "DDS6105", "DDS6514", "DDS6507", "DDS6513", "DDS6501", "DDS6101", "DDS6107", "DDS6401",
        "DDS6402", "DDS6404", "DDS6408",
    ] {
        assert!(ids.contains(&want), "{want} missing from {ids:?}");
    }
    assert_eq!(c.max_severity(), Some(dspf::Severity::Error));
}
