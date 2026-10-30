// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The certificate panels (`QSCERT`): Quartz extensions with no IBM
//! screen to match, so each is rendered and its rows checked for what
//! must be there and fit in 80 columns.

use uim::render::{ListState, MapDialog, Options, render};
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

fn group() -> PanelGroup {
    let text = uim::shipped::source("QSCERT").expect("QSCERT is shipped");
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "QSCERT:\n{}", c.listing);
    c.group.expect("a group")
}

fn opts() -> Options {
    Options {
        system: "QS400".into(),
        ..Options::default()
    }
}

fn list(rows: &[&[(&str, &str)]]) -> ListState {
    ListState {
        rows: rows
            .iter()
            .map(|r| r.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect())
            .collect(),
        top: 0,
        total: Some(rows.len()),
        complete: true,
    }
}

fn rows_of(panel: &str, d: &MapDialog) -> Vec<String> {
    let s = render(&group(), panel, d, &opts()).unwrap();
    let rows = s.text_rows();
    if std::env::var_os("QS400_SHOW").is_some() {
        eprintln!("{}", rows.join("
"));
    }
    assert_eq!(rows.len(), 24);
    for r in &rows {
        assert!(r.chars().count() <= 80, "{r:?}");
    }
    rows
}

fn find<'a>(rows: &'a [String], text: &str) -> &'a str {
    rows.iter()
        .find(|r| r.contains(text))
        .unwrap_or_else(|| panic!("no {text:?} in\n{}", rows.join("\n")))
}

#[test]
fn wrkcrt_lists_labels_with_their_expiry() {
    let mut d = MapDialog::new();
    d.set("KEYSTORE", "*SYSTEM");
    d.set_list(
        "CERTS",
        list(&[
            &[("CRTLABEL", "ADMIN"), ("CRTTYPE", "Server"), ("CRTEXP", "2027-10-30"), ("CRTSTS", "Valid"), ("CRTDFT", "Yes")],
            &[
                ("CRTLABEL", "LOCAL_CERTIFICATE_AUTHORITY"),
                ("CRTTYPE", "CA"),
                ("CRTEXP", "2036-10-27"),
                ("CRTSTS", "Valid"),
                ("CRTDFT", ""),
            ],
        ]),
    );
    let rows = rows_of("WRKCRT", &d);
    assert!(rows[0].contains("Work with Certificates"));
    assert!(find(&rows, "Certificate store").contains("*SYSTEM"));
    find(&rows, "2=Change");
    find(&rows, "8=Assign");
    let admin = find(&rows, "ADMIN ");
    assert!(admin.contains("2027-10-30") && admin.contains("Server"));
    find(&rows, "LOCAL_CERTIFICATE_AUTHORITY");
    find(&rows, "F6=Create");
}

#[test]
fn dspcrt_shows_the_facts() {
    let mut d = MapDialog::new();
    for (k, v) in [
        ("DLABEL", "ADMIN"),
        ("KEYSTORE", "*SYSTEM"),
        ("DTYPE", "Server"),
        ("DSUBJECT", "CN=qs400.example"),
        ("DTO", "2027-10-30 12:00:00"),
        ("DDAYS", "365"),
        ("DKEY", "*ECDSAP256"),
        ("DAPPS", "QIBM_HTTP_SERVER_ADMIN"),
        ("DSHA1", "AB:CD"),
        ("DSHA2", "EF:01"),
    ] {
        d.set(k, v);
    }
    let rows = rows_of("DSPCRT", &d);
    assert!(find(&rows, "Subject").contains("CN=qs400.example"));
    assert!(find(&rows, "Days left").contains("365"));
    assert!(find(&rows, "Assigned to").contains("QIBM_HTTP_SERVER_ADMIN"));
    find(&rows, "SHA-256 fingerprint");
    assert!(find(&rows, "SHA-256 (continued)").contains("EF:01"));
}

#[test]
fn wrkappid_lists_the_applications() {
    let mut d = MapDialog::new();
    d.set_list(
        "APPS",
        list(&[
            &[("APPID", "QIBM_HTTP_SERVER_ADMIN"), ("APPTYPE", "*SERVER"), ("APPCRT", "ADMIN"), ("APPAUTH", "*NONE")],
            &[("APPID", "QIBM_QTV_TELNET_SERVER"), ("APPTYPE", "*SERVER"), ("APPCRT", "*NONE"), ("APPAUTH", "*NONE")],
        ]),
    );
    let rows = rows_of("WRKAPPID", &d);
    assert!(rows[0].contains("Work with Certificate Applications"));
    let a = find(&rows, "QIBM_HTTP_SERVER_ADMIN");
    assert!(a.contains("ADMIN") && a.contains("*SERVER"));
    find(&rows, "9=Unassign");
    find(&rows, "F6=Add");
}
