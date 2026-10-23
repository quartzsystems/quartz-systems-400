// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The UIM dialog engine (`uim::engine`) on the job's screen: the
//! engine renders and acts on keys; this module puts its screens on
//! the 5250 model, reads the keys and fields back, opens other panel
//! groups and search indexes through the storage service and prints
//! panels and help to `QSYSPRT`.

use std::rc::Rc;

use tn5250::ds::field::{Ffw, Shift as FfwShift};
use tn5250::ds::{aid, attr};
use tn5250::screen::{Read, ScreenModel};
use uim::engine::{IndexEntry, Input, Terminal};
use uim::model::Key;
use uim::render::{Attr, Field, Screen};

use super::surface::Surface;

pub use uim::engine::{CMDLINE_VAR, Choice, Response};

/// The dialog of one panel group.
pub type UimDialog = uim::engine::Engine;

/// Why a panel could not be shown.
pub type UimError = uim::engine::EngineError;

/// The 5250 attribute of a rendered attribute.
fn attr_of(a: Attr) -> u8 {
    match a {
        Attr::Normal => attr::GREEN,
        Attr::HighIntensity => attr::WHITE,
        Attr::Blue => attr::BLUE,
        Attr::Reverse => attr::GREEN_RI,
        Attr::Underline => attr::GREEN_UL,
        Attr::Nondisplay => attr::NONDISPLAY,
    }
}

/// The field format word of a rendered field.
fn ffw_of(f: &Field) -> Ffw {
    Ffw {
        bypass: !f.usage.is_input(),
        shift: match f.shift {
            uim::model::Shift::Numeric => FfwShift::SignedNumeric,
            _ => FfwShift::AlphaShift,
        },
        mandatory_entry: f.required,
        auto_enter: f.autoskip,
        monocase: false,
        ..Ffw::default()
    }
}

/// Put a rendered screen on the model.
pub fn apply(s: &Screen, m: &mut ScreenModel) {
    m.clear();
    for t in &s.texts {
        m.put(t.row, t.col, &t.text, attr_of(t.attr));
    }
    for f in &s.fields {
        let a = match f.attr {
            Attr::Normal | Attr::Underline => attr::GREEN_UL,
            // An option that is not valid: underlined and reversed.
            Attr::Reverse => attr::GREEN_UL_RI,
            other => attr_of(other),
        };
        m.field(f.row, f.col, f.len, a, &f.value, ffw_of(f), &[]);
    }
    if let Some((r, c)) = s.cursor {
        m.set_cursor(r, c);
    }
}

/// The UIM key of an attention identifier.
fn key_of(a: u8) -> Option<Key> {
    match a {
        aid::ENTER | aid::AUTO_ENTER => Some(Key::Enter),
        aid::ROLL_UP => Some(Key::PageDown),
        aid::ROLL_DOWN => Some(Key::PageUp),
        aid::HELP => Some(Key::Help),
        aid::PRINT => Some(Key::Print),
        other => aid::function_of(other).map(Key::F),
    }
}

impl Terminal for Surface {
    fn wide(&mut self) -> bool {
        Surface::wide(self)
    }

    fn exchange(&mut self, screen: &Screen) -> Option<Input> {
        self.with_model(|m| apply(screen, m));
        self.flush(Read::MdtFields);
        let (a, cursor) = self.wait()?;
        let values = self.with_model(|m| {
            screen
                .fields
                .iter()
                .map(|f| {
                    m.field_index_at(f.row, f.col)
                        .map(|i| m.fields()[i].text())
                        .unwrap_or_default()
                })
                .collect()
        });
        Some(Input {
            key: key_of(a),
            cursor: (usize::from(cursor.0), usize::from(cursor.1)),
            values,
        })
    }

    fn group(&mut self, lib: &str, name: &str) -> Option<Rc<uim::PanelGroup>> {
        crate::panels::group(self.host(), lib, name).ok()
    }

    fn search_index(&mut self, lib: &str, name: &str) -> Option<Vec<IndexEntry>> {
        crate::command::uiapi::search_index(self.host(), lib, name)
    }

    fn print(&mut self, title: &str, rows: &[String]) -> Result<String, String> {
        crate::command::uiapi::print_rows(self.host(), title, rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GROUP: &str = "\
:PNLGRP.
:CLASS NAME=OPTCL BASETYPE='CHAR 3'.
:ECLASS.
:VAR NAME=OPTION CLASS=OPTCL.
:KEYL NAME=KEYS.
:KEYI KEY=ENTER ACTION=ENTER.
:KEYI KEY=F3 ACTION=EXIT.F3=Exit
:EKEYL.
:PANEL NAME=TEST KEYL=KEYS ENTER='RETURN 1' TOPSEP=SYSNAM.A Test Menu
:MENU DEPTH='*' SCROLL=YES OPTIONS=OPTION.Select one of the following:
:MENUI OPTION=1 ACTION='MENU USER'.User tasks
:EMENU.
:CMDLINE SIZE=LONG.Selection or command
:EPANEL.
:EPNLGRP.
";

    #[test]
    fn screen_onto_the_model() {
        let c = uim::compile(
            GROUP,
            &uim::NoImbeds,
            &uim::NoImports,
            &uim::CompileOpts::default(),
        );
        let g = Rc::new(c.group.unwrap_or_else(|| panic!("{}", c.listing)));
        let d = UimDialog::new(g, "QS400");
        let screen = uim::render(&d.group, "TEST", &d, &uim::Options::default()).unwrap();
        let mut m = ScreenModel::new(false);
        apply(&screen, &mut m);
        let rows = m.text_rows();
        assert!(rows[0].contains("A Test Menu"));
        assert!(rows[1].contains("System:"));
        assert!(rows[4].contains("1. User tasks"));
        assert_eq!(m.fields().len(), 1);
        assert_eq!(key_of(aid::ROLL_UP), Some(Key::PageDown));
    }
}
