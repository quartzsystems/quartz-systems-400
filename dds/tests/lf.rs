// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The LF compiler end to end: subsets, derivations, multi-format, join,
//! select/omit, errors, listing.

use dds::{
    CompOp, CompileOpts, DataType, FieldSource, KeyTransform, LfDefinition, Literal, MapRefs,
    Severity, Source, Validity, compile_lf, compile_pf,
};

fn at(parts: &[(usize, &str)]) -> String {
    let mut s = vec![' '; 80];
    for (col, text) in parts {
        for (i, ch) in text.chars().enumerate() {
            s[col - 1 + i] = ch;
        }
    }
    s.into_iter().collect::<String>().trim_end().to_string()
}

fn file(kw: &str) -> String {
    at(&[(6, "A"), (45, kw)])
}

fn rec(name: &str, kw: &str) -> String {
    at(&[(6, "A"), (17, "R"), (19, name), (45, kw)])
}

fn key(name: &str, kw: &str) -> String {
    at(&[(6, "A"), (17, "K"), (19, name), (45, kw)])
}

fn join(kw: &str) -> String {
    at(&[(6, "A"), (17, "J"), (45, kw)])
}

fn sel(name: &str, kw: &str) -> String {
    at(&[(6, "A"), (17, "S"), (19, name), (45, kw)])
}

fn omit(name: &str, kw: &str) -> String {
    at(&[(6, "A"), (17, "O"), (19, name), (45, kw)])
}

fn fld(name: &str, len: &str, ty: &str, dec: &str, kw: &str) -> String {
    let len_col = 35 - len.len();
    let dec_col = 38 - dec.len();
    at(&[
        (6, "A"),
        (19, name),
        (len_col, len),
        (35, ty),
        (dec_col, dec),
        (45, kw),
    ])
}

/// A library with CUST and ORDERS.
fn library() -> MapRefs {
    let mut refs = MapRefs::default();
    let cust = [
        rec("CUSREC", "TEXT('Customer')"),
        fld("CUSNO", "7", "P", "0", "COLHDG('Customer')"),
        fld("NAME", "30", "A", "", "TEXT('Name')"),
        fld("CITY", "20", "A", "", ""),
        fld("CUSTYP", "1", "A", "", ""),
        fld("STATUS", "1", "A", "", ""),
        fld("BAL", "9", "P", "2", ""),
        fld("SINCE", "", "L", "", ""),
        fld("ZIP", "5", "S", "0", ""),
        fld("ZIP4", "4", "S", "0", ""),
        key("CUSNO", ""),
    ];
    let orders = [
        rec("ORDREC", ""),
        fld("ORDNO", "7", "P", "0", ""),
        fld("OCUSNO", "7", "P", "0", ""),
        fld("OAMT", "9", "P", "2", ""),
        fld("ODATE", "", "L", "", ""),
        fld("STATUS", "1", "A", "", ""),
        key("ORDNO", ""),
    ];
    for (name, src) in [("CUST", &cust[..]), ("ORDERS", &orders[..])] {
        let (s, _) = Source::from_text(&src.join("\n"));
        let c = compile_pf(&s, &refs, &CompileOpts::default());
        let def = c.definition.unwrap_or_else(|| panic!("{}", c.listing));
        refs.insert_pf("QGPL", name, &def);
    }
    refs
}

fn compile(lines: &[String], refs: &MapRefs) -> dds::Compilation<LfDefinition> {
    let (src, d) = Source::from_text(&lines.join("\n"));
    assert!(d.is_empty(), "{d:?}");
    compile_lf(&src, refs, &CompileOpts::default())
}

fn ok(c: dds::Compilation<LfDefinition>) -> LfDefinition {
    assert!(
        c.max_severity() < Some(Severity::Error),
        "unexpected messages:\n{}",
        c.listing
    );
    c.definition.expect("definition")
}

