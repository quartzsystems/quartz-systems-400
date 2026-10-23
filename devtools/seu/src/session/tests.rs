// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use super::*;
use crate::session::apply::Key;

fn member(n: u32) -> Buffer {
    Buffer::new(
        80,
        (1..=n)
            .map(|i| (i * 100, 260101, format!("     A            F{i}")))
            .collect(),
    )
}

fn session(n: u32) -> Session {
    let mut s = Session::new(Mode::Edit, member(n), "", ("MYLIB", "QDDSSRC", "MBR"), 261023);
    let _ = s.view();
    s
}

/// The display's rows as the session showed them, unchanged.
fn rows(s: &Session) -> Vec<RowInput> {
    s.shown
        .iter()
        .map(|(row, _, seq, data)| RowInput {
            row: *row,
            seq: seq.clone(),
            data: data.clone(),
        })
        .collect()
}

/// Type over row `row`'s sequence field and press Enter.
fn line_cmd(s: &mut Session, cmds: &[(usize, &str)]) -> Action {
    let mut input = Input {
        rows: rows(s),
        cursor: (2, 9),
        ..Input::default()
    };
    for (row, c) in cmds {
        let r = input.rows.iter_mut().find(|r| r.row == *row).unwrap();
        r.seq = format!("{c:<7}");
    }
    let a = s.key(Key::Enter, &input);
    let _ = s.view();
    a
}

fn data(s: &Session) -> Vec<String> {
    s.buf.records().iter().map(|r| r.data.trim().to_string()).collect()
}

#[test]
fn the_first_display() {
    let mut s = session(7);
    let v = s.view();
    assert_eq!(v.header.columns, "    1  71");
    assert_eq!(v.header.title, "Edit");
    assert_eq!(v.header.right.trim(), "MYLIB/QDDSSRC");
    assert_eq!(v.header.member.trim(), "MBR");
    assert_eq!(v.header.fmt_label, "FMT **");
    assert!(v.rows[0].data.contains("Beginning of data"));
    assert_eq!(v.rows[1].seq, "0001.00");
    assert!(v.rows[8].data.contains("End of data"));
    assert_eq!(v.rows.len(), 17);
    assert!(v.copyright);
}

#[test]
fn insert_then_type_continues() {
    let mut s = session(3);
    line_cmd(&mut s, &[(5, "i")]);
    let v = s.view();
    assert_eq!(v.rows[2].seq, "'''''''");
    assert_eq!(v.cursor, (6, 9));
    // Type into the insert line: a record, and a new insert line.
    let mut input = Input {
        rows: rows(&s),
        cursor: (6, 9),
        ..Input::default()
    };
    input.rows[2].data = "new line".into();
    s.key(Key::Enter, &input);
    let v = s.view();
    assert_eq!(s.buf.records()[1].data, "NEW LINE");
    assert_eq!(s.buf.records()[1].seq, 101);
    assert_eq!(s.buf.records()[1].date, 261023);
    assert_eq!(v.rows[3].seq, "'''''''");
    // Enter on the empty insert line ends insert mode.
    line_cmd(&mut s, &[]);
    assert!(s.inserts.is_empty());
}

#[test]
fn insert_at_the_end_numbers_by_whole_lines() {
    let mut s = session(7);
    line_cmd(&mut s, &[(11, "i")]);
    let mut input = Input {
        rows: rows(&s),
        cursor: (12, 9),
        ..Input::default()
    };
    input.rows[8].data = "dsplib".into();
    s.key(Key::Enter, &input);
    assert_eq!(s.buf.records()[7].seq, 800);
}

#[test]
fn block_pending_then_done() {
    let mut s = session(5);
    let _ = line_cmd(&mut s, &[(5, "cc")]);
    let v = s.view();
    assert_eq!(v.header.right, "Pending . . . . . :   CC");
    assert!(v.header.member.is_empty());
    assert!(s.message.is_some());
    line_cmd(&mut s, &[(6, "cc"), (8, "a")]);
    assert!(s.pending.is_empty());
    assert_eq!(data(&s), ["A            F1", "A            F2", "A            F3", "A            F4", "A            F1", "A            F2", "A            F5"]);
}

#[test]
fn move_before_and_delete() {
    let mut s = session(4);
    line_cmd(&mut s, &[(5, "m"), (8, "b")]);
    assert_eq!(data(&s)[2], "A            F1");
    assert_eq!(s.buf.len(), 4);
    line_cmd(&mut s, &[(5, "dd"), (6, "dd")]);
    assert_eq!(s.buf.len(), 2);
    line_cmd(&mut s, &[(5, "d")]);
    assert_eq!(s.buf.len(), 1);
}

