// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a grid is drawn on: the framebuffer, a serial terminal.

use std::io;

use crate::scale::Cells;

/// A place the console's grid shows.
pub trait Display: Send {
    /// Draw the grid: what changed since the last draw goes out.
    fn draw(&mut self, cells: &dyn Cells) -> io::Result<()>;

    /// One line for a log: what the display is and how the grid sits
    /// on it.
    fn describe(&self) -> String;

    /// Ask the device for its mode again. True when it changed: the
    /// next draw repaints everything.
    fn check_mode(&mut self) -> bool {
        false
    }

    /// Draw the grid with what the console knows beside its cells, for
    /// a display that shows more than the cells; the others draw them.
    fn draw_view(&mut self, cells: &dyn Cells, view: &View) -> io::Result<()> {
        let _ = view;
        self.draw(cells)
    }

    /// Report a pointer from the next draw on, or stop: a display whose
    /// device has one (a terminal's mouse) asks it to; the others
    /// ignore this.
    fn set_pointer(&mut self, on: bool) {
        let _ = on;
    }
}

/// What the console knows beside its cells.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct View {
    /// The SRC display's code and text, when that is what shows.
    pub src: Option<(String, String)>,
    /// Input inhibited.
    pub inhibited: bool,
    /// Insert mode.
    pub insert: bool,
    /// The console is a 5250 session's terminal.
    pub session: bool,
}