#[test]
fn whole_format_with_keys() {
    let refs = library();
    let src = [
        file("UNIQUE"),
        rec("CUSREC", "PFILE(QGPL/CUST)"),
        key("NAME", ""),
        key("CUSNO", "DESCEND"),
    ];
    let def = ok(compile(&src, &refs));
    assert_eq!(def.formats.len(), 1);
    let f = &def.formats[0];
    assert_eq!(f.format.fields.len(), 9);
    assert_eq!(f.format.record_len, 4 + 30 + 20 + 1 + 1 + 5 + 10 + 5 + 4);
    assert_eq!(f.based_on[0].to_string(), "QGPL/CUST");
    assert_eq!(f.base_formats[0].to_string(), "CUSREC");
    assert!(
        f.sources
            .iter()
            .all(|s| matches!(s, FieldSource::Base { file: 0, .. }))
    );
    assert_eq!(f.format.keys.len(), 2);
    assert_eq!(f.format.keys[0].name.to_string(), "NAME");
    assert!(f.format.keys[1].descend);
    assert!(def.is_keyed());
    assert!(!def.is_join());
    assert_eq!(def.unique, Some(dds::NullKeys::Include));
}

#[test]
fn subset_rename_concat_sst_and_overrides() {
    let refs = library();
    let src = [
        rec("CUSSUB", "PFILE(QGPL/CUST) TEXT('Subset')"),
        fld("CUSNO", "", "", "", ""),
        fld("CNAME", "", "", "", "RENAME(NAME) +"),
        file("COLHDG('Customer' 'Name')"),
        fld("WHERE", "", "", "", "CONCAT(CITY CUSTYP)"),
        fld("FULLZIP", "", "", "", "CONCAT(ZIP ZIP4)"),
        fld("INIT", "", "", "", "SST(NAME 1 3)"),
        fld("REST", "", "", "", "SST(NAME 11)"),
        fld("ZBAL", "", "S", "", "RENAME(BAL)"),
        key("CNAME", ""),
        key("FULLZIP", "ABSVAL"),
    ];
    let def = ok(compile(&src, &refs));
    let f = &def.formats[0];
    let shapes: Vec<String> = f.format.fields.iter().map(dds::FieldDef::shape).collect();
    assert_eq!(
        shapes,
        ["P 7,0", "A 30", "A 21", "S 9,0", "A 3", "A 20", "S 9,2"]
    );
    assert_eq!(f.format.record_len, 4 + 30 + 21 + 9 + 3 + 20 + 9);
    let cname = &f.format.fields[1];
    assert_eq!(cname.text.as_deref(), Some("Name"));
    assert_eq!(cname.colhdg, ["Customer", "Name"]);
    assert_eq!(
        f.sources[1],
        FieldSource::Base {
            file: 0,
            field: "NAME".parse().unwrap()
        }
    );
    assert!(matches!(&f.sources[2], FieldSource::Concat { file: 0, fields } if fields.len() == 2));
    assert!(matches!(
        &f.sources[4],
        FieldSource::Substring {
            start: 1,
            length: 3,
            ..
        }
    ));
    assert!(matches!(
        &f.sources[5],
        FieldSource::Substring {
            start: 11,
            length: 20,
            ..
        }
    ));
    assert_eq!(f.format.fields[6].data_type, DataType::Zoned);
    assert_eq!(f.format.keys[1].transform, KeyTransform::Absval);
    assert!(def.text.is_none());
}

#[test]
fn multi_format_and_arrival() {
    let refs = library();
    let src = [
        rec("CUSREC", "PFILE(QGPL/CUST)"),
        key("CUSNO", ""),
        rec("ORDREC", "PFILE(QGPL/ORDERS)"),
        key("OCUSNO", ""),
    ];
    let def = ok(compile(&src, &refs));
    assert_eq!(def.formats.len(), 2);
    assert_eq!(def.formats[1].format.fields.len(), 5);
    assert_eq!(def.formats[1].format.keys[0].name.to_string(), "OCUSNO");

    let src = [
        rec("CUSREC", "PFILE(QGPL/CUST)"),
        key("CUSNO", ""),
        rec("ORDREC", "PFILE(QGPL/ORDERS)"),
    ];
    let c = compile(&src, &refs);
    assert_eq!(c.ids(), ["DDS4017"]);

    let src = [
        rec("CUSREC", "PFILE(QGPL/CUST)"),
        key("*NONE", ""),
        rec("ORDREC", "PFILE(QGPL/ORDERS)"),
        key("*NONE", ""),
    ];
    let def = ok(compile(&src, &refs));
    assert!(def.formats.iter().all(|f| f.arrival));
    assert!(!def.is_keyed());

    let src = [
        rec("CUSREC", "PFILE(QGPL/CUST)"),
        key("*NONE", ""),
        key("CUSNO", ""),
    ];
    assert_eq!(compile(&src, &refs).ids(), ["DDS4019"]);
}

