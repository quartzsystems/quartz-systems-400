// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The renderer over `tests/pnlgrp/render-*.pnlgrp`: every case's
//! screen (the character grid, the cursor, the indicator and the
//! fields) compared with `tests/expected/render-<name>.scr`.
//! `QS400_UPDATE_UIM=1` rewrites the expected screens.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use uim::render::{Attr, ListState, MapDialog, Options, Screen, Target, render, render_help};
use uim::{CompileOpts, NoImbeds, NoImports, PanelGroup, compile};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

fn group(name: &str) -> PanelGroup {
    let path = root().join("pnlgrp").join(format!("render-{name}.pnlgrp"));
    let text = std::fs::read_to_string(&path).expect("source");
    let c = compile(&text, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "render-{name} must compile:\n{}", c.listing);
    c.group.expect("group")
}

/// One screen as text: the rows between bars, then the cursor, the
/// indicator and the fields.
fn dump(title: &str, s: &Screen) -> String {
    let mut out = format!("=== {title}\n");
    for row in s.text_rows() {
        let _ = writeln!(out, "|{row}|");
    }
    let _ = writeln!(out, "cursor: {:?}", s.cursor);
    let _ = writeln!(out, "more: {:?}", s.more);
    for f in &s.fields {
        let target = match &f.target {
            Target::Var(v) => format!("var {v}"),
            Target::ListOption { list, row_index } => format!("opt {list}[{row_index}]"),
            Target::ListCell {
                list,
                row_index,
                var,
            } => format!("cell {list}[{row_index}].{var}"),
            Target::CmdLine => "cmdline".to_string(),
            Target::MenuSelection => "selection".to_string(),
            Target::PositionTo(l) => format!("posto {l}"),
        };
        let _ = writeln!(
            out,
            "field {:>2},{:>2} len {:>3} {:?} {target} {:?}",
            f.row, f.col, f.len, f.attr, f.value
        );
    }
    let blue = s
        .texts
        .iter()
        .filter(|t| t.attr == Attr::Blue)
        .map(|t| format!("{},{}", t.row, t.col))
        .collect::<Vec<_>>()
        .join(" ");
    let _ = writeln!(out, "blue: {blue}");
    let bright = s
        .texts
        .iter()
        .filter(|t| t.attr == Attr::HighIntensity)
        .map(|t| format!("{},{}", t.row, t.col))
        .collect::<Vec<_>>()
        .join(" ");
    let _ = writeln!(out, "bright: {bright}");
    out
}

