// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Showing a debugger display and reading what the operator typed: the
//! named fields, the options on entries, the command line, the key and
//! where the cursor was.

use std::collections::BTreeMap;

use tn5250::ds::aid;
use tn5250::screen::Read;
use uim::Screen;
use uim::render::Target;

use crate::display::surface::{Surface, Waited};
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
#[derive(Clone, Debug)]
pub struct Answer {
    /// The key.
    pub key: Key,
    /// Named fields.
    pub vars: BTreeMap<String, String>,
    /// Options typed on entries, by entry index, trimmed.
    pub options: BTreeMap<usize, String>,
    /// The command line, trimmed.
    pub cmdline: String,
    /// The cursor's row and column.
    pub cursor: (usize, usize),
}

impl Answer {
    /// A named field, trimmed, upper case.
    pub fn up(&self, name: &str) -> String {
        self.vars
            .get(name)
            .map(|v| v.trim().to_ascii_uppercase())
            .unwrap_or_default()
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
    match show_or(surface, screen, &mut || false)? {
        Shown::Answer(a) => Some(a),
        Shown::Polled => None,
    }
}

/// What a display with a poll ended with.
pub enum Shown {
    /// The operator's answer.
    Answer(Answer),
    /// The poll asked to stop waiting.
    Polled,
}

/// Show a screen and wait for a key or for `poll` to say stop
/// (`Shown::Polled`); `None` when the display is gone.
pub fn show_or(
    surface: &mut Surface,
    screen: &Screen,
    poll: &mut dyn FnMut() -> bool,
) -> Option<Shown> {
    surface.with_model(|m| apply(screen, m));
    surface.flush(Read::MdtFields);
    let Waited::Key(a, (row, col)) = surface.wait_or(poll)? else {
        return Some(Shown::Polled);
    };
    let mut ans = Answer {
        key: key_of(a),
        vars: BTreeMap::new(),
        options: BTreeMap::new(),
        cmdline: String::new(),
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
                    let t = v.trim().to_ascii_uppercase();
                    if !t.is_empty() {
                        ans.options.insert(*row_index, t);
                    }
                }
                Target::CmdLine => ans.cmdline = v.trim().to_string(),
                _ => {}
            }
        }
    });
    Some(Shown::Answer(ans))
}
