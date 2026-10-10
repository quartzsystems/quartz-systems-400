// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PF compiler end to end: shapes, keys, references, errors, listing.

use dds::{
    CompileOpts, DataType, DateFmt, DefaultValue, DupOrder, EditCode, FloatPrecision, KeyTransform,
    Literal, MapRefs, NoRefs, NullKeys, PfDefinition, RefSource, Severity, Source, TimeFmt, Usage,
    Validity, compile_pf,
};

/// A record from (1-based column, text) pairs.
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

fn fref(name: &str, len: &str, ty: &str, kw: &str) -> String {
    let len_col = 35 - len.len();
    at(&[
        (6, "A"),
        (19, name),
        (29, "R"),
        (len_col, len),
        (35, ty),
        (45, kw),
    ])
}

fn compile(lines: &[String]) -> dds::Compilation<PfDefinition> {
    let (src, d) = Source::from_text(&lines.join("\n"));
    assert!(d.is_empty(), "{d:?}");
    compile_pf(&src, &NoRefs, &CompileOpts::default())
}

fn compile_with(lines: &[String], refs: &MapRefs) -> dds::Compilation<PfDefinition> {
    let (src, d) = Source::from_text(&lines.join("\n"));
    assert!(d.is_empty(), "{d:?}");
    compile_pf(&src, refs, &CompileOpts::default())
}

fn ok(c: dds::Compilation<PfDefinition>) -> PfDefinition {
    assert!(
        c.max_severity() < Some(Severity::Error),
        "unexpected messages:\n{}",
        c.listing
    );
    c.definition.expect("definition")
}

fn every_type_source() -> Vec<String> {
    let src = [
        at(&[(6, "A"), (7, "* Every database type")]),
        file("UNIQUE(*EXCNULL)"),
        rec("ALLREC", "TEXT('Every type')"),
        fld("CHR", "30", "A", "", "VARLEN(10) CCSID(37) +"),
        file("       TEXT('Character')"),
        fld("HEX", "4", "H", "", ""),
        fld("GRA", "5", "G", "", ""),
        fld("OPN", "8", "O", "", "ALWNULL"),
        fld("ZON", "9", "S", "2", "COLHDG('Zoned' 'Amount' 'Here')"),
        fld("PKD", "7", "P", "0", "EDTCDE(J *CR)"),
        fld("DFL", "7", "", "2", "DFT(12345.67)"),
        fld("DFA", "3", "", "", "DFT('abc')"),
        fld("BIN2", "4", "B", "0", ""),
        fld("BIN4", "9", "B", "0", ""),
        fld("BIN8", "18", "B", "0", ""),
        fld("FLT4", "9", "F", "2", ""),
        fld("FLT8", "17", "F", "4", "FLTPCN(*DOUBLE)"),
        fld("DTE", "", "L", "", "DATFMT(*MDY) DATSEP('-')"),
        fld("DTI", "", "L", "", ""),
        fld("TIM", "", "T", "", ""),
        fld("TIH", "", "T", "", "TIMFMT(*HMS)"),
        fld("TMS", "", "Z", "", "ALWNULL DFT(*NULL)"),
        fld("BOTH", "1", "A", "", ""),
        key("PKD", "DESCEND ABSVAL"),
        key("CHR", "DIGIT"),
        key("DTE", ""),
    ];
    let mut src = src.to_vec();
    src[22] = at(&[(6, "A"), (19, "BOTH"), (34, "1"), (35, "A"), (38, "B")]);
    src
}

