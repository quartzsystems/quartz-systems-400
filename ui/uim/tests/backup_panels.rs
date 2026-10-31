// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The backup panels (`QEZBCKUP`): `DSPBCKUP`'s two pages against
//! pub400's captures under `tools/conformance/golden/phase11/displays/`
//! row by row, the message line aside; the `GO SAVE` command defaults
//! and the command list against goldens of our own rendering under
//! `tests/golden/backup/` (`QS400_UPDATE_GOLDEN=1` rewrites them).

use std::path::{Path, PathBuf};

use uim::render::{ListState, MapDialog, Options, render};
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

fn group() -> PanelGroup {
    let text = uim::shipped::source("QEZBCKUP").expect("QEZBCKUP is shipped");
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "QEZBCKUP:\n{}", c.listing);
    c.group.expect("a group")
}

fn rows(panel: &str, d: &MapDialog, page: usize) -> Vec<String> {
    let opts = Options {
        system: "SYSNAME".into(),
        page,
        ..Options::default()
    };
    let s = render(&group(), panel, d, &opts).unwrap();
    let rows = s.text_rows();
    assert_eq!(rows.len(), 24);
    rows
}

/// The daily options pub400 shows.
fn daily() -> MapDialog {
    let mut d = MapDialog::new();
    for (k, v) in [
        ("TAPSET1", "DAYA"),
        ("TAPSET2", "DAYB"),
        ("TAPSET3", "DAYC"),
        ("CLRTAP", "Yes"),
        ("USRLIB", "From list"),
        ("USRFLR", "From list"),
        ("USRDIR", "All"),
        ("SECDTA", "No"),
        ("CFGDTA", "No"),
        ("SBMJOB", "Yes"),
        ("CHGONLY", "Yes"),
        ("PRTRPT", "Yes"),
        ("EXITPGM", "*NONE"),
    ] {
        d.set(k, v);
    }
    d
}

fn against_pub400(golden: &str, panel: &str, page: usize) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/phase11/displays")
        .join(format!("{golden}.txt"));
    let text = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let theirs: Vec<&str> = text.lines().collect();
    let ours = rows(panel, &daily(), page);
    let mut bad = Vec::new();
    // The last row is the message line.
    for (i, our) in ours.iter().enumerate().take(23) {
        let their = theirs.get(i).copied().unwrap_or("").trim_end();
        if our.trim_end() != their {
            bad.push(format!("row {:>2}\n  ours:   |{}|\n  golden: |{their}|", i + 1, our.trim_end()));
        }
    }
    assert!(bad.is_empty(), "{panel} against {golden}:\n{}", bad.join("\n"));
}

#[test]
fn dspbckup_daily_matches_pub400() {
    against_pub400("dspbckup", "DSPBCKD", 0);
    against_pub400("dspbckup-2", "DSPBCKD", 1);
}

#[test]
fn weekly_and_monthly_titles() {
    assert!(rows("DSPBCKW", &daily(), 0)[0].contains("Display Weekly Backup Options"));
    assert!(rows("DSPBCKM", &daily(), 0)[0].contains("Display Monthly Backup Options"));
}

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/backup")
}

fn golden(panel: &str, d: &MapDialog) -> Vec<String> {
    let r = rows(panel, d, 0);
    let text = r.iter().map(|r| r.trim_end()).collect::<Vec<_>>().join("\n") + "\n";
    let path = dir().join(format!("{panel}.txt"));
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() || !path.exists() {
        std::fs::create_dir_all(dir()).unwrap();
        std::fs::write(&path, &text).unwrap();
    }
    let want = std::fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    assert_eq!(text, want, "{panel} differs from {}", path.display());
    r
}

#[test]
fn save_command_defaults() {
    let mut d = MapDialog::new();
    for (k, v) in wm_fields() {
        d.set(k, v);
    }
    let r = golden("SAVDFT", &d);
    for label in [
        "Devices",
        "Prompt for commands",
        "Check for active files",
        "Message queue delivery",
        "Start time",
        "Vary off network servers",
        "Unmount file systems",
        "Print system information",
        "Use system reply list",
        "Spooled file data",
    ] {
        assert!(r.iter().any(|x| x.contains(label)), "no {label:?}:\n{}", r.join("\n"));
    }
    assert!(r[0].contains("Specify Command Defaults"));
}

/// The shipped defaults as the panel shows them.
fn wm_fields() -> Vec<(&'static str, &'static str)> {
    vec![
        ("DEV1", "TAP01"),
        ("PMTCMD", "Y"),
        ("CHKACT", "Y"),
        ("MSGDLV", "*BREAK"),
        ("STRTIM", "*CURRENT"),
        ("VRYOFF", "*ALL"),
        ("UNMNT", "N"),
        ("PRTINF", "N"),
        ("RPYL", "N"),
        ("SPLDTA", "*NONE"),
    ]
}

#[test]
fn the_commands_an_option_runs() {
    let mut d = MapDialog::new();
    let cmds = [
        "ENDSBS SBS(*ALL) OPTION(*IMMED)",
        "CHGMSGQ MSGQ(QSYSOPR) DLVRY(*BREAK) SEV(60)",
        "SAVLIB LIB(*NONSYS) DEV(TAP01) ACCPTH(*YES) SPLFDTA(*NONE)",
        "STRSBS SBSD(QSYS/QCTL)",
    ];
    d.set_list(
        "CMDS",
        ListState {
            rows: cmds.iter().map(|c| vec![("CMDLINE".to_string(), (*c).to_string())]).collect(),
            top: 0,
            total: Some(cmds.len()),
            complete: true,
        },
    );
    let r = golden("SAVCMDS", &d);
    assert!(r.iter().any(|x| x.contains("STRSBS SBSD(QSYS/QCTL)")));
}
