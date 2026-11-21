// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! 5250 field editing on a `Screen`: the cursor lives in input fields,
//! typing fills them, Field Exit clears to the end and moves on. A
//! click puts the cursor in a field, or presses the key whose label it
//! is on (`F3=Exit`, `Enter=Continue`).

use crate::keyboard::map::Key;
use crate::screen::{COLS, Field, ROWS, Screen};

/// What a key did, for the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Handled within the screen.
    Edited,
    /// Enter was pressed; the caller decides what the screen means.
    Enter,
    /// A function key.
    Function(u8),
    /// Attention.
    Attention,
    /// Reset.
    Reset,
    /// Nothing to do (no fields, or key unused here).
    Ignored,
}

/// Apply a key to the screen.
pub fn apply(s: &mut Screen, key: Key) -> Outcome {
    match key {
        Key::Enter => return Outcome::Enter,
        Key::Function(n) => return Outcome::Function(n),
        Key::Attention => return Outcome::Attention,
        Key::Reset => return Outcome::Reset,
        Key::SysReq | Key::PageUp | Key::PageDown => return Outcome::Ignored,
        Key::Pointer { row, col } => return click(s, usize::from(row), usize::from(col)),
        _ => {}
    }
    if s.fields.is_empty() {
        return Outcome::Ignored;
    }
    let (fi, pos) = locate(s);
    match key {
        Key::Char(c) => type_char(s, fi, pos, c),
        Key::FieldExit => {
            clear_from(s, fi, pos);
            s.hidden[fi] = None;
            goto_field(s, next(s, fi));
        }
        Key::FieldAdvance | Key::Down => goto_field(s, next(s, fi)),
        Key::FieldBackspace | Key::Up => goto_field(s, prev(s, fi)),
        Key::Backspace => {
            let f = s.fields[fi];
            if pos + 1 == f.len && s.hidden[fi].is_some() {
                // The hidden tail goes first, then the last cell.
                let h = s.hidden[fi].as_mut().expect("some");
                if h.pop().is_none() {
                    s.hidden[fi] = None;
                    s.cell_mut(f.row - 1, f.col - 1 + pos).ch = ' ';
                }
            } else if pos > 0 {
                shift_left(s, &f, pos - 1);
                s.cursor = (f.row - 1, f.col - 1 + pos - 1);
            } else {
                goto_field(s, prev(s, fi));
            }
        }
        Key::Delete => {
            let f = s.fields[fi];
            shift_left(s, &f, pos);
        }
        Key::Insert => s.insert = !s.insert,
        Key::Home => goto_field(s, 0),
        Key::Left => {
            if pos > 0 {
                let f = s.fields[fi];
                s.cursor = (f.row - 1, f.col - 1 + pos - 1);
            } else {
                let p = prev(s, fi);
                let f = s.fields[p];
                s.cursor = (f.row - 1, f.col - 1 + f.len - 1);
            }
        }
        Key::Right => {
            let f = s.fields[fi];
            if pos + 1 < f.len {
                s.cursor = (f.row - 1, f.col - 1 + pos + 1);
            } else {
                goto_field(s, next(s, fi));
            }
        }
        Key::Enter
        | Key::Function(_)
        | Key::Attention
        | Key::SysReq
        | Key::Reset
        | Key::PageUp
        | Key::PageDown
        | Key::Pointer { .. } => {}
    }
    Outcome::Edited
}

/// Put the cursor at the start of the first field.
pub fn home(s: &mut Screen) {
    if !s.fields.is_empty() {
        goto_field(s, 0);
    }
}

/// Field index and offset of the cursor; the first field when the cursor
/// is outside every field.
fn locate(s: &Screen) -> (usize, usize) {
    let (r, c) = s.cursor;
    field_at(s, r, c).unwrap_or((0, 0))
}

/// The field holding a 0-based cell, and the offset in it.
fn field_at(s: &Screen, r: usize, c: usize) -> Option<(usize, usize)> {
    s.fields
        .iter()
        .position(|f| r + 1 == f.row && c + 1 >= f.col && c + 1 < f.col + f.len)
        .map(|i| (i, c + 1 - s.fields[i].col))
}

