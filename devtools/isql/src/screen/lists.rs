// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The select lists behind F16, F17 and F18: Select and Sequence
//! Libraries, Files and Fields (Collections, Tables and Columns under
//! `*SQL` naming). Each entry takes a sequence number; the entries
//! numbered go into the statement in that order.

use uim::Screen;
use uim::render::{Attr, Target};

use super::{blank, input, keys, message, more, put, title};

/// Which list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Which {
    /// Libraries (collections).
    Libraries,
    /// Files (tables).
    Files,
    /// Fields (columns).
    Fields,
}

/// One entry: its sequence number as typed and its columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Entry {
    /// The sequence number typed.
    pub seq: String,
    /// The name.
    pub name: String,
    /// Library (files) or file (fields).
    pub owner: String,
    /// Type: `SYS` for a library, `PF`/`LF`/`VW`/`TB` for a file, the
    /// data type for a field.
    pub kind: String,
    /// Length (fields).
    pub length: String,
    /// Scale (fields).
    pub scale: String,
    /// Text.
    pub text: String,
}

/// The list name the sequence fields use.
pub const LIST: &str = "SEL";

/// Entries a page of the list shows.
#[must_use]
pub fn page(which: Which) -> usize {
    match which {
        Which::Fields => 14,
        _ => 15,
    }
}

fn words(w: Which, sql: bool) -> (&'static str, &'static str) {
    match (w, sql) {
        (Which::Libraries, false) => ("Libraries", "libraries"),
        (Which::Libraries, true) => ("Collections", "collections"),
        (Which::Files, false) => ("Files", "files"),
        (Which::Files, true) => ("Tables", "tables"),
        (Which::Fields, false) => ("Fields", "fields"),
        (Which::Fields, true) => ("Columns", "columns"),
    }
}

/// A name cut to `width` with ` >` after it when longer (F20 shows it
/// whole).
fn cut(name: &str, width: usize) -> String {
    if name.chars().count() > width {
        format!("{} >", name.chars().take(width).collect::<String>())
    } else {
        name.to_string()
    }
}

