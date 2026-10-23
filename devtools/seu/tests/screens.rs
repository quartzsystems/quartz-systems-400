// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SEU's displays against the pub400 goldens (`tools/conformance/
//! probes/seu-*.tn`): each test puts a session in the state the capture
//! left the oracle in and compares the character grid. Rows holding a
//! message in IBM's words are left out.

mod common;

use common::{compare, compare_rows, draw, edit_session, key, member, seq_on};
use seu::defaults::Defaults;
use seu::display::{BrowseForm, Screens, plain_env};
use seu::exit::ExitForm;
use seu::find::FindOptions;
use seu::session::Key;
use seu::{Mode, Session};
use tn5250::screen::ScreenModel;

fn browse_session() -> Session {
    let b = member(&[("seu-browse", 6), ("seu-browse-pgdn", 6), ("seu-browse-bottom", 5)], 100);
    let mut s = Session::new(Mode::Browse, b, "RPGLE", ("QSYSINC", "QRPGLESRC", "QUSEC"), 261_023);
    let _ = s.view();
    s
}

fn c_session() -> Session {
    let b = member(&[("seu-browse-c", 1)], 80);
    let mut s = Session::new(Mode::Browse, b, "H", ("QSYSINC", "H", "QUSEC"), 261_023);
    let _ = s.view();
    s
}

fn none(_: &mut seu::session::Input) {}

#[test]
fn seu_browse() {
    let mut s = browse_session();
    compare("seu-browse", &draw(&mut s));
}

#[test]
fn seu_browse_f24() {
    let mut s = browse_session();
    key(&mut s, Key::F(24), (2, 9), none);
    compare("seu-browse-f24", &draw(&mut s));
    key(&mut s, Key::F(24), (2, 9), none);
    compare("seu-browse-f24-2", &draw(&mut s));
}

#[test]
fn seu_browse_pgdn() {
    let mut s = browse_session();
    s.keyset = 2;
    key(&mut s, Key::PageDown, (2, 9), none);
    compare("seu-browse-pgdn", &draw(&mut s));
}

#[test]
fn seu_browse_bottom() {
    let mut s = browse_session();
    s.keyset = 2;
    s.left = 5;
    key(&mut s, Key::Enter, (2, 9), |i| i.cmdline = "bottom".into());
    compare("seu-browse-bottom", &draw(&mut s));
}

#[test]
fn seu_shift() {
    let mut s = browse_session();
    s.copyright = false;
    key(&mut s, Key::F(20), (2, 9), none);
    compare("seu-shift-r1", &draw(&mut s));
    key(&mut s, Key::F(20), (2, 9), none);
    compare("seu-shift-r2", &draw(&mut s));
    key(&mut s, Key::F(19), (2, 9), none);
    compare("seu-shift-l1", &draw(&mut s));
    key(&mut s, Key::F(19), (2, 9), none);
    compare("seu-shift-l2", &draw(&mut s));
    key(&mut s, Key::F(11), (2, 9), none);
    compare("seu-shift-t1", &draw(&mut s));
    key(&mut s, Key::F(11), (2, 9), none);
    compare("seu-shift-t2", &draw(&mut s));
}

#[test]
fn seu_browse_c() {
    let mut s = c_session();
    compare("seu-browse-c", &draw(&mut s));
    key(&mut s, Key::F(11), (2, 9), none);
    compare("seu-browse-c-f11", &draw(&mut s));
}

#[test]
fn seu_browse_f16() {
    let mut s = browse_session();
    s.keyset = 2;
    s.left = 5;
    key(&mut s, Key::F(16), (2, 9), none);
    compare_rows("seu-browse-f16", &draw(&mut s), 1..=23);
}

#[test]
fn seu_edit() {
    let mut s = edit_session();
    s.copyright = false;
    compare("seu-edit", &draw(&mut s));
    key(&mut s, Key::F(24), (2, 9), none);
    compare("seu-edit-f24", &draw(&mut s));
    key(&mut s, Key::F(24), (2, 9), none);
    compare("seu-edit-f24-2", &draw(&mut s));
}

#[test]
fn seu_edit_f11_f20() {
    let mut s = edit_session();
    key(&mut s, Key::F(11), (2, 9), none);
    compare("seu-edit-f11", &draw(&mut s));
    key(&mut s, Key::F(11), (2, 9), none);
    key(&mut s, Key::F(20), (2, 9), none);
    compare("seu-edit-f20", &draw(&mut s));
}

