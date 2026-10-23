// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SEU golden tests' helpers: the pub400 goldens read, members
//! rebuilt from the records they show, screens drawn and compared row
//! by row with the goldens' masks applied to our side.

#![allow(dead_code)]

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use seu::display::{Screens, plain_env, read_session, show_session};
use seu::session::{Input, Key, RowInput};
use seu::{Buffer, Mode, Session};
use tn5250::screen::ScreenModel;

/// The masks the captures put over values that differ by system.
pub const MASKS: &[&str] = &[
    "SYSNAME", "USERNAME", "NNNNNN", "QPADEV000X", "MM/DD/YY", "HH:MM:SS",
];

/// The pub400 golden directory.
pub fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/conformance/golden/verified/pub400")
}

/// A golden's rows.
pub fn golden_rows(name: &str) -> Vec<String> {
    let path = golden_dir().join(format!("{name}.txt"));
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

/// The records a golden shows: sequence number, data from its window
/// starting at source column `left`.
pub fn records_of(name: &str, left: usize) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    for row in golden_rows(name) {
        let seq = row.get(..7).unwrap_or("");
        let Some(s) = seu::parse_seq(seq).filter(|_| seq.as_bytes().get(4) == Some(&b'.')) else {
            continue;
        };
        let data: String = row.chars().skip(8).take(71).collect();
        out.push((s, format!("{}{}", " ".repeat(left - 1), data.trim_end())));
    }
    out
}

/// A member from goldens' records; numbers no golden shows are blank
/// lines so the numbering holds.
pub fn member(goldens: &[(&str, usize)], width: usize) -> Buffer {
    let mut recs: Vec<(u32, String)> = Vec::new();
    for (g, left) in goldens {
        for r in records_of(g, *left) {
            if !recs.iter().any(|x| x.0 == r.0) {
                recs.push(r);
            }
        }
    }
    recs.sort_by_key(|r| r.0);
    Buffer::new(width, recs.into_iter().map(|(s, d)| (s, 0, d)).collect())
}

/// The member the edit captures used: seven DDS records.
pub fn dds_member() -> Buffer {
    member(&[("seu-edit-insert-gone", 1)], 100)
}

/// An edit session over it.
pub fn edit_session() -> Session {
    let mut s = Session::new(Mode::Edit, dds_member(), "", ("USERNAME1", "QDDSSRC", "QDFUZZIN"), 261_023);
    let _ = s.view();
    s
}

/// The display of a session.
pub fn draw(s: &mut Session) -> ScreenModel {
    let mut screens = Screens::new(false);
    let mut m = ScreenModel::new(false);
    let v = s.view();
    show_session(&mut screens, &mut m, &plain_env(), &v);
    m
}

/// A key on a session with what its display holds, after `edit`
/// changed the fields.
pub fn key(s: &mut Session, k: Key, cursor: (usize, usize), edit: impl FnOnce(&mut Input)) {
    let m = draw(s);
    let v = s.view();
    let mut input = read_session(&m, &v, cursor);
    edit(&mut input);
    let _ = s.key(k, &input);
}

/// Type over a row's sequence field.
pub fn seq_on(input: &mut Input, row: usize, text: &str) {
    if let Some(r) = input.rows.iter_mut().find(|r: &&mut RowInput| r.row == row) {
        r.seq = format!("{text:<7}");
    }
}

/// Our row with every mask the golden row shows copied over the same
/// columns; a copyright line counts only where it starts.
fn masked(golden: &str, ours: &str) -> String {
    let mark = "(C) COPYRIGHT";
    if golden.find(mark).is_some_and(|i| ours.find(mark) == Some(i)) {
        return golden.to_string();
    }
    let mut out: Vec<char> = ours.chars().collect();
    let g: Vec<char> = golden.chars().collect();
    for m in MASKS {
        let mc: Vec<char> = m.chars().collect();
        let mut i = 0;
        while i + mc.len() <= g.len() {
            if g[i..i + mc.len()] == mc[..] {
                if out.len() < i + mc.len() {
                    out.resize(i + mc.len(), ' ');
                }
                out[i..i + mc.len()].copy_from_slice(&mc);
                i += mc.len();
            } else {
                i += 1;
            }
        }
    }
    out.into_iter().collect()
}

/// The screen against golden `name` on the rows `rows` covers (1-based,
/// inclusive); the rest (a message in IBM's words) is not compared.
pub fn compare_rows(name: &str, m: &ScreenModel, rows: std::ops::RangeInclusive<usize>) {
    let want = golden_rows(name);
    let got = m.text_rows();
    let mut diff = String::new();
    for i in rows {
        let w = want.get(i - 1).map_or("", String::as_str);
        let ours = got.get(i - 1).map_or("", String::as_str);
        let ours = masked(w, ours);
        if w.trim_end() != ours.trim_end() {
            let _ = writeln!(diff, "row {i}:\n  want |{}|\n  got  |{}|", w.trim_end(), ours.trim_end());
        }
    }
    assert!(diff.is_empty(), "{name} differs:\n{diff}");
}

/// The whole screen against a golden.
pub fn compare(name: &str, m: &ScreenModel) {
    compare_rows(name, m, 1..=24);
}
