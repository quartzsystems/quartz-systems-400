// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `WRKTIMZON`'s views and Display Time Zone Description Details
//! (`QSTIMZON`) against the 7.1 captures
//! (`tools/conformance/golden/verified/oracle71/timzon/`), row for row,
//! the data the capture shows put in (IBM's full names included: the
//! geometry is what is compared; QS/400 shows its own names).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use uim::render::{ListState, MapDialog, Options, Screen, render};
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

fn group() -> PanelGroup {
    let text = uim::shipped::source("QSTIMZON").expect("QSTIMZON is shipped");
    let c = compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "QSTIMZON:\n{}", c.listing);
    c.group.expect("a group")
}

fn golden(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../tools/conformance/golden/verified/oracle71/timzon/{name}.txt"
    ))
}

fn against(name: &str, s: &Screen) {
    let want: Vec<String> = std::fs::read_to_string(golden(name))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect();
    let ours = s.text_rows();
    let mut diff = String::new();
    for (i, (w, o)) in want.iter().zip(&ours).enumerate().take(23) {
        if o.trim_end() != w {
            let _ = write!(
                diff,
                "row {:>2}\n  want |{w}|\n  got  |{}|\n",
                i + 1,
                o.trim_end()
            );
        }
    }
    assert!(diff.is_empty(), "{name}:\n{diff}");
}

fn opts(system: &str, page: usize) -> Options {
    Options {
        system: system.into(),
        page,
        ..Options::default()
    }
}

/// The capture's first page of zones.
fn zones() -> ListState {
    let rows: [[&str; 9]; 11] = [
        [
            " 00:00",
            "Q0000GMT",
            "GMT",
            "Greenwich Mean Time",
            "",
            "",
            "GMT",
            "",
            "",
        ],
        [
            " 00:00",
            "Q0000GMT2",
            "GMT",
            "Greenwich Mean Time",
            "BST",
            "British Summer Time",
            "Europe/London",
            "",
            "",
        ],
        [
            " 00:00",
            "Q0000GMT3",
            "GMT",
            "Greenwich Mean Time",
            "BST",
            "British Summer Time",
            "Europe/London",
            "",
            "",
        ],
        [
            " 00:00",
            "Q0000UTC",
            "UTC",
            "Coordinated Universal Time",
            "",
            "",
            "UTC",
            "",
            "",
        ],
        [
            "-01:00",
            "QN0100UTCS",
            "UTC-01:00S",
            "UTC-01:00 Standard Time",
            "",
            "",
            "GMT-1",
            "",
            "",
        ],
        [
            "-02:00",
            "QN0200UTCS",
            "UTC-02:00S",
            "UTC-02:00 Standard Time",
            "",
            "",
            "GMT-2",
            "",
            "",
        ],
        [
            "-03:00",
            "QN0300UTCS",
            "UTC-03:00S",
            "UTC-03:00 Standard Time",
            "",
            "",
            "GMT-3",
            "",
            "",
        ],
        [
            "-03:00",
            "QN0300UTC2",
            "UTC-03:00S",
            "(GMT - 3:00) Brasilia",
            "UTC-03:00D",
            "(GMT - 3:00) Brasilia Daylight Saving Time",
            "America/Sao_Paulo",
            "",
            "",
        ],
        [
            "-03:30",
            "QN0330NST",
            "NST",
            "Newfoundland Standard Time",
            "NDT",
            "Newfoundland Daylight Time",
            "America/St_Johns",
            "",
            "",
        ],
        [
            "-03:30",
            "QN0330NST2",
            "NST",
            "Newfoundland Standard Time",
            "NDT",
            "Newfoundland Daylight Time",
            "America/St_Johns",
            "",
            "",
        ],
        [
            "-03:30",
            "QN0330NST3",
            "NST",
            "Newfoundland Standard Time",
            "NDT",
            "Newfoundland Daylight Time",
            "America/St_Johns",
            "",
            "",
        ],
    ];
    let names = [
        "OFFSET", "TIMZON", "ABBR", "FULL", "DABBR", "DFULL", "ALT", "TEXT", "MARK",
    ];
    ListState {
        total: Some(93),
        rows: rows
            .iter()
            .map(|r| {
                names
                    .iter()
                    .zip(r)
                    .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                    .collect()
            })
            .collect(),
        top: 0,
        complete: true,
    }
}

#[test]
fn the_four_views_match_the_captures() {
    let g = group();
    let mut d = MapDialog::new();
    d.set("*DATETIME", "MM/DD/YY  HH:MM:SS EST");
    d.set_list("ZONES", zones());
    for (panel, name) in [
        ("WRKTIMZON", "wrktimzon"),
        ("WRKTIMZON2", "wrktimzon-dst"),
        ("WRKTIMZON3", "wrktimzon-alt"),
        ("WRKTIMZON4", "wrktimzon-text"),
    ] {
        against(name, &render(&g, panel, &d, &opts("S062B175", 0)).unwrap());
    }
}

fn details(dst: bool) -> MapDialog {
    let mut d = MapDialog::new();
    let common = [
        ("ZONE", if dst { "QN0500EST3" } else { "Q0000UTC" }),
        ("ZOFFSET", if dst { "-05:00" } else { " 00:00" }),
        (
            "STDFULL",
            if dst {
                "Eastern Standard Time"
            } else {
                "Coordinated Universal Time"
            },
        ),
        ("STDABBR", if dst { "EST" } else { "UTC" }),
        ("STDMSG", if dst { "CPX091E" } else { "CPX0910" }),
        ("MSGF", "QCPFMSG"),
        ("MSGFLIB", "*LIBL"),
        ("YEAROFS", "0"),
        ("ALTNAME", if dst { "America/New_York" } else { "UTC" }),
        ("ZTEXT", ""),
    ];
    for (k, v) in common {
        d.set(k, v);
    }
    if dst {
        for (k, v) in [
            ("DSTFULL", "Eastern Daylight Time"),
            ("DSTABBR", "EDT"),
            ("DSTMSG", "CPX091F"),
            ("SMONTH", "March"),
            ("SDAY", "Sunday"),
            ("SREL", "Second"),
            ("STIME", "HH:MM:SS"),
            ("EMONTH", "November"),
            ("EDAY", "Sunday"),
            ("EREL", "First"),
            ("ETIME", "HH:MM:SS"),
            ("SHIFT", "60"),
        ] {
            d.set(k, v);
        }
    }
    d
}

#[test]
fn the_details_match_the_captures() {
    let g = group();
    let d = details(true);
    for (page, name) in [(0, "dsptimzon-1"), (1, "dsptimzon-2"), (2, "dsptimzon-3")] {
        against(
            name,
            &render(&g, "DSPTZ", &d, &opts("SYSNAME", page)).unwrap(),
        );
    }
    let d = details(false);
    for (page, name) in [(0, "dsptimzon-nodst-1"), (1, "dsptimzon-nodst-2")] {
        against(
            name,
            &render(&g, "DSPTZN", &d, &opts("SYSNAME", page)).unwrap(),
        );
    }
}
