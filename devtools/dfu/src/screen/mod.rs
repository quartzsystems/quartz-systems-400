// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DFU's and RLU's displays, laid out as IBM's display files place
//! them (each position measured on the pub400 goldens): the menus
//! ([`menu`]), the entry panels ([`prompt`]), the program definition
//! ([`define`]), working with data ([`work`]) and the RLU editor
//! ([`rlu`]). Each builds a `uim::Screen`; input fields name what they
//! hold with their `Target`. The drawing helpers are PDM's.

pub mod define;
pub mod menu;
pub mod prompt;
pub mod rlu;
pub mod work;

pub use pdm::screen::{blank, input, keys, leader, put, var};
pub use uim::Screen;
use uim::render::Attr;

/// The copyright line's column on the first display of a session.
pub const COPYRIGHT_COL: usize = 43;
/// The copyright line.
pub const COPYRIGHT: &str = "(C) COPYRIGHT QUARTZ SYSTEMS 2026.";

/// The message line, else the copyright when asked for.
pub fn bottom_line(s: &mut Screen, message: Option<&str>, copyright: bool) {
    match message {
        Some(m) => put(s, 24, 2, Attr::HighIntensity, m),
        None if copyright => put(s, 24, COPYRIGHT_COL, Attr::HighIntensity, COPYRIGHT),
        None => {}
    }
}

/// An output value.
pub fn out(s: &mut Screen, row: usize, col: usize, text: &str) {
    put(s, row, col, Attr::Normal, text);
}
