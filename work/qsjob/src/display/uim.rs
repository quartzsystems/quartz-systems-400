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

pub use uim::engine::{CMDLINE_VAR, Choice, LineMsg, Response};

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

/// Put a rendered screen on the model, in the screen's own size.
pub fn apply(s: &Screen, m: &mut ScreenModel) {
    m.resize(s.cols > 80);
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

    fn print(&mut self, title: &str, rows: &[String]) -> Result<LineMsg, LineMsg> {
        crate::command::uiapi::print_rows(self.host(), title, rows)
    }

    fn message_text(&mut self, m: &LineMsg) -> Option<String> {
        Some(super::msgline::text_of(self.host(), m))
    }

    fn message_help(&mut self, m: &LineMsg) -> bool {
        super::msgline::help(self, m)
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

    /// A panel goes out in its own size: a 24x80 panel on a 27x132
    /// screen switches it to 24x80, a 27x132 screen back.
    #[test]
    fn screen_sets_the_mode() {
        let c = uim::compile(GROUP, &uim::NoImbeds, &uim::NoImports, &uim::CompileOpts::default());
        let g = Rc::new(c.group.unwrap_or_else(|| panic!("{}", c.listing)));
        let d = UimDialog::new(g, "QS400");
        let opts = uim::Options {
            wide: true,
            ..uim::Options::default()
        };
        let screen = uim::render(&d.group, "TEST", &d, &opts).unwrap();
        let mut m = ScreenModel::new(true);
        apply(&screen, &mut m);
        assert_eq!((m.rows(), m.cols()), (24, 80));
        let out = m.render(Read::MdtFields);
        assert_eq!(&out.data[..2], &[0x04, tn5250::ds::cmd::CLEAR_UNIT]);
        apply(&Screen::new(27, 132), &mut m);
        assert_eq!((m.rows(), m.cols()), (27, 132));
        let out = m.render(Read::MdtFields);
        assert_eq!(&out.data[..2], &[0x04, tn5250::ds::cmd::CLEAR_UNIT_ALT]);
    }

    /// MAIN with `cmdline` on its command line, through the 5250 stream
    /// into a terminal: its text and attribute grids.
    fn main_grids(cmdline: &str) -> (Vec<String>, Vec<String>) {
        let src = menus::shipped::source("MAIN").expect("MAIN");
        let compiled = uim::compile(src, &uim::NoImbeds, &uim::NoImports, &uim::CompileOpts::default());
        let group = Rc::new(compiled.group.unwrap_or_else(|| panic!("{}", compiled.listing)));
        let mut d = UimDialog::new(group, "QS400");
        d.set(CMDLINE_VAR, cmdline);
        let opts = uim::Options {
            system: "SYSNAME".into(),
            show_copyright: true,
            copyright: "(C) COPYRIGHT QUARTZ SYSTEMS 2026.".into(),
            ..uim::Options::default()
        };
        let screen = uim::render(&d.group, "MAIN", &d, &opts).unwrap();
        let mut m = ScreenModel::new(false);
        apply(&screen, &mut m);
        let out = m.render(Read::MdtFields);
        let mut t = tn5250::term::Terminal::new(tn5250::ds::wdsf::Capabilities {
            model: tn5250::ds::wdsf::Model::M3179,
            gui: false,
            color: true,
        });
        t.apply(&tn5250::record::Record::new(out.opcode, out.data))
            .unwrap();
        // The text grid of the UIM screen agrees with the terminal's.
        assert_eq!(screen.text_rows()[19..21], t.text_rows()[19..21]);
        (t.text_rows(), t.attr_rows())
    }

    /// Checks the grids against `audit/cmdline/NAME.{txt,attr}`;
    /// `QS400_UPDATE_GOLDEN=1` rewrites them.
    fn golden(name: &str, text: &[String], attrs: &[String]) {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/conformance/golden/audit/cmdline");
        for (ext, rows) in [("txt", text), ("attr", attrs)] {
            let path = dir.join(format!("{name}.{ext}"));
            let ours: String = rows.iter().flat_map(|r| [r.as_str(), "\n"]).collect();
            if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(&path, &ours).unwrap();
            }
            let theirs = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e} (QS400_UPDATE_GOLDEN=1 writes it)", path.display()))
                .replace('\r', "");
            assert_eq!(ours, theirs, "{}", path.display());
        }
    }

    /// pub400's MAIN attribute rows 19 to 21 (1-based 20 to 22).
    fn pub400_cmdline_rows() -> Vec<String> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/conformance/golden/verified/pub400/main-menu.attr");
        let text = std::fs::read_to_string(path).unwrap();
        text.lines()
            .skip(18)
            .take(3)
            .map(|l| l.trim_end_matches('\r').to_string())
            .collect()
    }

    #[test]
    fn main_command_line_is_one_field_over_two_rows() {
        let (text, attrs) = main_grids("");
        // Row 19 "===>", rows 20 and 21 the field (153), as on pub400.
        assert_eq!(attrs[18..21], pub400_cmdline_rows()[..]);
        golden("main-empty", &text, &attrs);
    }

    #[test]
    fn a_long_command_goes_on_at_row_21() {
        let cmd = "CRTLINETH LIND(ETHLINE) RSRCNAME(CMN04) LINESPEED(*AUTO) \
                   DUPLEX(*AUTO) ONLINE(*YES) TEXT('Primary Ethernet')";
        let (text, attrs) = main_grids(cmd);
        assert_eq!(attrs[18..21], pub400_cmdline_rows()[..]);
        assert_eq!(&text[19][6..], &cmd[..74]);
        assert_eq!(text[20].trim_end(), &cmd[74..]);
        golden("main-long", &text, &attrs);
    }
}