#[test]
fn exclude_and_show() {
    let mut s = session(5);
    line_cmd(&mut s, &[(6, "x3")]);
    let v = s.view();
    assert_eq!(v.rows[2].seq, "- - - -");
    assert!(v.rows[2].data.contains("3 data records excluded"));
    line_cmd(&mut s, &[(6, "s")]);
    let v = s.view();
    assert_eq!(v.rows[2].seq, "0002.00");
    assert!(v.rows[3].data.contains("2 data records excluded"));
}

#[test]
fn repeat_shift_overlay() {
    let mut s = session(2);
    line_cmd(&mut s, &[(5, "rp2")]);
    assert_eq!(s.buf.len(), 4);
    line_cmd(&mut s, &[(5, "l3")]);
    assert_eq!(s.buf.records()[0].data, "  A            F1");
    line_cmd(&mut s, &[(5, "r2")]);
    assert_eq!(s.buf.records()[0].data, "    A            F1");
}

#[test]
fn bad_line_command() {
    let mut s = session(2);
    line_cmd(&mut s, &[(5, "q")]);
    assert!(s.message.as_deref().unwrap().contains("not valid"));
    assert_eq!(s.buf.len(), 2);
}

#[test]
fn browse_refuses_changes() {
    let mut s = Session::new(Mode::Browse, member(3), "", ("L", "F", "M"), 0);
    let _ = s.view();
    line_cmd(&mut s, &[(5, "d")]);
    assert_eq!(s.buf.len(), 3);
    assert!(s.message.is_some());
    let mut input = Input {
        rows: rows(&s),
        ..Input::default()
    };
    input.rows[1].data = "changed".into();
    s.key(Key::Enter, &input);
    assert_eq!(s.buf.records()[0].data, "     A            F1");
    assert_eq!(s.key(Key::F(3), &Input::default()), Action::Cancel);
}

#[test]
fn command_line() {
    let mut s = session(30);
    let input = |c: &str| Input {
        cmdline: c.into(),
        cursor: (2, 9),
        ..Input::default()
    };
    s.key(Key::Enter, &input("find F25"));
    assert_eq!(s.message.as_deref(), Some("Found F25."));
    let v = s.view();
    assert!(v.rows.iter().any(|r| r.data.contains("F25")));
    s.key(Key::Enter, &input("find zzzz"));
    assert!(s.message.as_deref().unwrap().contains("ZZZZ"));
    s.key(Key::Enter, &input("bottom"));
    let v = s.view();
    assert!(v.rows[16].data.contains("End of data"));
    s.key(Key::Enter, &input("top"));
    assert_eq!(s.top, 0);
    s.key(Key::Enter, &input("xyz"));
    assert!(s.message.is_some());
    assert_eq!(s.cmdline, "xyz");
    s.key(Key::F(9), &Input::default());
    assert_eq!(s.cmdline, "xyz");
    assert_eq!(s.key(Key::Enter, &input("cancel")), Action::Cancel);
    assert_eq!(s.key(Key::Enter, &input("exit")), Action::Exit);
    assert!(matches!(s.key(Key::Enter, &input("save")), Action::Save(_)));
}

#[test]
fn change_all() {
    let mut s = session(3);
    let input = Input {
        cmdline: "c F x all".into(),
        ..Input::default()
    };
    s.key(Key::Enter, &input);
    assert_eq!(data(&s)[0], "A            X1");
    assert!(s.buf.changed());
}

#[test]
fn roll_and_shift() {
    let mut s = session(40);
    s.key(Key::PageDown, &Input::default());
    assert_eq!(s.top, 9);
    s.key(Key::PageUp, &Input::default());
    assert_eq!(s.top, 0);
    s.key(Key::PageUp, &Input::default());
    assert!(s.message.is_some());
    s.key(Key::F(20), &Input::default());
    assert_eq!(s.left, 21);
    s.key(Key::F(19), &Input::default());
    assert_eq!(s.left, 1);
    s.key(Key::F(11), &Input::default());
    assert_eq!(s.left, 31);
    assert_eq!(s.view().header.columns, "   31  80");
    s.key(Key::F(11), &Input::default());
    assert_eq!(s.left, 1);
}

#[test]
fn rpg_window_and_dates() {
    let b = Buffer::new(100, vec![(1, 0, "     D*** x".into())]);
    let mut s = Session::new(Mode::Browse, b, "RPGLE", ("QSYSINC", "QRPGLESRC", "QUSEC"), 0);
    let v = s.view();
    assert_eq!(v.header.columns, "    6  76");
    assert_eq!(v.header.fmt_label, "FMT * ");
    assert_eq!(v.rows[1].date, None);
    s.key(Key::F(20), &Input::default());
    let v = s.view();
    assert_eq!(v.header.columns, "   41 100");
    assert_eq!(v.rows[1].date, Some((74, "000000".into())));
    s.key(Key::F(20), &Input::default());
    assert_eq!(s.left, 41);
    s.key(Key::F(19), &Input::default());
    assert_eq!(s.left, 5);
    s.key(Key::F(19), &Input::default());
    assert_eq!(s.left, 1);
}