#[test]
fn every_field_type_lays_out() {
    let src = every_type_source();
    let def = ok(compile(&src));
    let f = &def.format;
    assert_eq!(f.name.to_string(), "ALLREC");
    assert_eq!(f.text.as_deref(), Some("Every type"));
    let shapes: Vec<(String, u32, u32)> = f
        .fields
        .iter()
        .map(|f| (f.shape(), f.byte_len, f.offset))
        .collect();
    assert_eq!(
        shapes,
        vec![
            ("A 30".to_string(), 32, 0),
            ("H 4".to_string(), 4, 32),
            ("G 5".to_string(), 10, 36),
            ("O 8".to_string(), 8, 46),
            ("S 9,2".to_string(), 9, 54),
            ("P 7,0".to_string(), 4, 63),
            ("P 7,2".to_string(), 4, 67),
            ("A 3".to_string(), 3, 71),
            ("B 4,0".to_string(), 2, 74),
            ("B 9,0".to_string(), 4, 76),
            ("B 18,0".to_string(), 8, 80),
            ("F 9,2".to_string(), 4, 88),
            ("F 17,4".to_string(), 8, 92),
            ("L *MDY".to_string(), 8, 100),
            ("L *ISO".to_string(), 10, 108),
            ("T *ISO".to_string(), 8, 118),
            ("T *HMS".to_string(), 8, 126),
            ("Z 26".to_string(), 26, 134),
            ("A 1".to_string(), 1, 160),
        ]
    );
    assert_eq!(f.record_len, 161);
    let chr = &f.fields[0];
    assert_eq!(chr.varlen.unwrap().allocated, Some(10));
    assert_eq!(chr.ccsid, Some(37));
    assert_eq!(chr.text.as_deref(), Some("Character"));
    assert!(f.fields[3].allow_null);
    assert_eq!(f.fields[4].colhdg, vec!["Zoned", "Amount", "Here"]);
    assert_eq!(
        f.fields[5].edit_code,
        Some(EditCode {
            code: 'J',
            modifier: Some("*CR".into())
        })
    );
    assert_eq!(
        f.fields[6].default,
        Some(DefaultValue::Number("12345.67".into()))
    );
    assert_eq!(
        f.fields[7].default,
        Some(DefaultValue::Quoted("abc".into()))
    );
    assert_eq!(f.fields[11].precision, FloatPrecision::Single);
    assert_eq!(f.fields[12].precision, FloatPrecision::Double);
    assert_eq!(f.fields[13].date_fmt, Some(DateFmt::Mdy));
    assert_eq!(f.fields[13].date_sep, Some('-'));
    assert_eq!(f.fields[14].date_sep, Some('-'));
    assert_eq!(f.fields[15].time_fmt, Some(TimeFmt::Iso));
    assert_eq!(f.fields[15].time_sep, Some('.'));
    assert_eq!(f.fields[16].time_sep, Some(':'));
    assert_eq!(f.fields[17].default, Some(DefaultValue::Null));
    assert_eq!(f.fields[18].usage, Some(Usage::Both));
    assert_eq!(def.unique, Some(NullKeys::Exclude));
    assert_eq!(f.keys.len(), 3);
    assert_eq!(f.keys[0].name.to_string(), "PKD");
    assert_eq!(f.keys[0].field, 5);
    assert!(f.keys[0].descend);
    assert_eq!(f.keys[0].transform, KeyTransform::Absval);
    assert_eq!(f.keys[1].transform, KeyTransform::Digit);
    assert_eq!(f.keys[2].transform, KeyTransform::Unsigned);
    assert_eq!(f.key_len(), 4 + 30 + 8);
    assert!(def.is_keyed());
}

fn reference_library() -> MapRefs {
    let src = [
        rec("REFREC", ""),
        fld("NAME", "30", "A", "", "TEXT('A name') COLHDG('Name') +"),
        file("CHECK(ME)"),
        fld("AMT", "9", "P", "2", "EDTCDE(J) COLHDG('Amount') +"),
        file("RANGE(0 100)"),
        fld("WHEN", "", "L", "", "DATFMT(*USA)"),
        key("NAME", "DESCEND"),
        key("AMT", "ABSVAL"),
    ];
    let def = ok(compile(&src));
    let mut refs = MapRefs::default();
    refs.insert_pf("QGPL", "REFS", &def);
    refs
}

