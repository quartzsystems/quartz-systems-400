// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The communications commands against the pub400 definitions
//! (`tools/conformance/oracle/net/pub400/command-definitions.txt`):
//! every parameter the capture lists, in its order, with its default,
//! minimum and maximum. Parameters the capture leaves out but IBM
//! documents (`EXCHID`, `AUTOCRTCTL`, ...) sit among them, prompted on
//! request.

use cmd::def::CmdDef;

fn def(name: &str) -> CmdDef {
    let src = sys_cpps::defs::source(name).unwrap_or_else(|| panic!("{name}: no definition"));
    cmd::source::compile(name, src).unwrap_or_else(|d| panic!("{name}: {d:?}"))
}

/// The capture's parameters appear in its order (others may sit
/// between them).
fn in_order(d: &CmdDef, captured: &[&str]) {
    let kwds: Vec<&str> = d.parms.iter().map(|p| p.kwd.as_str()).collect();
    let mut at = 0;
    for k in captured {
        let i = kwds[at..]
            .iter()
            .position(|x| x == k)
            .unwrap_or_else(|| panic!("{}: {k} missing or out of order in {kwds:?}", d.name));
        at += i + 1;
    }
}

fn parm<'a>(d: &'a CmdDef, kwd: &str) -> &'a cmd::def::Parm {
    d.parms
        .iter()
        .find(|p| p.kwd == kwd)
        .unwrap_or_else(|| panic!("{}: no {kwd}", d.name))
}

#[test]
fn crtlineth_has_the_captured_parameters() {
    let d = def("CRTLINETH");
    assert_eq!(d.maxpos, Some(2));
    in_order(
        &d,
        &[
            "LIND", "RSRCNAME", "BRIDGE", "ONLINE", "VRYWAIT", "AGGPCY", "AGGRSCL", "NETCTL", "NWS",
            "ASSOCPORT", "ADPTADR", "GRPADR", "ETHSTD", "LINESPEED", "DUPLEX", "SRVOPT", "MAXFRAME",
            "SSAP", "CMNRCYLMT", "MSGQ", "AUT", "TEXT",
        ],
    );
    // And the documented ones the capture leaves out.
    for k in ["EXCHID", "AUTOCRTCTL", "AUTODLTCTL", "MAXCTL", "THRESHOLD", "GENTSTFRM"] {
        parm(&d, k);
    }
    assert_eq!(parm(&d, "LIND").min, 1);
    assert_eq!(parm(&d, "RSRCNAME").min, 1);
    assert_eq!(parm(&d, "ONLINE").dft.as_deref(), Some("*YES"));
    assert_eq!(parm(&d, "VRYWAIT").dft.as_deref(), Some("*NOWAIT"));
    assert_eq!(parm(&d, "MAXFRAME").dft.as_deref(), Some("1496"));
    assert_eq!(
        parm(&d, "MAXFRAME").range,
        Some(("1496".to_string(), "8996".to_string()))
    );
    assert_eq!(parm(&d, "AGGRSCL").max, 8);
    assert_eq!(parm(&d, "SSAP").max, 24);
    assert_eq!(parm(&d, "GRPADR").max, 12);
    assert_eq!(parm(&d, "ADPTADR").dft.as_deref(), Some("*ADPT"));
    assert_eq!(parm(&d, "ETHSTD").dft.as_deref(), Some("*ETHV2"));
    assert_eq!(parm(&d, "LINESPEED").dft.as_deref(), Some("*AUTO"));
    assert_eq!(
        parm(&d, "LINESPEED").values,
        ["10M", "100M", "1G", "10G", "25G", "100G", "*AUTO"]
    );
    assert_eq!(parm(&d, "MSGQ").dft.as_deref(), Some("*SYSVAL"));
    assert_eq!(parm(&d, "AUT").dft.as_deref(), Some("*CHANGE"));
    assert_eq!(parm(&d, "TEXT").dft.as_deref(), Some("*BLANK"));
}

#[test]
fn chglineth_defaults_to_same() {
    let d = def("CHGLINETH");
    assert_eq!(d.maxpos, Some(1));
    in_order(
        &d,
        &[
            "LIND", "RSRCNAME", "BRIDGE", "ONLINE", "VRYWAIT", "AGGPCY", "AGGRSCL", "ASSOCPORT",
            "ADPTADR", "GRPADR", "SSAP", "LINESPEED", "DUPLEX", "SRVOPT", "MAXFRAME", "CMNRCYLMT",
            "MSGQ", "TEXT",
        ],
    );
    for p in d.parms.iter().filter(|p| p.kwd != "LIND") {
        assert_eq!(p.dft.as_deref(), Some("*SAME"), "CHGLINETH {}", p.kwd);
    }
}

#[test]
fn the_other_line_commands() {
    let d = def("DLTLIND");
    in_order(&d, &["LIND"]);
    assert_eq!(parm(&d, "LIND").min, 1);
    let d = def("DSPLIND");
    in_order(&d, &["LIND", "OUTPUT", "OPTION"]);
    assert_eq!(parm(&d, "OPTION").dft.as_deref(), Some("*ALL"));
    let d = def("WRKLIND");
    in_order(&d, &["LIND"]);
    assert_eq!(parm(&d, "LIND").dft.as_deref(), Some("*ALL"));
}

#[test]
fn vrycfg_and_wrkcfgsts() {
    let d = def("VRYCFG");
    in_order(
        &d,
        &[
            "CFGOBJ",
            "CFGTYPE",
            "STATUS",
            "RANGE",
            "VRYWAIT",
            "ASCVRYOFF",
            "RESET",
            "RSRCNAME",
            "FRCVRYOFF",
            "SBMMLTJOB",
            "JOBD",
            "GENPTHCERT",
            "RESETSYS",
        ],
    );
    assert_eq!(parm(&d, "CFGOBJ").max, 256);
    assert_eq!(parm(&d, "VRYWAIT").dft.as_deref(), Some("*CFGOBJ"));
    assert_eq!(parm(&d, "RANGE").dft.as_deref(), Some("*NET"));
    let d = def("WRKCFGSTS");
    in_order(
        &d,
        &["CFGTYPE", "CFGD", "OUTPUT", "RMTLOCNAME", "ASTLVL", "RANGE", "STATUS"],
    );
    assert_eq!(parm(&d, "STATUS").dft.as_deref(), Some("*ALL"));
    assert_eq!(
        parm(&d, "STATUS").values,
        ["*ALL", "*ACTIVE", "*FAILED", "*VARYOFF", "*VARYON", "*AVAILABLE"]
    );
}

#[test]
fn where_they_run() {
    use sys_cpps::inventory::{DIRECT, JOB_SITE};
    for c in ["CRTLINETH", "CHGLINETH", "DLTLIND", "DSPLIND", "WRKLIND", "VRYCFG", "WRKCFGSTS"] {
        assert!(DIRECT.contains(&c), "{c} runs in the service");
    }
    for c in ["DSPLIND", "WRKLIND", "WRKCFGSTS"] {
        assert!(JOB_SITE.contains(&c), "{c} is a panel in the job");
    }
}
