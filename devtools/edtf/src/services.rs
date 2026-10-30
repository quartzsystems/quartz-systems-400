// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The F15 services display: what the session knows about the file and
//! what may be changed for the save (the CCSID the file is tagged
//! with, the line end written) and for the display (how far F19 and F20
//! and the page keys move). Browsing changes only the movements.

use tn5250::ds::attr;
use tn5250::ds::field::Ffw;
use tn5250::screen::ScreenModel;

use crate::msg::Msg;
use crate::session::{Mode, Session};
use crate::text::Eol;

const CCSID: (usize, usize) = (11, 37);
const EOL: (usize, usize) = (12, 37);
const SHIFT: (usize, usize) = (13, 37);
const SCROLL: (usize, usize) = (14, 37);

/// What the services display read back.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Choices {
    /// The CCSID typed.
    pub ccsid: String,
    /// The line end typed.
    pub eol: String,
    /// Columns to shift.
    pub shift: String,
    /// Records to scroll.
    pub scroll: String,
}

fn label(m: &mut ScreenModel, row: usize, text: &str) {
    let dots = if text.len() < 33 {
        let mut t = format!("{text} ");
        while t.len() < 33 {
            t.push_str(if t.len() % 2 == 0 { " " } else { "." });
        }
        t
    } else {
        text.to_string()
    };
    m.put(row, 4, &dots, attr::GREEN);
}

/// Draw the services display for `s`.
pub fn draw(s: &Session, m: &mut ScreenModel, message: Option<&Msg>) {
    m.clear();
    let title = match s.mode {
        Mode::Edit => "Edit Services",
        Mode::Browse => "Browse Services",
    };
    m.put(1, 1 + (80 - title.len()) / 2, title, attr::WHITE);
    m.put(3, 2, "Type changes, press Enter.", attr::GREEN);
    label(m, 5, "File");
    m.put(5, 37, &s.path, attr::WHITE);
    label(m, 6, "File CCSID");
    m.put(6, 37, &s.file_ccsid.to_string(), attr::WHITE);
    label(m, 7, "Job CCSID");
    m.put(7, 37, &s.job_ccsid.to_string(), attr::WHITE);
    label(m, 8, "Record length");
    m.put(8, 37, &s.rcdlen.to_string(), attr::WHITE);
    label(m, 9, "Records");
    m.put(9, 37, &s.buf.len().to_string(), attr::WHITE);
    let edit = s.mode == Mode::Edit;
    let ul = |m: &mut ScreenModel, at: (usize, usize), len: usize, v: &str, open: bool| {
        let a = if open { attr::GREEN_UL } else { attr::WHITE };
        let ffw = Ffw {
            bypass: !open,
            monocase: true,
            ..Ffw::default()
        };
        m.field(at.0, at.1, len, a, v, ffw, &[]);
    };
    label(m, CCSID.0, "Save with CCSID");
    ul(m, CCSID, 5, &s.new_ccsid.unwrap_or(s.file_ccsid).to_string(), edit);
    m.put(CCSID.0, 51, "1-65533", attr::GREEN);
    label(m, EOL.0, "Line end written");
    ul(m, EOL, 5, s.eol.name(), edit);
    m.put(EOL.0, 51, "*CRLF, *LF, *CR, *LFCR, *NL", attr::GREEN);
    label(m, SHIFT.0, "Columns F19 and F20 move");
    ul(m, SHIFT, 5, &s.col_by.to_string(), true);
    m.put(SHIFT.0, 51, "1-32766", attr::GREEN);
    label(m, SCROLL.0, "Records the page keys move");
    ul(m, SCROLL, 5, &s.rec_by.to_string(), true);
    m.put(SCROLL.0, 51, "1-9999999", attr::GREEN);
    m.put(23, 2, "F3=Exit   F12=Cancel", attr::BLUE);
    if let Some(msg) = message {
        m.error_line(&msg.text, attr::WHITE);
    }
    m.set_cursor(if edit { CCSID.0 } else { SHIFT.0 }, 37);
}

fn value(m: &ScreenModel, at: (usize, usize)) -> String {
    m.field_index_at(at.0, at.1)
        .and_then(|i| m.fields().get(i))
        .map(tn5250::screen::Field::text)
        .unwrap_or_default()
}

/// Read the display back.
#[must_use]
pub fn read(m: &ScreenModel) -> Choices {
    Choices {
        ccsid: value(m, CCSID),
        eol: value(m, EOL),
        shift: value(m, SHIFT),
        scroll: value(m, SCROLL),
    }
}

/// Apply the choices to the session; a message when one is not valid.
pub fn apply(s: &mut Session, c: &Choices) -> Result<(), Msg> {
    let num = |t: &str| t.trim().parse::<usize>().ok();
    let ccsid = if s.mode == Mode::Edit {
        match c.ccsid.trim() {
            "" => None,
            t => match t.parse::<u16>() {
                Ok(n) if (1..=65533).contains(&n) => Some(n),
                _ => return Err(Msg::new("CPDB607", &[t])),
            },
        }
    } else {
        None
    };
    let eol = if s.mode == Mode::Edit && !c.eol.trim().is_empty() {
        Some(Eol::parse(&c.eol).ok_or_else(|| Msg::new("CPDB624", &[]))?)
    } else {
        None
    };
    s.services(ccsid, eol, num(&c.shift), num(&c.scroll));
    match &s.message {
        Some(m) if m.id == "CPDB607" => Err(m.clone()),
        _ => Ok(()),
    }
}