/// The column headings: row and `(column, text)`.
fn headings(which: Which, sql: bool) -> (usize, Vec<(usize, &'static str)>) {
    let pick = |a: &'static str, b: &'static str| if sql { b } else { a };
    match which {
        Which::Libraries => (
            5,
            vec![
                (3, "Seq"),
                (9, pick("Library", "Collection")),
                (24, "Type"),
                (31, "Text"),
            ],
        ),
        Which::Files => (
            5,
            vec![
                (2, "Seq"),
                (7, pick("File", "Table")),
                (32, pick("Library", "Collection")),
                (46, "Type"),
                (52, "Text"),
            ],
        ),
        Which::Fields => (
            6,
            vec![
                (2, "Seq"),
                (7, pick("Field", "Column")),
                (26, pick("File", "Table")),
                (45, "Type"),
                (66, "Length"),
                (74, "Scale"),
            ],
        ),
    }
}

/// One entry's columns on `row`.
fn entry_row(s: &mut Screen, which: Which, row: usize, e: &Entry) {
    match which {
        Which::Libraries => {
            put(s, row, 9, Attr::Normal, &e.name);
            put(s, row, 24, Attr::Normal, &e.kind);
            put(s, row, 31, Attr::Normal, &e.text);
        }
        Which::Files => {
            let name = cut(&e.name, 20);
            let name = name.replacen(" >", "  >", 1);
            put(s, row, 7, Attr::Normal, &name);
            put(s, row, 32, Attr::Normal, &e.owner);
            put(s, row, 46, Attr::Normal, &e.kind);
            put(s, row, 52, Attr::Normal, &e.text);
        }
        Which::Fields => {
            put(s, row, 7, Attr::Normal, &cut(&e.name, 15));
            put(s, row, 26, Attr::Normal, &e.owner);
            put(s, row, 45, Attr::Normal, &e.kind);
            put(s, row, 72 - e.length.len(), Attr::Normal, &e.length);
            put(s, row, 78 - e.scale.len(), Attr::Normal, &e.scale);
        }
    }
}

/// The function keys.
fn list_keys(s: &mut Screen, which: Which, sql: bool) {
    let pick = |a: &'static str, b: &'static str| if sql { b } else { a };
    match which {
        Which::Libraries => keys(
            s,
            &[
                (23, 2, "F3=Exit"),
                (23, 12, "F5=Refresh"),
                (23, 25, "F12=Cancel"),
            ],
        ),
        Which::Files => keys(
            s,
            &[
                (22, 2, "F3=Exit"),
                (22, 12, "F5=Refresh"),
                (22, 25, "F12=Cancel"),
                (
                    22,
                    38,
                    pick("F16=Select libraries", "F16=Select collections"),
                ),
                (23, 2, "F20=Display entire name"),
            ],
        ),
        Which::Fields => keys(
            s,
            &[
                (22, 2, "F3=Exit"),
                (22, 12, "F5=Refresh"),
                (22, 25, "F11=Display text"),
                (22, 44, "F12=Cancel"),
                (22, 57, pick("F17=Select files", "F17=Select tables")),
                (
                    23,
                    2,
                    pick(
                        "F19=Display system field names",
                        "F19=Display system column names",
                    ),
                ),
                (23, 35, "F20=Display entire name"),
            ],
        ),
    }
}

/// A list's display from entry `top`.
#[must_use]
pub fn list(
    which: Which,
    entries: &[Entry],
    top: usize,
    sql_naming: bool,
    msg: Option<&str>,
) -> Screen {
    let mut s = blank();
    let (cap, low) = words(which, sql_naming);
    let t = format!("Select and Sequence {cap}");
    title(&mut s, 41 - t.len().div_ceil(2), &t);
    put(
        &mut s,
        3,
        2,
        Attr::Blue,
        &format!("Type sequence numbers (1-999) to select {low}, press Enter."),
    );
    let (head, cols) = headings(which, sql_naming);
    for (c, h) in cols {
        put(&mut s, head, c, Attr::HighIntensity, h);
    }
    let (seq_col, seq_len) = if which == Which::Libraries {
        (3, 5)
    } else {
        (2, 3)
    };
    let per = page(which);
    for (k, e) in entries.iter().enumerate().skip(top).take(per) {
        let row = head + 1 + k - top;
        input(
            &mut s,
            row,
            seq_col,
            seq_len,
            Target::ListOption {
                list: LIST.into(),
                row_index: k,
            },
            &e.seq,
        );
        entry_row(&mut s, which, row, e);
    }
    let bottom = top + per >= entries.len();
    match which {
        Which::Libraries => more(&mut s, 22, 78, bottom),
        _ => more(&mut s, 21, 79, bottom),
    }
    list_keys(&mut s, which, sql_naming);
    message(&mut s, msg);
    if !entries.is_empty() {
        s.cursor = Some((head + 1, seq_col));
    }
    s
}

/// The entries chosen, in sequence order (ties in list order).
#[must_use]
pub fn chosen(entries: &[Entry]) -> Vec<&Entry> {
    let mut picked: Vec<(u32, usize, &Entry)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| e.seq.trim().parse::<u32>().ok().map(|n| (n, i, e)))
        .filter(|(n, _, _)| (1..=999).contains(n))
        .collect();
    picked.sort_by_key(|(n, i, _)| (*n, *i));
    picked.into_iter().map(|(_, _, e)| e).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_order() {
        let e = |seq: &str, name: &str| Entry {
            seq: seq.into(),
            name: name.into(),
            ..Entry::default()
        };
        let list = vec![
            e("2", "A"),
            e("", "B"),
            e("1", "C"),
            e("x", "D"),
            e("2", "E"),
        ];
        let names: Vec<&str> = chosen(&list).iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["C", "A", "E"]);
    }

    #[test]
    fn long_names() {
        assert_eq!(cut("DATA_MART_LIBRARY", 15), "DATA_MART_LIBRA >");
        assert_eq!(cut("AJ_START", 15), "AJ_START");
    }
}
