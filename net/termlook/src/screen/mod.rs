// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The screen the look draws, one model whatever fed it: the size, each
//! cell's character and how it shows, the cursor and its shape, the
//! status row, and the look (the scheme, and greyed when the screen is
//! off). Two ways in: a 5250 screen ([`s5250`]) and a partition
//! console's grid ([`grid`]).

pub mod grid;
pub mod s5250;

use crate::file::{pdf, png};
use crate::look::font;
use crate::look::render::{self, Canvas, Look, Oia, Paint};

/// How the cursor draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Shape {
    /// The cell's last scanline, as the framebuffer draws it.
    #[default]
    Underline,
    /// The cell reversed.
    Block,
    /// Not drawn.
    Hidden,
}

/// A screen to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    /// Rows, the status row not counted.
    pub rows: usize,
    /// Columns.
    pub cols: usize,
    /// `rows * cols` cells, row by row, the cursor not marked.
    pub cells: Vec<Paint>,
    /// How the cursor draws; where it is is the status row's.
    pub shape: Shape,
    /// The status row below the grid.
    pub status: Oia,
    /// The scheme, and whether the screen is off (greyed).
    pub look: Look,
}

impl Screen {
    /// The cells with the cursor marked.
    pub fn paints(&self) -> Vec<Paint> {
        let mut p = self.cells.clone();
        let (r, c) = self.status.cursor;
        if r >= 1 && c >= 1 && r <= self.rows && c <= self.cols {
            let cell = &mut p[(r - 1) * self.cols + c - 1];
            match self.shape {
                Shape::Underline => cell.cursor = true,
                Shape::Block => cell.selected = !cell.selected,
                Shape::Hidden => {}
            }
        }
        p
    }

    /// The picture at `scale`.
    pub fn picture(&self, scale: usize) -> Canvas {
        let status = render::oia_cells(&self.status, self.cols);
        render::render(
            self.rows,
            self.cols,
            scale,
            &self.paints(),
            &status,
            self.look,
        )
    }

    /// The picture as a PNG.
    pub fn png(&self, scale: usize) -> Vec<u8> {
        let c = self.picture(scale);
        png::encode(c.w, c.h, &c.px)
    }

    /// Print Screen: the picture at scale 1 as a one-page PDF.
    pub fn print_screen(&self, title: &str, producer: &str) -> Vec<u8> {
        pdf::print_screen(
            &self.picture(1),
            self.look.scheme.background(),
            title,
            producer,
        )
    }

    /// Width and height of the picture at `scale`.
    pub fn size(&self, scale: usize) -> (usize, usize) {
        size(self.rows, self.cols, scale)
    }

    /// The text of each row, a character a column: nondisplay cells as
    /// blanks.
    pub fn text_rows(&self) -> Vec<String> {
        self.cells
            .chunks(self.cols.max(1))
            .map(|row| {
                row.iter()
                    .map(|p| if p.style.nondisplay { ' ' } else { p.ch })
                    .collect()
            })
            .collect()
    }
}

/// Width and height of a picture of `rows` by `cols` and its status
/// row at `scale`.
pub fn size(rows: usize, cols: usize, scale: usize) -> (usize, usize) {
    let s = scale.max(1);
    (cols * font::GLYPH_W * s, (rows + 1) * font::GLYPH_H * s)
}
