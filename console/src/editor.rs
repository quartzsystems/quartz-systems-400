// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! 5250 field editing on a `Screen`: the cursor lives in input fields,
//! typing fills them, Field Exit clears to the end and moves on.

use crate::keyboard::map::Key;
use crate::screen::{Field, Screen};

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
            goto_field(s, next(s, fi));
        }
        Key::FieldAdvance | Key::Down => goto_field(s, next(s, fi)),
        Key::FieldBackspace | Key::Up => goto_field(s, prev(s, fi)),
        Key::Backspace => {
            if pos > 0 {
                let f = s.fields[fi];
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
        Key::Enter | Key::Function(_) | Key::Attention | Key::Reset => {}
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
    for (i, f) in s.fields.iter().enumerate() {
        if r == f.row - 1 && c >= f.col - 1 && c < f.col - 1 + f.len {
            return (i, c - (f.col - 1));
        }
    }
    (0, 0)
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

/// Type one character; autoskip to the next field after the last cell.
fn type_char(s: &mut Screen, fi: usize, pos: usize, c: char) {
    let f = s.fields[fi];
    if s.insert {
        for i in (pos + 1..f.len).rev() {
            let ch = s.cell(f.row - 1, f.col - 1 + i - 1).ch;
            s.cell_mut(f.row - 1, f.col - 1 + i).ch = ch;
        }
    }
    s.cell_mut(f.row - 1, f.col - 1 + pos).ch = c;
    if pos + 1 < f.len {
        s.cursor = (f.row - 1, f.col - 1 + pos + 1);
    } else {
        goto_field(s, next(s, fi));
    }
}

fn clear_from(s: &mut Screen, fi: usize, pos: usize) {
    let f = s.fields[fi];
    for i in pos..f.len {
        s.cell_mut(f.row - 1, f.col - 1 + i).ch = ' ';
    }
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

#[cfg(test)]
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
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE");
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
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE");
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
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE");
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
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE");
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
        let mut s = sign_on("QS400", "QINTER", "QSCONSOLE");
        home(&mut s);
        apply(&mut s, Key::FieldAdvance);
        typed(&mut s, "secret");
        assert_eq!(s.field_text(&s.fields[1]), "secret");
        assert!(!s.dump().contains("secret"));
    }
}
