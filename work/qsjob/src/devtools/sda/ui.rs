// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Showing an SDA display and reading what the operator typed; the
//! design image as one input field over the whole screen.

use std::collections::BTreeMap;

use tn5250::ds::field::Ffw;
use tn5250::ds::{aid, attr};
use tn5250::screen::Read;
use uim::Screen;
use uim::render::Target;

use crate::display::surface::Surface;
use crate::display::uim::apply;

/// The key that ended a read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Enter.
    Enter,
    /// A function key.
    F(u8),
    /// Page Down (Roll Up).
    PageDown,
    /// Page Up (Roll Down).
    PageUp,
    /// Help.
    Help,
    /// Print and the others: not acted on.
    Other,
}

/// What came back from a display.
#[derive(Clone, Debug, Default)]
pub struct Answer {
    /// The key.
    pub key: Option<Key>,
    /// Named fields, as typed.
    pub vars: BTreeMap<String, String>,
    /// Options typed on list entries, by entry index, trimmed.
    pub options: BTreeMap<usize, String>,
    /// Other list cells: (entry index, variable) to text.
    pub cells: BTreeMap<(usize, String), String>,
    /// The command line, trimmed.
    pub cmdline: String,
}

impl Answer {
    /// A named field, trimmed, upper case.
    pub fn up(&self, name: &str) -> String {
        self.vars
            .get(name)
            .map(|v| v.trim().to_ascii_uppercase())
            .unwrap_or_default()
    }

    /// A named field trimmed on the right, as typed.
    pub fn raw(&self, name: &str) -> String {
        self.vars
            .get(name)
            .map(|v| v.trim_end().to_string())
            .unwrap_or_default()
    }

    /// The key, `Other` when the display was gone.
    pub fn key(&self) -> Key {
        self.key.unwrap_or(Key::Other)
    }
}

fn key_of(a: u8) -> Key {
    match a {
        aid::ENTER | aid::AUTO_ENTER => Key::Enter,
        aid::ROLL_UP => Key::PageDown,
        aid::ROLL_DOWN => Key::PageUp,
        aid::HELP => Key::Help,
        other => aid::function_of(other).map_or(Key::Other, Key::F),
    }
}

/// Show a screen and wait for a key; `None` when the display is gone.
pub fn show(surface: &mut Surface, screen: &Screen) -> Option<Answer> {
    surface.with_model(|m| apply(screen, m));
    surface.flush(Read::MdtFields);
    let (a, _) = surface.wait()?;
    let mut ans = Answer {
        key: Some(key_of(a)),
        ..Answer::default()
    };
    surface.with_model(|m| {
        for f in &screen.fields {
            let v = m
                .field_index_at(f.row, f.col)
                .map(|i| m.fields()[i].text())
                .unwrap_or_default();
            match &f.target {
                Target::Var(n) | Target::PositionTo(n) => {
                    ans.vars.insert(n.clone(), v);
                }
                Target::ListOption { row_index, .. } => {
                    let t = v.trim().to_ascii_uppercase();
                    if !t.is_empty() {
                        ans.options.insert(*row_index, t);
                    }
                }
                Target::ListCell { row_index, var, .. } => {
                    ans.cells.insert((*row_index, var.clone()), v);
                }
                Target::CmdLine => ans.cmdline = v.trim().to_string(),
                Target::MenuSelection => {}
            }
        }
    });
    Some(ans)
}

/// Positions of the image field: from row 1 column 2 to the end.
const IMAGE_LEN: usize = 24 * 80 - 1;

/// Show the design image (24 rows of 80, row 24 holding the message
/// when there is one) as one input field and read it back as rows.
/// `None` when the display is gone.
pub fn show_image(surface: &mut Surface, rows: &[String], cursor: (usize, usize)) -> Option<(Key, Vec<String>, (usize, usize))> {
    let flat: String = rows
        .iter()
        .map(|r| format!("{r:80}").chars().take(80).collect::<String>())
        .collect::<String>()
        .chars()
        .skip(1)
        .collect();
    surface.with_model(|m| {
        m.clear();
        m.field(1, 2, IMAGE_LEN, attr::GREEN, &flat, Ffw::default(), &[]);
        m.set_cursor(cursor.0.max(1), cursor.1.max(1));
    });
    surface.flush(Read::MdtFields);
    let (a, (row, col)) = surface.wait()?;
    let text = surface.with_model(|m| m.fields().first().map(tn5250::screen::Field::text).unwrap_or_default());
    let all: Vec<char> = std::iter::once(' ')
        .chain(text.chars())
        .chain(std::iter::repeat(' '))
        .take(24 * 80)
        .collect();
    let out = all.chunks(80).map(|c| c.iter().collect()).collect();
    Some((key_of(a), out, (usize::from(row), usize::from(col))))
}