#[test]
fn references_copy_and_override() {
    let refs = reference_library();
    let src = [
        file("REF(QGPL/REFS)"),
        rec("CUSREC", ""),
        fref("NAME", "", "", ""),
        fref("CUST", "", "", "REFFLD(NAME) COLHDG('Customer')"),
        fref("AMT2", "11", "", "REFFLD(REFREC/AMT) +"),
        file("EDTWRD('   ,   ,   0.  ')"),
        fref("WHEN2", "", "", "REFFLD(WHEN QGPL/REFS) DATFMT(*ISO)"),
        fref("CUST2", "", "", "REFFLD(CUST *SRC) VALUES('A' 'B')"),
        fref("ZNAME", "10", "S", "REFFLD(NAME *SRC)"),
    ];
    let def = ok(compile_with(&src, &refs));
    let f = &def.format;
    assert_eq!(f.fields.len(), 6);
    let name = &f.fields[0];
    assert_eq!(name.shape(), "A 30");
    assert_eq!(name.text.as_deref(), Some("A name"));
    assert_eq!(name.validity, vec![Validity::Check(vec!["ME".into()])]);
    let r = name.reference.as_ref().unwrap();
    assert_eq!(r.field.to_string(), "NAME");
    assert_eq!(r.format.unwrap().to_string(), "REFREC");
    assert!(matches!(&r.source, RefSource::File(fr) if fr.to_string() == "QGPL/REFS"));
    let cust = &f.fields[1];
    assert_eq!(cust.name.to_string(), "CUST");
    assert_eq!(cust.shape(), "A 30");
    assert_eq!(cust.colhdg, vec!["Customer"]);
    assert_eq!(cust.text.as_deref(), Some("A name"));
    let amt2 = &f.fields[2];
    assert_eq!(amt2.shape(), "P 11,2");
    assert_eq!(amt2.byte_len, 6);
    assert_eq!(amt2.edit_code, None);
    assert!(amt2.edit_word.is_some());
    assert_eq!(
        amt2.validity,
        vec![Validity::Range(
            Literal::Word("0".into()),
            Literal::Word("100".into())
        )]
    );
    let when2 = &f.fields[3];
    assert_eq!(when2.date_fmt, Some(DateFmt::Iso));
    assert_eq!(when2.length, 10);
    let cust2 = &f.fields[4];
    assert_eq!(cust2.colhdg, vec!["Customer"]);
    assert!(matches!(
        cust2.reference.as_ref().unwrap().source,
        RefSource::Source
    ));
    assert_eq!(
        cust2.validity,
        vec![Validity::Values(vec![
            Literal::Quoted("A".into()),
            Literal::Quoted("B".into())
        ])]
    );
    let zname = &f.fields[5];
    assert_eq!(zname.data_type, DataType::Zoned);
    assert_eq!(zname.shape(), "S 10,0");
    assert_eq!(def.reference.as_ref().unwrap().to_string(), "QGPL/REFS");
    assert_eq!(f.record_len, 30 + 30 + 6 + 10 + 30 + 10);
}

#[test]
fn format_and_refaccpth_copy_whole_pieces() {
    let refs = reference_library();
    let src = [
        file("REFACCPTH(QGPL/REFS)"),
        rec("REFREC", "FORMAT(QGPL/REFS)"),
    ];
    let def = ok(compile_with(&src, &refs));
    let f = &def.format;
    assert_eq!(f.fields.len(), 3);
    assert_eq!(f.from.as_ref().unwrap().to_string(), "QGPL/REFS");
    assert_eq!(f.record_len, 30 + 5 + 10);
    assert_eq!(f.keys.len(), 2);
    assert!(f.keys[0].descend);
    assert_eq!(f.keys[1].transform, KeyTransform::Absval);
    assert_eq!(f.keys[1].field, 1);
    assert_eq!(def.refaccpth.as_ref().unwrap().to_string(), "QGPL/REFS");

    let src = [
        file("REFACCPTH(QGPL/REFS)"),
        rec("OTHER", "FORMAT(QGPL/REFS)"),
        fld("X", "1", "A", "", ""),
        key("X", ""),
    ];
    let c = compile_with(&src, &refs);
    assert!(c.definition.is_none());
    let ids = c.ids();
    assert!(ids.contains(&"DDS3005"), "{ids:?}");
    assert!(ids.contains(&"DDS1027"), "{ids:?}");
    assert!(ids.contains(&"DDS1028"), "{ids:?}");
    assert!(ids.contains(&"DDS1008"), "{ids:?}");
}

