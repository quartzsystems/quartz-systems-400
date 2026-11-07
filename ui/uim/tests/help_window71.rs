// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The help window over a command prompt against the 7.1 oracle's
//! (`tools/conformance/golden/verified/oracle71/help/`, script
//! `probes/phase12/help71.tn`): field help under the cursor's row,
//! extended help from row 2, their borders, title, indicator, keys and
//! message rows. The oracle had no help text for the command (no
//! `QHLPSYS`), so the windows hold none; the frame is what is compared.

use std::path::PathBuf;

use uim::render::{Attr, HelpView, HelpWindow, Options, Screen, render_view};

fn golden(name: &str) -> Vec<String> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/verified/oracle71/help")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end().to_string())
        .collect()
}

fn screen_of(rows: &[String]) -> Screen {
    let mut s = Screen::new(24, 80);
    for (i, r) in rows.iter().enumerate().take(24) {
        if !r.trim().is_empty() {
            s.put(i + 1, 1, Attr::Normal, r);
        }
    }
    s
}

fn window(name: &str, w: HelpWindow, keys: &[&str], message: &str) {
    let under = screen_of(&golden("prompt-addlible"));
    let view = HelpView {
        title: "Add Library List Entry".into(),
        keys: keys.iter().map(|k| (*k).to_string()).collect(),
        page: 0,
        window: Some(w),
    };
    let opts = Options {
        message: Some((message.into(), false)),
        ..Options::default()
    };
    let s = render_view(&[], &view, &opts, 24, 80, Some(&under));
    let got: Vec<String> = s
        .text_rows()
        .iter()
        .map(|r| r.trim_end().to_string())
        .collect();
    let want = golden(name);
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        assert_eq!(g, w, "{name} row {}", i + 1);
    }
}

#[test]
fn field_help_window_matches_the_oracle() {
    window(
        "help-addlible-lib",
        HelpWindow {
            top: 6,
            left: 16,
            rows: 14,
            cols: 62,
        },
        &[
            "F2=Extended help",
            "F3=Exit help",
            "F10=Move to top",
            "F12=Cancel",
            "F13=Information Assistant",
            "F14=Print help",
        ],
        "Help information is not available.",
    );
}

#[test]
fn extended_help_window_matches_the_oracle() {
    window(
        "help-addlible-ext",
        HelpWindow {
            top: 2,
            left: 2,
            rows: 18,
            cols: 76,
        },
        &[
            "F3=Exit help",
            "F10=Move to top",
            "F12=Cancel",
            "F13=Information Assistant",
            "F14=Print help",
        ],
        "Help information is not available.",
    );
}