/// A click on a 0-based cell: the cursor into a field, or a key's label
/// pressed.
fn click(s: &mut Screen, row: usize, col: usize) -> Outcome {
    if row >= ROWS || col >= COLS {
        return Outcome::Ignored;
    }
    if field_at(s, row, col).is_some() {
        s.cursor = (row, col);
        return Outcome::Edited;
    }
    match hot_spot(s, row, col) {
        Some(Key::Enter) => Outcome::Enter,
        Some(Key::Function(n)) => Outcome::Function(n),
        _ => Outcome::Ignored,
    }
}

/// The key whose label is on a 0-based cell: `F1=` to `F24=` or `Enter=`
/// at the start of a word, up to the next label or two blanks in a row.
#[must_use]
pub fn hot_spot(s: &Screen, row: usize, col: usize) -> Option<Key> {
    if row >= ROWS || col >= COLS {
        return None;
    }
    let text: Vec<char> = (0..COLS)
        .map(|c| {
            let cell = s.cell(row, c);
            if cell.attr.nondisplay { ' ' } else { cell.ch }
        })
        .collect();
    let labels: Vec<(usize, Key)> = (0..COLS)
        .filter(|&i| i == 0 || text[i - 1] == ' ')
        .filter_map(|i| label(&text[i..]).map(|k| (i, k)))
        .collect();
    for (n, &(start, key)) in labels.iter().enumerate() {
        let limit = labels.get(n + 1).map_or(COLS, |l| l.0);
        let mut end = start;
        while end < limit && !(text[end] == ' ' && text.get(end + 1).is_none_or(|&c| c == ' ')) {
            end += 1;
        }
        if (start..end).contains(&col) {
            return Some(key);
        }
    }
    None
}

/// The key a label starts with.
fn label(t: &[char]) -> Option<Key> {
    if t.starts_with(&['E', 'n', 't', 'e', 'r', '=']) {
        return Some(Key::Enter);
    }
    if t.first() != Some(&'F') {
        return None;
    }
    let digits = t[1..].iter().take_while(|c| c.is_ascii_digit()).count();
    if !(1..=2).contains(&digits) || t.get(1 + digits) != Some(&'=') {
        return None;
    }
    let n: u8 = t[1..=digits].iter().collect::<String>().parse().ok()?;
    (1..=24).contains(&n).then_some(Key::Function(n))
}

fn next(s: &Screen, fi: usize) -> usize {
    (fi + 1) % s.fields.len()
}

fn prev(s: &Screen, fi: usize) -> usize {
    (fi + s.fields.len() - 1) % s.fields.len()
}

fn goto_field(s: &mut Screen, fi: usize) {
    let f = s.fields[fi];
    s.cursor = (f.row - 1, f.col - 1);
}

/// Type one character; autoskip to the next field after the last cell,
/// unless the field holds more than it shows: then the cursor stays on
/// the last cell and the rest is kept hidden.
fn type_char(s: &mut Screen, fi: usize, pos: usize, c: char) {
    let f = s.fields[fi];
    let deep = f.capacity > f.len;
    if deep && pos + 1 == f.len && s.hidden[fi].is_some() {
        let h = s.hidden[fi].as_mut().expect("some");
        if f.len + h.chars().count() < f.capacity {
            h.push(c);
        }
        return;
    }
    if s.insert {
        for i in (pos + 1..f.len).rev() {
            let ch = s.cell(f.row - 1, f.col - 1 + i - 1).ch;
            s.cell_mut(f.row - 1, f.col - 1 + i).ch = ch;
        }
    }
    s.cell_mut(f.row - 1, f.col - 1 + pos).ch = c;
    if pos + 1 < f.len {
        s.cursor = (f.row - 1, f.col - 1 + pos + 1);
    } else if deep {
        s.hidden[fi] = Some(String::new());
    } else {
        goto_field(s, next(s, fi));
    }
}

fn clear_from(s: &mut Screen, fi: usize, pos: usize) {
    let f = s.fields[fi];
    for i in pos..f.len {
        s.cell_mut(f.row - 1, f.col - 1 + i).ch = ' ';
    }
    s.hidden[fi] = None;
}

/// Remove the character at `pos`, pulling the rest of the field left.
fn shift_left(s: &mut Screen, f: &Field, pos: usize) {
    for i in pos..f.len {
        let ch = if i + 1 < f.len {
            s.cell(f.row - 1, f.col - 1 + i + 1).ch
        } else {
            ' '
        };
        s.cell_mut(f.row - 1, f.col - 1 + i).ch = ch;
    }
}

