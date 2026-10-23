// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A DST screen (the console's cell grid) as a `uim::Screen` a job puts
//! on its display: each row's runs of one attribute become texts, each
//! input field a field named for its place in tab order ([`field_name`]),
//! so Enter hands the panel its fields in the order DST reads them.

use console::{Attr as Cell, COLS, Color, ROWS, Screen as Grid};
use uim::Screen;
use uim::model::{Shift, Usage};
use uim::render::{Attr, Field, Target, Text};

/// The name of the `i`th input field (from 0).
#[must_use]
pub fn field_name(i: usize) -> String {
    format!("F{i}")
}

fn attr_of(a: Cell) -> Attr {
    if a.reverse {
        Attr::Reverse
    } else if a.nondisplay {
        Attr::Nondisplay
    } else {
        match a.color {
            Color::Blue => Attr::Blue,
            Color::Green if !a.high => Attr::Normal,
            // White, red (errors) and the rest stand out.
            _ => Attr::HighIntensity,
        }
    }
}

/// The screen for a job's display.
#[must_use]
pub fn to_uim(g: &Grid) -> Screen {
    let mut s = Screen::new(ROWS, COLS);
    let in_field = |r: usize, c: usize| {
        g.fields
            .iter()
            .any(|f| f.row == r && (f.col..f.col + f.len).contains(&c))
    };
    for r in 1..=ROWS {
        let mut c = 1;
        while c <= COLS {
            let cell = g.cell(r - 1, c - 1);
            if in_field(r, c) || cell.ch == ' ' {
                c += 1;
                continue;
            }
            // A run: one attribute, up to a field.
            let a = attr_of(cell.attr);
            let start = c;
            let mut text = String::new();
            while c <= COLS && !in_field(r, c) {
                let cell = g.cell(r - 1, c - 1);
                if cell.ch != ' ' && attr_of(cell.attr) != a {
                    break;
                }
                text.push(cell.ch);
                c += 1;
            }
            let text = text.trim_end().to_string();
            if !text.is_empty() {
                s.texts.push(Text {
                    row: r,
                    col: start,
                    attr: a,
                    text,
                });
            }
        }
    }
    for (i, f) in g.fields.iter().enumerate() {
        s.field(Field {
            row: f.row,
            col: f.col,
            len: f.len,
            attr: if f.nondisplay {
                Attr::Nondisplay
            } else {
                Attr::Underline
            },
            value: if f.nondisplay {
                String::new()
            } else {
                g.field_text(f)
            },
            target: Target::Var(field_name(i)),
            usage: Usage::Inout,
            shift: Shift::None,
            required: false,
            autoskip: false,
        });
    }
    s.cursor = Some((g.cursor.0 + 1, g.cursor.1 + 1));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_fields_and_cursor() {
        let mut g = Grid::new();
        g.put(1, 30, "Title Here", Cell::WHITE);
        g.put(4, 2, "Label . . :", Cell::GREEN);
        g.field(4, 15, 10, false);
        g.put(4, 15, "VALUE", Cell::INPUT);
        g.field(5, 15, 10, true);
        g.cursor = (3, 14);
        let s = to_uim(&g);
        assert_eq!(s.texts.len(), 2);
        assert_eq!((s.texts[0].col, s.texts[0].attr), (30, Attr::HighIntensity));
        assert_eq!(s.texts[1].text, "Label . . :");
        assert_eq!(s.fields.len(), 2);
        assert_eq!(s.fields[0].value, "VALUE");
        assert_eq!(s.fields[1].attr, Attr::Nondisplay);
        assert_eq!(s.fields[1].target, Target::Var("F1".into()));
        assert_eq!(s.cursor, Some((4, 15)));
        let rows = s.text_rows();
        assert_eq!(&rows[3][1..12], "Label . . :");
        assert_eq!(&rows[3][14..19], "VALUE");
    }
}
