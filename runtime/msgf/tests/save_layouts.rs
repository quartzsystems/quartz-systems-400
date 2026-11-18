// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The save and restore messages carry IBM's severity and data layout:
//! each entry's variables are the first of those the primary oracle
//! declares (`tools/conformance/oracle/msgdata/fields.txt`), `*UBIN`
//! read as `*BIN`, so a program that monitors for one reads its data
//! where IBM's puts it.

use std::collections::HashMap;

use msgf::source::{QCPFMSG, parse};

/// The identifiers `SAVLIB`, `RSTLIB`, `SAV`, `RST`, `SAVSECDTA`,
/// `RSTUSRPRF`, `RSTAUT`, `SAVSYS`, the save file commands and `CRTLIB`
/// send.
const SAVE_MSGS: &[&str] = &[
    "CPC2102", "CPC3701", "CPC3702", "CPC3703", "CPC3705", "CPC3706", "CPC370C", "CPC370D",
    "CPC370E", "CPC370F", "CPC3710", "CPC3722", "CPC3724", "CPF3701", "CPF3771", "CPF3770",
    "CPF3773", "CPF3775", "CPF3776", "CPF3777", "CPF3780", "CPF3781", "CPF3787", "CPF3794",
    "CPF3743", "CPF3785", "CPF3707", "CPF3808", "CPF3815", "CPF4110", "CPF2189", "CPF3760",
    "CPF3761", "CPF3764", "CPF3756", "CPF3724", "CPF3725", "CPF3726", "CPF7085", "CPD2415",
    "CPF7081", "CPF3734", "CPF3204", "CPF3284", "CPF3283", "CPI320A", "CPF3766", "CPF370B",
    "CPD37A2", "CPI3811", "CPF3881", "CPI371D", "CPF3836", "CPF3848", "CPF3795", "CPF3801",
    "CPD3773", "CPF3719", "CPF3294", "CPD3740", "CPD373F", "CPD37C0", "CPI3733", "CPI374B",
    "CPD3765", "CPD3775", "CPF3837", "CPF3838", "CPF3839", "CPF383E", "CPF3845", "CPF3814",
    "CPF9812", "CPF9814", "CPF3741", "CPF3757", "CPF3782", "CPF3793", "CPF3736",
];

/// `ID -> (severity, fields)` from the oracle's table.
fn oracle() -> HashMap<String, (u16, Vec<String>)> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/oracle/msgdata/fields.txt");
    let text = std::fs::read_to_string(path).expect("fields.txt");
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| {
            let mut p = l.split('|');
            let id = p.next()?.to_string();
            let sev = p.next()?.trim().parse().ok()?;
            let fields = p
                .next()?
                .split(';')
                .filter(|f| !f.trim().is_empty())
                .map(|f| {
                    let f = f.trim();
                    let decl = f.split_once(' ').map_or("", |(_, d)| d);
                    decl.replace("*UBIN", "*BIN")
                })
                .collect();
            Some((id, (sev, fields)))
        })
        .collect()
}

#[test]
fn save_messages_have_ibms_layouts() {
    let f = parse(QCPFMSG).unwrap();
    let ibm = oracle();
    for id in SAVE_MSGS {
        let d = f.find(id).unwrap_or_else(|| panic!("{id} not in QCPFMSG"));
        let (sev, fields) = ibm
            .get(*id)
            .unwrap_or_else(|| panic!("{id} not in fields.txt"));
        assert_eq!(d.severity, *sev, "{id} severity");
        let ours: Vec<String> = d.vars.iter().map(msgf::VarType::declaration).collect();
        assert!(
            ours.len() <= fields.len() && ours.iter().zip(fields).all(|(a, b)| a == b),
            "{id}: ours {ours:?}, IBM's {fields:?}"
        );
    }
}

#[test]
fn every_variable_a_text_names_is_declared() {
    let f = parse(QCPFMSG).unwrap();
    for id in SAVE_MSGS {
        let d = f.find(id).unwrap();
        for n in 1..=20 {
            let used = |s: &str| {
                s.match_indices(&format!("&{n}"))
                    .any(|(i, m)| !s[i + m.len()..].starts_with(|c: char| c.is_ascii_digit()))
            };
            if used(&d.text) || used(&d.help) {
                assert!(n <= d.vars.len(), "{id} names &{n}");
            }
        }
    }
}

#[test]
fn restore_completion_renders_its_data() {
    let f = parse(QCPFMSG).unwrap();
    let d = f.find("CPC3703").unwrap();
    let mut data = Vec::new();
    data.extend_from_slice(&6i32.to_be_bytes());
    data.extend_from_slice(&0i32.to_be_bytes());
    data.extend_from_slice(&ccsid::to_ebcdic("TAPLIB    ").unwrap());
    data.extend_from_slice(&ccsid::to_ebcdic("TAPLIB2   ").unwrap());
    assert_eq!(
        d.format(&data),
        "6 objects saved from library TAPLIB were restored to library TAPLIB2."
    );
    let d = f.find("CPC2102").unwrap();
    let mut data = ccsid::to_ebcdic("TAPLIB    ").unwrap();
    data.extend_from_slice(&1i16.to_be_bytes());
    assert_eq!(d.format(&data), "Library TAPLIB was created in ASP 1.");
}