#[cfg(all(test, feature = "session"))]
mod tests {
    use super::*;
    use crate::screens::sign_on;

    fn typed(s: &mut Screen, text: &str) {
        for c in text.chars() {
            assert_eq!(apply(s, Key::Char(c)), Outcome::Edited);
        }
    }

    #[test]
    fn typing_fills_fields_and_autoskips() {
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE", false);
        home(&mut s);
        typed(&mut s, "QSECOFR");
        assert_eq!(s.field_text(&s.fields[0]), "QSECOFR");
        assert_eq!(s.cursor, (5, 52 + 7));
        typed(&mut s, "XYZ");
        assert_eq!(s.field_text(&s.fields[0]), "QSECOFRXYZ");
        assert_eq!(s.cursor, (6, 52), "autoskip to the password field");
    }

    #[test]
    fn field_exit_clears_and_advances() {
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE", false);
        home(&mut s);
        typed(&mut s, "ABCDEF");
        apply(&mut s, Key::Left);
        apply(&mut s, Key::Left);
        apply(&mut s, Key::FieldExit);
        assert_eq!(s.field_text(&s.fields[0]), "ABCD");
        assert_eq!(s.cursor, (6, 52));
    }

    #[test]
    fn backspace_delete_insert() {
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE", false);
        home(&mut s);
        typed(&mut s, "ABC");
        apply(&mut s, Key::Backspace);
        assert_eq!(s.field_text(&s.fields[0]), "AB");
        apply(&mut s, Key::Left);
        apply(&mut s, Key::Left);
        apply(&mut s, Key::Delete);
        assert_eq!(s.field_text(&s.fields[0]), "B");
        apply(&mut s, Key::Insert);
        typed(&mut s, "Z");
        assert_eq!(s.field_text(&s.fields[0]), "ZB");
    }

    #[test]
    fn navigation_and_outcomes() {
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE", false);
        home(&mut s);
        apply(&mut s, Key::FieldAdvance);
        assert_eq!(s.cursor, (6, 52));
        apply(&mut s, Key::FieldBackspace);
        assert_eq!(s.cursor, (5, 52));
        apply(&mut s, Key::Up);
        assert_eq!(s.cursor, (9, 52), "wraps to the last field");
        apply(&mut s, Key::Home);
        assert_eq!(s.cursor, (5, 52));
        assert_eq!(apply(&mut s, Key::Enter), Outcome::Enter);
        assert_eq!(apply(&mut s, Key::Function(3)), Outcome::Function(3));
        assert_eq!(apply(&mut s, Key::Reset), Outcome::Reset);
        let mut none = Screen::new();
        assert_eq!(apply(&mut none, Key::Char('x')), Outcome::Ignored);
    }

    #[test]
    fn nondisplay_field_hides_text_but_keeps_it() {
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE", false);
        home(&mut s);
        apply(&mut s, Key::FieldAdvance);
        typed(&mut s, "secret");
        assert_eq!(s.field_text(&s.fields[1]), "secret");
        assert!(!s.dump().contains("secret"));
    }

    #[test]
    fn a_click_in_a_field_moves_the_cursor_and_outside_does_nothing() {
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE", false);
        home(&mut s);
        let f = s.fields[2];
        let (row, col) = (f.row - 1, f.col - 1 + 3);
        let at = |r: usize, c: usize| Key::Pointer {
            row: u16::try_from(r).unwrap(),
            col: u16::try_from(c).unwrap(),
        };
        assert_eq!(apply(&mut s, at(row, col)), Outcome::Edited);
        assert_eq!(s.cursor, (row, col));
        typed(&mut s, "X");
        assert_eq!(s.field_text(&f), "   X");
        // The last cell of a field is in it; the one after is not.
        assert_eq!(
            apply(&mut s, at(row, f.col - 1 + f.len - 1)),
            Outcome::Edited
        );
        let before = (s.dump(), s.cursor);
        assert_eq!(apply(&mut s, at(row, f.col - 1 + f.len)), Outcome::Ignored);
        assert_eq!(apply(&mut s, at(0, 0)), Outcome::Ignored);
        // Past the grid: nothing, and no panic.
        assert_eq!(apply(&mut s, at(24, 0)), Outcome::Ignored);
        assert_eq!(
            apply(
                &mut s,
                Key::Pointer {
                    row: u16::MAX,
                    col: u16::MAX
                }
            ),
            Outcome::Ignored
        );
        assert_eq!((s.dump(), s.cursor), before);
        // A screen without fields ignores a click too.
        let mut none = Screen::new();
        assert_eq!(apply(&mut none, at(5, 5)), Outcome::Ignored);
    }