fn check(name: &str, text: &str) {
    let dir = root().join("expected");
    let path = dir.join(format!("render-{name}.scr"));
    if std::env::var_os("QS400_UPDATE_UIM").is_some() {
        std::fs::write(&path, text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e} (QS400_UPDATE_UIM=1 writes it)", path.display()))
        .replace("\r\n", "\n");
    if want != text {
        let mut diff = String::new();
        for (i, (a, b)) in want.lines().zip(text.lines()).enumerate() {
            if a != b {
                let _ = writeln!(diff, "line {}:\n  want {a}\n  got  {b}", i + 1);
            }
        }
        panic!("render-{name} differs from the expected screens:\n{diff}");
    }
}

fn widgets(n: usize, complete: bool, top: usize) -> ListState {
    let rows = (0..n)
        .map(|i| {
            let k = top + i + 1;
            vec![
                ("OPT".to_string(), String::new()),
                ("WNAME".to_string(), format!("WIDGET{k:02}")),
                (
                    "WKIND".to_string(),
                    if k.is_multiple_of(2) {
                        "*GEAR"
                    } else {
                        "*LEVER"
                    }
                    .to_string(),
                ),
                ("WLIB".to_string(), "QGPL".to_string()),
                (
                    "WTEXT".to_string(),
                    format!("widget number {k} with a long text that is cut"),
                ),
                ("WQTY".to_string(), (k * 7).to_string()),
            ]
        })
        .collect();
    ListState {
        rows,
        top,
        total: if complete { Some(top + n) } else { None },
        complete,
    }
}

#[test]
fn list_panels() {
    let g = group("list");
    let mut out = String::new();
    let mut d = MapDialog::new();
    d.set_list("WIDGETS", widgets(3, true, 0));
    let opts = Options::default();
    out += &dump(
        "WRKWDG view 1, three entries",
        &render(&g, "WRKWDG", &d, &opts).unwrap(),
    );

    d.set_list("WIDGETS", widgets(14, false, 0));
    let opts = Options {
        view: 1,
        more_keys: true,
        message: Some(("Widget WIDGET03 is in use.".into(), true)),
        ..Options::default()
    };
    out += &dump(
        "WRKWDG view 2, more keys, a message",
        &render(&g, "WRKWDG", &d, &opts).unwrap(),
    );

    d.set_list("WIDGETS", widgets(2, true, 20))
        .set("ALLOWRNM", "N");
    let opts = Options {
        more_keys: true,
        ..Options::default()
    };
    out += &dump(
        "WRKWDG second key set without F21",
        &render(&g, "WRKWDG", &d, &opts).unwrap(),
    );

    let mut d = MapDialog::new();
    d.set_list("WIDGETS", widgets(2, true, 0));
    let opts = Options {
        system: "QUARTZ1".into(),
        ..Options::default()
    };
    out += &dump(
        "SHOWWDG grouped headings",
        &render(&g, "SHOWWDG", &d, &opts).unwrap(),
    );
    check("list", &out);
}

#[test]
fn data_panels() {
    let g = group("data");
    let mut d = MapDialog::new();
    d.set("WNAME", "GADGET")
        .set("WLIB", "QGPL")
        .set("OWNER", "QPGMR")
        .set("KIND", "*GEAR")
        .set("QTY", "120")
        .set("PRIO", "5")
        .set("OUTQ", "QPRINT")
        .set("OUTQLIB", "QGPL")
        .set("SDATE", "2026-10-23")
        .set("STIME", "14:05:00")
        .set("SIZE", "4096")
        .set("UNITS", "*BYTES")
        .set("WTEXT", "A gadget for the test corpus")
        .set("NOTE", "Nothing to note");
    let mut out = String::new();
    let opts = Options::default();
    out += &dump("DSPWDG page 1", &render(&g, "DSPWDG", &d, &opts).unwrap());
    let opts = Options {
        page: 1,
        ..Options::default()
    };
    out += &dump("DSPWDG page 2", &render(&g, "DSPWDG", &d, &opts).unwrap());
    d.set("ACTIVE", "N");
    out += &dump(
        "CHGWDG two columns",
        &render(&g, "CHGWDG", &d, &Options::default()).unwrap(),
    );
    check("data", &out);
}

#[test]
fn menu_panels() {
    let g = group("menu");
    let d = MapDialog::new();
    let opts = Options {
        show_copyright: true,
        copyright: "Copyright (C) 2026 Quartz Systems".into(),
        ..Options::default()
    };
    let mut out = String::new();
    out += &dump("QSMAIN", &render(&g, "QSMAIN", &d, &opts).unwrap());
    out += &dump("QSCMDS page 1", &render(&g, "QSCMDS", &d, &opts).unwrap());
    let opts2 = Options {
        page: 1,
        ..opts.clone()
    };
    out += &dump("QSCMDS page 2", &render(&g, "QSCMDS", &d, &opts2).unwrap());
    let mut d = MapDialog::new();
    d.set("OPERATOR", "Y");
    out += &dump(
        "QSMAIN as the operator",
        &render(&g, "QSMAIN", &d, &opts).unwrap(),
    );
    out += &dump(
        "SELWDG without a command line",
        &render(&g, "SELWDG", &d, &opts).unwrap(),
    );
    out += &dump(
        "ABOUT information panel",
        &render(&g, "ABOUT", &d, &opts).unwrap(),
    );
    check("menu", &out);
}

#[test]
fn help_panels() {
    let g = group("help");
    let mut out = String::new();
    out += &dump(
        "HLPWDG page 1",
        &render_help(&g, "HLPWDG", &Options::default()).unwrap(),
    );
    let opts = Options {
        page: 1,
        ..Options::default()
    };
    out += &dump("HLPWDG page 2", &render_help(&g, "HLPWDG", &opts).unwrap());
    out += &dump(
        "HLPMORE",
        &render_help(&g, "HLPMORE", &Options::default()).unwrap(),
    );
    check("help", &out);
}

#[test]
fn errors() {
    let g = group("help");
    assert!(render(&g, "NOSUCH", &MapDialog::new(), &Options::default()).is_err());
    assert!(render_help(&g, "NOSUCH", &Options::default()).is_err());
}