#[test]
fn reference_errors() {
    let refs = reference_library();
    let src = [
        rec("R1", ""),
        fref("NAME", "", "", ""),
        fref("NOPE", "", "", "REFFLD(NOPE QGPL/REFS)"),
        fref("NOFILE", "", "", "REFFLD(NAME QGPL/GONE)"),
        fref("NOFMT", "", "", "REFFLD(BAD/NAME QGPL/REFS)"),
        fref("NOSRC", "", "", "REFFLD(ZZZ *SRC)"),
    ];
    let c = compile_with(&src, &refs);
    assert!(c.definition.is_none());
    assert_eq!(
        c.ids(),
        [
            "DDS3003", "DDS3001", "DDS3002", "DDS3005", "DDS3006", "DDS1031"
        ]
    );
}

#[test]
fn semantic_errors_block_the_definition() {
    let src = [
        file("UNIQUE FIFO TEXT('File text') BOGUS"),
        rec("R1", "DESCEND"),
        fld("A1", "5", "A", "", ""),
        fld("A1", "5", "A", "", ""),
        fld("NOLEN", "", "A", "", ""),
        fld("BADT", "5", "X", "", ""),
        fld("NUMV", "5", "P", "0", "VARLEN"),
        fld("CDFT", "5", "A", "", "DFT(12)"),
        fld("NDFT", "5", "P", "0", "DFT('x') ALWNULL ALWNULL"),
        fld("BIGD", "5", "P", "7", ""),
        fld("LONGA", "40000", "A", "", ""),
        fld("F10", "10", "F", "0", ""),
        fld("USG", "1", "A", "", ""),
        fld("DSEP", "", "L", "", "DATSEP('-')"),
        fld("NULLD", "1", "A", "", "DFT(*NULL)"),
        at(&[(6, "A"), (17, "J"), (19, "JOINX")]),
        key("A1", "ABSVAL ZONE"),
        key("A1", ""),
        key("ZZ", ""),
        fld("LATE", "1", "A", "", ""),
        rec("R2", ""),
        fld("IGNORED", "1", "A", "", ""),
    ];
    let mut src = src.to_vec();
    src[12] = at(&[(6, "A"), (19, "USG"), (34, "1"), (35, "A"), (38, "I")]);
    let c = compile(&src);
    assert!(c.definition.is_none());
    assert_eq!(c.max_severity(), Some(Severity::Error));
    let ids = c.ids();
    for want in [
        "DDS0210", "DDS0209", "DDS0207", "DDS1003", "DDS1004", "DDS0112", "DDS1014", "DDS1015",
        "DDS0208", "DDS1006", "DDS1005", "DDS1032", "DDS1007", "DDS1020", "DDS1019", "DDS1013",
        "DDS1010", "DDS1009", "DDS1008", "DDS1023", "DDS1002",
    ] {
        assert!(ids.contains(&want), "missing {want} in {ids:?}");
    }
    assert!(!ids.contains(&"DDS1011"), "{ids:?}");
    assert!(c.listing.contains("DDS1003 20"));
    assert!(c.listing.contains("compilation ended with severity 20"));
}

#[test]
fn keys_are_required_by_unique_and_order() {
    let src = [file("UNIQUE"), rec("R1", ""), fld("A1", "5", "A", "", "")];
    let c = compile(&src);
    assert_eq!(c.ids(), ["DDS1011"]);
    let src = [file("LIFO"), rec("R1", ""), fld("A1", "5", "A", "", "")];
    let c = compile(&src);
    assert_eq!(c.ids(), ["DDS1011"]);
    let src = [
        file("FCFO"),
        rec("R1", ""),
        fld("A1", "5", "A", "", ""),
        key("A1", ""),
    ];
    let def = ok(compile(&src));
    assert_eq!(def.dup_order, Some(DupOrder::Fcfo));
    assert_eq!(def.unique, None);
}