#[test]
fn seu_edit_messages() {
    let mut s = edit_session();
    key(&mut s, Key::F(16), (2, 9), none);
    compare_rows("seu-edit-f16", &draw(&mut s), 1..=23);
    key(&mut s, Key::Enter, (2, 9), |i| i.cmdline = "find IID".into());
    compare_rows("seu-edit-find", &draw(&mut s), 1..=3);
    key(&mut s, Key::Enter, (2, 9), |i| i.cmdline = "XYZ".into());
    compare_rows("seu-edit-badcmd", &draw(&mut s), 1..=23);
}

#[test]
fn seu_edit_insert() {
    let mut s = edit_session();
    key(&mut s, Key::Enter, (2, 9), |i| seq_on(i, 5, "i"));
    compare_rows("seu-edit-insert", &draw(&mut s), 1..=23);
    key(&mut s, Key::Enter, (6, 9), none);
    compare_rows("seu-edit-insert-gone", &draw(&mut s), 1..=23);
}

#[test]
fn seu_edit_pending() {
    let mut s = edit_session();
    key(&mut s, Key::Enter, (2, 9), |i| seq_on(i, 5, "cc"));
    compare_rows("seu-edit-pending", &draw(&mut s), 1..=23);
}

#[test]
fn seu_edit_exclude() {
    let mut s = edit_session();
    key(&mut s, Key::Enter, (2, 9), |i| seq_on(i, 6, "x"));
    compare_rows("seu-edit-exclude", &draw(&mut s), 1..=23);
}

#[test]
fn seu_edit_f4() {
    let mut s = edit_session();
    key(&mut s, Key::F(4), (5, 20), none);
    compare_rows("seu-edit-f4", &draw(&mut s), 1..=23);
}

#[test]
fn seu_prompt_dds() {
    let mut s = edit_session();
    s.set_srctype("DSPF");
    let _ = s.view();
    key(&mut s, Key::F(4), (7, 20), none);
    compare_rows("seu-prompt-dds", &draw(&mut s), 1..=23);
    key(&mut s, Key::F(24), (7, 20), none);
    compare_rows("seu-prompt-dds-f24", &draw(&mut s), 1..=23);
}

#[test]
fn seu_prompt_dds_ip() {
    let mut s = edit_session();
    s.set_srctype("DSPF");
    let _ = s.view();
    key(&mut s, Key::Enter, (2, 9), |i| seq_on(i, 7, "ip"));
    compare_rows("seu-prompt-dds-ip", &draw(&mut s), 1..=23);
}

/// The fixed-form RPG prompts: the prompt area, from its first row.
#[test]
fn seu_prompt_rpg() {
    for (fmt, golden) in [
        ("h", "seu-prompt-rpg-h"),
        ("f", "seu-prompt-rpg-f"),
        ("d", "seu-prompt-rpg-d"),
        ("c", "seu-prompt-rpg-c"),
        ("cx", "seu-prompt-rpg-cx"),
        ("o", "seu-prompt-rpg-o"),
        ("p", "seu-prompt-rpg-p"),
        ("i", "seu-prompt-rpg-i"),
    ] {
        let mut s = edit_session();
        s.set_srctype("RPGLE");
        let _ = s.view();
        key(&mut s, Key::Enter, (2, 9), |i| seq_on(i, 7, &format!("ip{fmt}")));
        let top = s.view().prompt.expect("a prompt").top;
        compare_rows(golden, &draw(&mut s), top..=23);
        compare_rows(golden, &draw(&mut s), 1..=1);
    }
}

#[test]
fn seu_edit_dspf_format_line() {
    let mut s = edit_session();
    s.set_srctype("DSPF");
    compare_rows("seu-edit-dspf", &draw(&mut s), 3..=3);
    s.set_srctype("RPGLE");
    s.left = 6;
    compare_rows("seu-edit-rpgle", &draw(&mut s), 1..=3);
}

fn screen() -> (Screens, ScreenModel) {
    (Screens::new(false), ScreenModel::new(false))
}

#[test]
fn seu_defaults() {
    let (mut sc, mut m) = screen();
    sc.show_defaults(&mut m, &plain_env(), Mode::Edit, 0, &Defaults::default());
    compare("seu-defaults", &m);
    let (mut sc, mut m) = screen();
    sc.show_defaults(&mut m, &plain_env(), Mode::Edit, 1, &Defaults::default());
    compare("seu-defaults-2", &m);
    let (mut sc, mut m) = screen();
    sc.show_defaults(&mut m, &plain_env(), Mode::Browse, 0, &Defaults::default());
    compare("seu-browse-f13", &m);
}