#[test]
fn join_logical_file() {
    let refs = library();
    let src = [
        file("JDFTVAL"),
        rec("CUSORD", "JFILE(QGPL/CUST QGPL/ORDERS)"),
        join("JOIN(1 2) JFLD(CUSNO OCUSNO) +"),
        file("JDUPSEQ(ODATE *DESCEND)"),
        fld("CUSNO", "", "", "", ""),
        fld("NAME", "", "", "", ""),
        fld("ORDNO", "", "", "", ""),
        fld("OAMT", "", "", "", "JREF(2)"),
        fld("OSTAT", "", "", "", "RENAME(STATUS) JREF(ORDERS)"),
        key("CUSNO", ""),
        sel("OSTAT", "COMP(NE 'X')"),
    ];
    let def = ok(compile(&src, &refs));
    assert!(def.is_join());
    assert!(def.jdftval);
    let f = &def.formats[0];
    assert_eq!(f.joins.len(), 1);
    let j = &f.joins[0];
    assert_eq!((j.from, j.to), (0, 1));
    assert_eq!(j.fields[0].0.to_string(), "CUSNO");
    assert_eq!(j.fields[0].1.to_string(), "OCUSNO");
    assert!(j.dupseq.as_ref().unwrap().1);
    let files: Vec<usize> = f
        .sources
        .iter()
        .map(|s| {
            if let FieldSource::Base { file, .. } = s {
                *file
            } else {
                99
            }
        })
        .collect();
    assert_eq!(files, [0, 0, 1, 1, 1]);
    assert_eq!(f.format.keys[0].name.to_string(), "CUSNO");
    assert_eq!(f.select_omit.len(), 1);

    let bad = [
        rec("CUSORD", "JFILE(QGPL/CUST QGPL/ORDERS)"),
        join("JFLD(CUSNO OCUSNO)"),
        fld("STATUS", "", "", "", ""),
        fld("NOPE", "", "", "", "JREF(3)"),
        fld("ORDNO", "", "", "", ""),
        key("ORDNO", ""),
    ];
    let c = compile(&bad, &refs);
    assert_eq!(c.ids(), ["DDS4007", "DDS4006", "DDS4018"]);

    let bad = [
        rec("ONE", "JFILE(QGPL/CUST)"),
        rec("TWO", "PFILE(QGPL/CUST)"),
        join("JFLD(CUSNO CUSNO)"),
        rec("THREE", "JFILE(QGPL/CUST QGPL/ORDERS +"),
        file("QGPL/CUST)"),
        join("JFLD(CUSNO OCUSNO)"),
    ];
    let c = compile(&bad, &refs);
    assert_eq!(
        c.ids(),
        [
            "DDS4004", "DDS4001", "DDS4005", "DDS4025", "DDS4021", "DDS4013"
        ]
    );
}

