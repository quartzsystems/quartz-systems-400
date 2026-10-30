// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The certificate commands (Quartz extensions, no oracle to match):
//! their keywords in prompt order, where each runs, and a typed line
//! for each through the analyzer.

use sys_cpps::Site;

const COMMANDS: &[(&str, &[&str])] = &[
    (
        "QSCRTCRTS",
        &["KEYSTORE", "PASSWORD", "STASH", "ROOTS", "CN", "ORG", "ORGUNIT", "LOCALITY", "STATE", "COUNTRY", "KEYALG", "VALIDITY"],
    ),
    ("QSDLTCRTS", &["KEYSTORE", "PASSWORD"]),
    (
        "QSCRTCRT",
        &[
            "LABEL", "CN", "KEYSTORE", "PASSWORD", "SIGNER", "USAGE", "ORG", "ORGUNIT", "LOCALITY", "STATE", "COUNTRY", "ALTNAME",
            "KEYALG", "VALIDITY", "CSR", "LCLCAPWD", "TEXT",
        ],
    ),
    ("QSIMPCRT", &["FROMSTMF", "LABEL", "KEYSTORE", "PASSWORD", "FORMAT", "IMPPWD", "TRUST"]),
    ("QSEXPCRT", &["LABEL", "TOSTMF", "KEYSTORE", "PASSWORD", "FORMAT", "INCKEY", "EXPPWD", "REPLACE"]),
    ("QSCHGCRT", &["LABEL", "KEYSTORE", "PASSWORD", "NEWLABEL", "TRUST", "DFTCRT", "TEXT"]),
    ("QSDLTCRT", &["LABEL", "KEYSTORE", "PASSWORD"]),
    ("QSWRKCRT", &["KEYSTORE", "PASSWORD"]),
    ("QSASNCRT", &["APPID", "LABEL"]),
    ("QSADDAPPID", &["APPID", "TYPE", "TEXT"]),
    ("QSCHGAPPID", &["APPID", "CLTAUTH", "CATRUST", "TEXT"]),
    ("QSRMVAPPID", &["APPID"]),
    ("QSWRKAPPID", &["APPID"]),
    ("QSCHKCRT", &[]),
];

#[test]
fn definitions_have_their_keywords() {
    for (name, kwds) in COMMANDS {
        let src = sys_cpps::defs::source(name).unwrap_or_else(|| panic!("{name} has no definition"));
        let def = cmd::source::compile(name, src).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        let ours: Vec<&str> = def.parms.iter().map(|p| p.kwd.as_str()).collect();
        assert_eq!(&ours, kwds, "{name}");
    }
}

#[test]
fn the_panels_run_in_the_job_the_rest_in_the_service() {
    for (name, _) in COMMANDS {
        let e = sys_cpps::entry(name).unwrap_or_else(|| panic!("{name} has no entry"));
        let job = matches!(*name, "QSWRKCRT" | "QSWRKAPPID");
        assert_eq!(e.site, if job { Site::Job } else { Site::Service }, "{name}");
        assert!(e.cpp.starts_with("QSZC"), "{name}: {}", e.cpp);
        if job {
            assert!(sys_cpps::inventory::in_service_too(name), "{name} has no text form");
        }
    }
}

#[test]
fn passwords_are_not_shown() {
    for (name, kwds) in COMMANDS {
        let def = cmd::source::compile(name, sys_cpps::defs::source(name).unwrap()).unwrap();
        for k in kwds.iter().filter(|k| k.ends_with("PWD") || **k == "PASSWORD") {
            let p = def.parms.iter().find(|p| p.kwd == *k).unwrap();
            assert_eq!(p.dspinput, cmd::def::DspInput::No, "{name} {k} shows its input");
        }
    }
}