#[test]
fn seu_find_options() {
    let (mut sc, mut m) = screen();
    sc.show_find(&mut m, &plain_env(), Mode::Edit, &FindOptions::new(), 100);
    compare("seu-findchg", &m);
    let (mut sc, mut m) = screen();
    sc.show_find(&mut m, &plain_env(), Mode::Browse, &FindOptions::new(), 100);
    compare("seu-browse-f14", &m);
}

#[test]
fn seu_browse_options() {
    let (mut sc, mut m) = screen();
    let b = BrowseForm::new("QDFUZZIN", "QDDSSRC", "USERNAME1", "USERNAME");
    sc.show_browse(&mut m, &plain_env(), Mode::Edit, &b);
    compare("seu-browsecopy", &m);
    let (mut sc, mut m) = screen();
    let b = BrowseForm::new("QUSEC", "QRPGLESRC", "QSYSINC", "USERNAME");
    sc.show_browse(&mut m, &plain_env(), Mode::Browse, &b);
    compare("seu-browse-f15", &m);
}

#[test]
fn seu_exit() {
    let (mut sc, mut m) = screen();
    let e = ExitForm::new("QDFUZZIN", "QDDSSRC", "USERNAME1", "", true, true, false);
    sc.show_exit(&mut m, &plain_env(), &e);
    compare("seu-exit", &m);
}

#[test]
fn seu_edit_f21() {
    let mut s = edit_session();
    s.copyright = false;
    let mut m = draw(&mut s);
    let mut sc = Screens::new(false);
    sc.show_window(&mut m, &plain_env(), "", None);
    compare("seu-edit-f21", &m);
    sc.show_window(&mut m, &plain_env(), "strseu srcfile(*curlib/qddssrc) srcmbr(qdfuzzin) option(2)", Some("in use"));
    compare_rows("seu-edit-inuse", &m, 18..=22);
}

#[test]
fn seu_select_prompt() {
    let (mut sc, mut m) = screen();
    let lines = seu::srctype::select_lines(&seu::srctype::SourceType::new("RPGLE"));
    sc.show_select(&mut m, &plain_env(), &lines, None);
    compare_rows("seu-select-rpgle", &m, 1..=5);
    compare_rows("seu-select-rpgle", &m, 21..=24);
}

/// `seu-lines.tn`: a CL line with an error, then the line commands in
/// turn on the same session.
#[test]
fn seu_lines() {
    let mut s = edit_session();
    s.set_srctype("CLP");
    let _ = s.view();
    key(&mut s, Key::Enter, (2, 9), |i| seq_on(i, 11, "i"));
    key(&mut s, Key::Enter, (12, 9), |i| {
        if let Some(r) = i.rows.iter_mut().find(|r| r.row == 12) {
            r.data = "     DSPLIB LIB(".into();
        }
    });
    compare_rows("seu-syntax-cl", &draw(&mut s), 1..=23);
    key(&mut s, Key::Enter, (2, 9), |i| seq_on(i, 12, "d"));
    compare_rows("seu-syntax-cl-gone", &draw(&mut s), 1..=23);
    let steps: [(&[(usize, &str)], &str); 8] = [
        (&[(5, "c"), (7, "a")], "seu-lc-copy"),
        (&[(5, "dd"), (6, "dd")], "seu-lc-delete"),
        (&[(5, "m"), (8, "b")], "seu-lc-move"),
        (&[(5, "rp")], "seu-lc-repeat"),
        (&[(5, "r3")], "seu-lc-shift-right"),
        (&[(5, "l2")], "seu-lc-shift-left"),
        (&[(5, "f")], "seu-lc-format"),
        (&[(7, "w20")], "seu-lc-window"),
    ];
    for (cmds, golden) in steps {
        key(&mut s, Key::Enter, (2, 9), |i| {
            for (row, c) in cmds {
                if let Some(r) = i.rows.iter_mut().find(|r| r.row == *row) {
                    // Typed over the number: the rest of it stays.
                    let mut typed: Vec<char> = c.chars().collect();
                    typed.extend(r.seq.chars().skip(typed.len()));
                    r.seq = typed.into_iter().collect();
                }
            }
        });
        compare_rows(golden, &draw(&mut s), 1..=23);
    }
}

#[test]
fn seu_exit_changed() {
    let (mut sc, mut m) = screen();
    let e = ExitForm::new("QDFUZZIN", "QDDSSRC", "USERNAME1", "", true, true, true);
    sc.show_exit(&mut m, &plain_env(), &e);
    compare_rows("seu-exit-changed", &m, 1..=23);
}