#[test]
fn select_omit_groups() {
    let refs = library();
    let src = [
        file("DYNSLT"),
        rec("ACTIVE", "PFILE(QGPL/CUST)"),
        key("CUSNO", ""),
        sel("CUSTYP", "VALUES('A' 'B')"),
        fld("BAL", "", "", "", "COMP(GT 0)"),
        omit("STATUS", "COMP(EQ 'D')"),
        sel("ZIP", "RANGE(10000 99999)"),
        omit("", "ALL"),
    ];
    let def = ok(compile(&src, &refs));
    assert!(def.dynslt);
    let g = &def.formats[0].select_omit;
    assert_eq!(g.len(), 4);
    assert!(g[0].select);
    assert_eq!(g[0].conditions.len(), 2);
    assert_eq!(
        g[0].conditions[0].test,
        Validity::Values(vec![
            Literal::Quoted("A".into()),
            Literal::Quoted("B".into())
        ])
    );
    assert_eq!(
        g[0].conditions[1].test,
        Validity::Comp(CompOp::Gt, Literal::Word("0".into()))
    );
    assert!(!g[1].select);
    assert_eq!(
        g[2].conditions[0].test,
        Validity::Range(Literal::Word("10000".into()), Literal::Word("99999".into()))
    );
    assert!(g[3].all && !g[3].select);

    let bad = [
        rec("ACTIVE", "PFILE(QGPL/CUST)"),
        sel("NOPE", "COMP(EQ 'A')"),
        fld("CUSNO", "", "", "", "COMP(EQ 1)"),
        sel("CUSTYP", ""),
        sel("CUSTYP", "ALL"),
        sel("CUSTYP", "VALUES(1 2)"),
        key("CUSNO", ""),
    ];
    let c = compile(&bad, &refs);
    assert_eq!(
        c.ids(),
        [
            "DDS4009", "DDS4012", "DDS4010", "DDS4011", "DDS0206", "DDS4010", "DDS4022"
        ]
    );
}

#[test]
fn errors() {
    let refs = library();
    let src = [
        rec("NOBASE", ""),
        rec("BOTH", "PFILE(QGPL/CUST) +"),
        file("JFILE(QGPL/CUST QGPL/ORDERS)"),
        fld("NOPE", "", "", "", ""),
        fld("SHORT", "", "", "", "SST(NAME 25 10)"),
        fld("DATES", "", "", "", "CONCAT(NAME SINCE)"),
        fld("TWO", "", "", "", "RENAME(NAME) SST(NAME 1 2)"),
        fld("REF", "", "", "", "REFFLD(NAME)"),
        rec("GONE", "PFILE(QGPL/GONE)"),
        fld("X", "", "", "", ""),
        rec("FMT", "PFILE(QGPL/CUST) FORMAT(QGPL/ORDERS)"),
        fld("CUSNO", "", "", "", ""),
    ];
    let c = compile(&src, &refs);
    assert!(c.definition.is_none());
    let ids = c.ids();
    for want in [
        "DDS4001", "DDS0210", "DDS4003", "DDS4015", "DDS4016", "DDS0210", "DDS0207", "DDS3002",
        "DDS1022", "DDS3005", "DDS1027",
    ] {
        assert!(ids.contains(&want), "missing {want} in {ids:?}");
    }
}

#[test]
fn listing_describes_the_logical_file() {
    let refs = library();
    let src = [
        rec("CUSSUB", "PFILE(QGPL/CUST)"),
        fld("CUSNO", "", "", "", ""),
        fld("CNAME", "", "", "", "RENAME(NAME)"),
        fld("WHERE", "", "", "", "CONCAT(CITY CUSTYP)"),
        fld("CUSTYP", "", "", "", ""),
        key("CUSNO", ""),
        sel("CUSTYP", "COMP(EQ 'A')"),
    ];
    let c = compile(&src, &refs);
    let l = &c.listing;
    assert!(c.definition.is_some(), "{l}");
    assert!(
        l.contains("Type . . . . . . . . . . . :  Logical file (LF)"),
        "{l}"
    );
    assert!(l.contains("  Based on 1 QGPL/CUST (CUSREC)"), "{l}");
    assert!(l.contains("  CNAME renames NAME of file 1"), "{l}");
    assert!(l.contains("  WHERE = CONCAT(CITY CUSTYP) of file 1"), "{l}");
    assert!(l.contains("  Select CUSTYP COMP(EQ 'A')"), "{l}");
    assert!(
        l.contains("       *       A            CNAME         30A         REFFLD(NAME QGPL/CUST)"),
        "{l}"
    );
    assert!(
        l.contains("Access path: duplicates allowed; key length 4"),
        "{l}"
    );
    assert!(l.lines().all(|line| line.chars().count() <= 132), "{l}");
}
