// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The hand-written task menus under `sys/` against the pub400
//! goldens: each panel rendered and compared with the golden's
//! character grid row by row. The message line (the copyright) may
//! differ, and a row may say `QS/400` where the golden says `IBM i`.

use std::path::{Path, PathBuf};

use uim::render::{MapDialog, Options, render};
use uim::{CompileOpts, NoImbeds, NoImports};

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/verified/pub400")
}

/// Words of a row, for the rows renamed from `IBM i`.
fn words(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn check(golden: &str, menu: &str, page: usize) {
    compare(golden, menu, page, false);
}

/// `title_words`: the title row matches by its words only.
fn compare(golden: &str, menu: &str, page: usize, title_words: bool) {
    let path = golden_dir().join(format!("{golden}.txt"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let theirs: Vec<&str> = text.lines().collect();
    let source = menus::shipped::source(menu).unwrap_or_else(|| panic!("{menu} is not shipped"));
    let c = uim::compile(source, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{menu}:\n{}", c.listing);
    let group = c.group.expect("group");
    let opts = Options {
        system: "SYSNAME".into(),
        show_copyright: true,
        copyright: "(C) COPYRIGHT QUARTZ SYSTEMS 2026.".into(),
        page,
        ..Options::default()
    };
    let screen = render(&group, menu, &MapDialog::new(), &opts).expect("render");
    let ours = screen.text_rows();
    assert_eq!(ours.len(), 24, "{menu}: rows");
    let mut bad = Vec::new();
    // The last row is the message line.
    for (i, our) in ours.iter().enumerate().take(23) {
        let their = theirs.get(i).copied().unwrap_or("").trim_end();
        let our = our.trim_end();
        if our == their {
            continue;
        }
        if i == 0 && title_words && words(their) == words(our) {
            continue;
        }
        if their.contains("IBM i") && words(&their.replace("IBM i", "QS/400")) == words(our) {
            continue;
        }
        bad.push(format!("row {:>2}\n  ours:   |{our}|\n  golden: |{their}|", i + 1));
    }
    assert!(bad.is_empty(), "{menu} against {golden}:\n{}", bad.join("\n"));
}

macro_rules! menu_test {
    ($($test:ident: $golden:literal, $menu:literal, $page:literal;)*) => {
        $(
            #[test]
            fn $test() {
                check($golden, $menu, $page);
            }
        )*
    };
}

menu_test! {
    menu_user: "menu-user", "USER", 0;
    menu_user2: "menu-user2", "USER2", 0;
    menu_ofctsk: "menu-ofctsk", "OFCTSK", 0;
    menu_system: "menu-system", "SYSTEM", 0;
    menu_system2: "menu-system2", "SYSTEM2", 0;
    menu_data: "menu-data", "DATA", 0;
    menu_program: "menu-program", "PROGRAM", 0;
    menu_cmn: "menu-cmn", "CMN", 0;
    menu_define: "menu-define", "DEFINE", 0;
    menu_problem: "menu-problem", "PROBLEM", 0;
    menu_problem2: "menu-problem2", "PROBLEM2", 0;
    menu_backup: "menu-backup", "BACKUP", 0;
    menu_cfg: "menu-cfg", "CFG", 0;
    menu_cleanup: "menu-cleanup", "CLEANUP", 0;
    menu_device: "menu-device", "DEVICE", 0;
    menu_devicests: "menu-devicests", "DEVICESTS", 0;
    menu_disktasks: "menu-disktasks", "DISKTASKS", 0;
    menu_document: "menu-document", "DOCUMENT", 0;
    menu_file: "menu-file", "FILE", 0;
    menu_file2: "menu-file2", "FILE2", 0;
    menu_hardware: "menu-hardware", "HARDWARE", 0;
    menu_job: "menu-job", "JOB", 0;
    menu_job2: "menu-job2", "JOB2", 0;
    menu_library: "menu-library", "LIBRARY", 0;
    menu_message: "menu-message", "MESSAGE", 0;
    menu_network: "menu-network", "NETWORK", 0;
    menu_network_2: "menu-network-2", "NETWORK", 1;
    menu_power: "menu-power", "POWER", 0;
    menu_printer: "menu-printer", "PRINTER", 0;
    menu_ptf: "menu-ptf", "PTF", 0;
    menu_ptf_2: "menu-ptf-2", "PTF", 1;
    menu_recovery: "menu-recovery", "RECOVERY", 0;
    menu_recovery_2: "menu-recovery-2", "RECOVERY", 1;
    menu_restore: "menu-restore", "RESTORE", 0;
    menu_restore_2: "menu-restore-2", "RESTORE", 1;
    menu_save: "menu-save", "SAVE", 0;
    menu_save_2: "menu-save-2", "SAVE", 1;
    menu_security: "menu-security", "SECURITY", 0;
    menu_setup: "menu-setup", "SETUP", 0;
    menu_software: "menu-software", "SOFTWARE", 0;
    menu_status: "menu-status", "STATUS", 0;
    menu_support: "menu-support", "SUPPORT", 0;
    menu_tcpadm: "menu-tcpadm", "TCPADM", 0;
    menu_techhelp: "menu-techhelp", "TECHHELP", 0;
    information_assistant: "information-assistant", "INFO", 0;
    main_menu: "main-menu", "MAIN", 0;
}

/// IBM's ASSIST is a display-file menu whose title sits one column right
/// of where a panel group centers it; the title matches by its words.
#[test]
fn menu_assist() {
    compare("menu-assist", "ASSIST", 0, true);
}

/// ACCESS has no golden: it compiles and renders as a menu with a
/// command line.
#[test]
fn access_menu() {
    let source = menus::shipped::source("ACCESS").expect("ACCESS");
    let c = uim::compile(source, &NoImbeds, &NoImports, &CompileOpts::default());
    assert!(c.ok(), "{}", c.listing);
    let group = c.group.expect("group");
    let screen = render(&group, "ACCESS", &MapDialog::new(), &Options::default()).expect("render");
    let rows = screen.text_rows();
    assert!(rows[0].contains("Access Tasks"));
    assert!(rows.iter().any(|r| r.trim() == "===>"));
}

/// Every shipped menu compiles, holds a panel named like it, and its
/// help module renders.
#[test]
fn every_task_menu_compiles_with_its_help() {
    for (name, text) in menus::shipped::FILES {
        let c = uim::compile(text, &NoImbeds, &NoImports, &CompileOpts::default());
        assert!(c.ok(), "{name}:\n{}", c.listing);
        let g = c.group.expect("group");
        let p = g.panel(name).unwrap_or_else(|| panic!("{name}: no panel named like the menu"));
        if let Some(h) = &p.help {
            uim::render::render_help(&g, h, &Options::default())
                .unwrap_or_else(|e| panic!("{name}: help {h}: {e}"));
        }
    }
}