    #[test]
    fn a_click_on_a_label_presses_its_key() {
        use crate::screen::Attr;
        let mut s = Screen::new();
        s.put(
            23,
            2,
            "F3=Exit   F5=Refresh   F11=Display names only",
            Attr::GREEN,
        );
        s.put(22, 2, "Enter=Continue  F12=Cancel", Attr::GREEN);
        s.put(10, 2, "XF4=no  word F99=no", Attr::GREEN);
        let at = |r: usize, c: usize| {
            let mut t = s.clone();
            apply(
                &mut t,
                Key::Pointer {
                    row: u16::try_from(r - 1).unwrap(),
                    col: u16::try_from(c - 1).unwrap(),
                },
            )
        };
        assert_eq!(at(23, 2), Outcome::Function(3));
        assert_eq!(at(23, 8), Outcome::Function(3), "the label's last letter");
        assert_eq!(at(23, 9), Outcome::Ignored, "the blanks after it");
        assert_eq!(at(23, 12), Outcome::Function(5));
        assert_eq!(
            at(23, 40),
            Outcome::Function(11),
            "a label of several words"
        );
        assert_eq!(at(23, 47), Outcome::Ignored);
        assert_eq!(at(22, 2), Outcome::Enter);
        assert_eq!(at(22, 15), Outcome::Enter);
        assert_eq!(at(22, 20), Outcome::Function(12));
        assert_eq!(at(10, 3), Outcome::Ignored, "not at a word's start");
        assert_eq!(at(10, 15), Outcome::Ignored, "no F99");
        assert_eq!(hot_spot(&s, 22, 1), Some(Key::Function(3)));
        assert_eq!(hot_spot(&s, 99, 1), None);
    }

    #[test]
    fn a_click_on_the_last_cell_of_a_deep_field_keeps_its_tail() {
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE", true);
        home(&mut s);
        apply(&mut s, Key::FieldExit);
        let f = s.fields[1];
        let long: String = (b'a'..=b'z').cycle().take(40).map(char::from).collect();
        typed(&mut s, &long);
        let last = Key::Pointer {
            row: u16::try_from(f.row - 1).unwrap(),
            col: u16::try_from(f.col - 1 + f.len - 1).unwrap(),
        };
        apply(&mut s, Key::Home);
        assert_eq!(apply(&mut s, last), Outcome::Edited);
        assert_eq!(s.cursor, (f.row - 1, f.col - 1 + f.len - 1));
        typed(&mut s, "Z");
        assert_eq!(s.field_text(&f), format!("{long}Z"), "typed onto the tail");
        assert_eq!(
            s.cursor,
            (f.row - 1, f.col - 1 + f.len - 1),
            "still on the last cell"
        );
    }

    #[test]
    fn a_long_password_keeps_typing_past_the_cells() {
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE", true);
        home(&mut s);
        typed(&mut s, "ANNE");
        apply(&mut s, Key::FieldExit);
        let f = s.fields[1];
        assert_eq!(f.capacity, 128);
        let long: String = (b'a'..=b'z').cycle().take(40).map(char::from).collect();
        typed(&mut s, &long);
        assert_eq!(
            s.cursor,
            (f.row - 1, f.col - 1 + f.len - 1),
            "stays on the last cell"
        );
        assert_eq!(s.field_text(&f), long);
        apply(&mut s, Key::Backspace);
        assert_eq!(s.field_text(&f), long[..39]);
        for _ in 0..12 {
            apply(&mut s, Key::Backspace);
        }
        assert_eq!(s.field_text(&f), long[..27]);
        assert_eq!(
            s.cursor,
            (f.row - 1, f.col - 1 + f.len - 1),
            "still on the last cell"
        );
        // Field Exit on the last cell clears it and moves on; the rest stays.
        apply(&mut s, Key::FieldExit);
        assert_eq!(s.field_text(&f), long[..27]);
        assert_eq!(s.cursor, (s.fields[2].row - 1, s.fields[2].col - 1));
    }
}