#[test]
fn keys_and_actions() {
    let mut s = session(3);
    assert_eq!(s.key(Key::F(24), &Input::default()), Action::Show);
    assert_eq!(s.keyset, 1);
    assert_eq!(s.key(Key::F(13), &Input::default()), Action::Defaults);
    assert_eq!(s.key(Key::F(14), &Input::default()), Action::FindOptions);
    assert_eq!(s.key(Key::F(15), &Input::default()), Action::BrowseOptions);
    assert_eq!(s.key(Key::F(21), &Input::default()), Action::SystemCommand);
    assert_eq!(s.key(Key::F(3), &Input::default()), Action::Exit);
    s.key(Key::F(16), &Input::default());
    assert!(s.message.is_some());
    let at_record = Input {
        cursor: (5, 20),
        ..Input::default()
    };
    let _ = s.view();
    assert_eq!(s.key(Key::F(23), &at_record), Action::SelectPrompt(Target::Rec(1)));
    let _ = s.view();
    assert_eq!(s.key(Key::F(4), &Input { cursor: (4, 20), ..Input::default() }), Action::Show);
    assert!(s.message.is_some());
}

#[test]
fn fixed_form_prompt() {
    let mut s = session(7);
    s.set_srctype("DSPF");
    let _ = s.view();
    let at = Input {
        cursor: (7, 20),
        ..Input::default()
    };
    assert_eq!(s.key(Key::F(4), &at), Action::Show);
    assert_eq!(s.body_rows(), 7);
    let v = s.view();
    let p = v.prompt.unwrap();
    assert_eq!((p.top, p.format.as_str(), p.seq.as_str()), (11, "DP", "0003.00"));
    let mut fields: Vec<String> = p.fields.iter().map(|f| f.3.clone()).collect();
    assert_eq!(fields[3].trim(), "F3");
    fields[3] = "NAME".into();
    s.key(Key::Enter, &Input { prompt: fields, ..Input::default() });
    assert!(s.prompt.is_none());
    assert_eq!(s.buf.records()[2].data, "     A            NAME");
}

#[test]
fn insert_with_prompt_continues() {
    let mut s = session(2);
    s.set_srctype("RPGLE");
    let _ = s.view();
    line_cmd(&mut s, &[(5, "ipc")]);
    let p = s.prompt.clone().unwrap();
    assert_eq!(p.format, "C");
    let v = s.view();
    let mut fields: Vec<String> = v.prompt.unwrap().fields.iter().map(|f| f.3.clone()).collect();
    fields[3] = "EVAL".into();
    s.key(Key::Enter, &Input { prompt: fields, ..Input::default() });
    assert_eq!(s.buf.len(), 3);
    assert!(matches!(s.prompt.as_ref().unwrap().target, Target::Ins(_)));
    s.key(Key::F(12), &Input::default());
    assert!(s.prompt.is_none());
    assert!(s.inserts.is_empty());
}

#[test]
fn cl_lines_go_to_the_command_prompter() {
    let mut s = Session::new(Mode::Edit, Buffer::new(80, vec![(100, 0, "             DSPLIB".into())]), "CLP", ("L", "F", "M"), 0);
    let _ = s.view();
    let a = s.key(Key::F(4), &Input { cursor: (5, 20), ..Input::default() });
    assert!(matches!(a, Action::ClPrompt(Target::Rec(_), ref t) if t.contains("DSPLIB")));
}

#[test]
fn prompt_a_line_just_typed() {
    let mut s = Session::new(Mode::Edit, Buffer::new(80, vec![(100, 0, "PGM".into())]), "CLP", ("L", "F", "M"), 0);
    let _ = s.view();
    line_cmd(&mut s, &[(5, "i")]);
    let mut input = Input {
        rows: rows(&s),
        cursor: (6, 20),
        ..Input::default()
    };
    input.rows[2].data = "             dsplib".into();
    let a = s.key(Key::F(4), &input);
    assert!(matches!(a, Action::ClPrompt(Target::Rec(_), ref t) if t.contains("DSPLIB")), "{a:?}");
}

#[test]
fn source_type_forces_case() {
    let mut s = session(1);
    assert!(s.defaults.upper);
    assert!(s.set_srctype("RPGLE").is_some());
    assert!(!s.defaults.upper);
    assert!(s.set_srctype("RPGLE").is_none());
}
