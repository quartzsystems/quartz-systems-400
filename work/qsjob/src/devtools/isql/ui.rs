// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Showing a STRSQL display and reading it back: the named fields, the
//! sequence numbers typed on a list, the key and the cursor.

use std::collections::BTreeMap;

use tn5250::ds::aid;
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
    /// Print and the others.
    Other,
}

/// What came back from a display.
#[derive(Clone, Debug)]
pub struct Answer {
    /// The key.
    pub key: Key,
    /// Named fields, as typed (not trimmed).
    pub vars: BTreeMap<String, String>,
    /// Sequence numbers typed on list entries, by entry index.
    pub options: BTreeMap<usize, String>,
    /// The cursor, row and column from 1.
    pub cursor: (usize, usize),
}

impl Answer {
    /// A named field, trimmed.
    pub fn get(&self, name: &str) -> String {
        self.vars
            .get(name)
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    }

    /// A named field, trimmed, upper case.
    pub fn up(&self, name: &str) -> String {
        self.get(name).to_ascii_uppercase()
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
    let (a, (row, col)) = surface.wait()?;
    let mut ans = Answer {
        key: key_of(a),
        vars: BTreeMap::new(),
        options: BTreeMap::new(),
        cursor: (usize::from(row), usize::from(col)),
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
                    let t = v.trim().to_string();
                    if !t.is_empty() {
                        ans.options.insert(*row_index, t);
                    }
                }
                _ => {}
            }
        }
    });
    Some(ans)
}
