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
}