#[test]
fn no_record_format_is_severe() {
    let src = [file("UNIQUE"), fld("A1", "5", "A", "", "")];
    let c = compile(&src);
    assert_eq!(c.ids(), ["DDS1022", "DDS1001"]);
    assert_eq!(c.max_severity(), Some(Severity::Severe));
    assert!(c.definition.is_none());
}

#[test]
fn warnings_keep_the_definition() {
    let src = [
        rec("R1", ""),
        at(&[(6, "A"), (8, "N01"), (19, "A1"), (34, "5"), (35, "A")]),
        key("A1", "NOALTSEQ"),
    ];
    let c = compile(&src);
    assert_eq!(c.ids(), ["DDS0114", "DDS1029"]);
    assert_eq!(c.max_severity(), Some(Severity::Warning));
    let def = c.definition.expect("usable");
    assert!(def.format.keys[0].noaltseq);
}

#[test]
fn listing_shows_source_summary_and_messages() {
    let refs = reference_library();
    let src = [
        file("REF(QGPL/REFS)"),
        file("TEXT('Customer master')"),
        rec("CUSREC", "TEXT('Customer')"),
        fref("NAME", "", "", ""),
        fld("CUSNO", "7", "P", "0", "COLHDG('Customer' +"),
        file("       'Number') DFT(1)"),
        key("CUSNO", ""),
    ];
    let (source, _) = Source::from_text(&src.join("\n"));
    let opts = CompileOpts {
        file: Some("QSTEST/CUSTOMER".into()),
        source: Some("QSTEST/QDDSSRC(CUSTOMER)".into()),
        timestamp: Some("2026-10-10 12:00:00".into()),
    };
    let c = compile_pf(&source, &refs, &opts);
    let def = c.definition.as_ref().expect("usable");
    assert_eq!(def.text.as_deref(), Some("Customer master"));
    let l = &c.listing;
    assert!(l.starts_with("QS/400 DDS compiler"));
    assert!(l.contains("2026-10-10 12:00:00"));
    assert!(l.contains("File . . . . . . . . . . . :  QSTEST/CUSTOMER"));
    assert!(l.contains("Type . . . . . . . . . . . :  Physical file (PF)"));
    assert!(l.contains("SEQNBR  ....+....1"));
    assert!(l.contains("    4.00       A            NAME      R"));
    assert!(
        l.contains("       *       A            NAME          30A         REFFLD(NAME QGPL/REFS)"),
        "{l}"
    );
    assert!(l.contains("Record format CUSREC    Customer"));
    assert!(
        l.contains("  NAME        A         30          30       0  TEXT('A name')"),
        "{l}"
    );
    assert!(l.contains("  CUSNO       P          7    0      4      30  DFT(1)"));
    assert!(l.contains("Record length 34; 2 field(s)"));
    assert!(l.contains("    1  CUSNO       P         4  Ascending, signed"));
    assert!(l.contains("Access path: duplicates allowed; key length 4"));
    assert!(l.contains("No messages."));
    assert!(l.contains("0 message(s); compilation ended with severity 00."));
    assert!(l.lines().all(|line| line.chars().count() <= 132), "{l}");
}

#[test]
fn cp037_records_compile_the_same() {
    let src = [
        rec("R1", "TEXT('Récord')"),
        fld("A1", "5", "A", "", "COLHDG('Field' +"),
        file("  'one')"),
        key("A1", ""),
    ];
    let text = src.join("\n");
    let from_text = ok(compile(&src));
    let records: Vec<Vec<u8>> = text.lines().map(|l| ccsid::to_ebcdic(l).unwrap()).collect();
    let source = Source::from_records(
        records
            .iter()
            .enumerate()
            .map(|(i, r)| (u32::try_from((i + 1) * 100).unwrap(), 261_010, r.as_slice())),
    );
    let c = compile_pf(&source, &NoRefs, &CompileOpts::default());
    let def = c.definition.expect("usable");
    assert_eq!(def, from_text);
    assert_eq!(def.format.text.as_deref(), Some("Récord"));
    assert_eq!(def.format.fields[0].colhdg, vec!["Field", "one"]);
    assert!(c.listing.contains("  261010"));
}
