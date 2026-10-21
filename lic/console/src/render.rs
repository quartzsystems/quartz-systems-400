// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Cells to pixels. The grid is centered on the framebuffer at 1:1.

use std::io;

use crate::fb::Framebuffer;
use crate::font::{GLYPH_H, GLYPH_W, glyph};
use crate::palette;
use crate::screen::{COLS, ROWS, Screen};

/// Draws a screen onto a framebuffer.
pub struct Renderer {
    fb: Framebuffer,
    x0: usize,
    y0: usize,
}

impl Renderer {
    /// Center the grid; clear the screen to the background.
    pub fn new(fb: Framebuffer) -> io::Result<Self> {
        let grid_w = COLS * GLYPH_W;
        let grid_h = ROWS * GLYPH_H;
        let x0 = fb.width.saturating_sub(grid_w) / 2;
        let y0 = fb.height.saturating_sub(grid_h) / 2;
        fb.fill(palette::BACKGROUND)?;
        Ok(Self { fb, x0, y0 })
    }

    /// One line for the log.
    pub fn describe(&self) -> String {
        format!(
            "{}x{} at 32 bpp, grid at ({}, {}), fbcon {}",
            self.fb.width,
            self.fb.height,
            self.x0,
            self.y0,
            if self.fb.fbcon_unbound {
                "unbound"
            } else {
                "still bound"
            }
        )
    }

    /// Draw the whole framebuffer: the grid in the middle, background in
    /// the margins, as one frame write.
    pub fn draw(&self, screen: &Screen) -> io::Result<()> {
        let (w, h) = (self.fb.width, self.fb.height);
        let mut frame = vec![palette::BACKGROUND; w * h];
        for r in 0..ROWS {
            for y in 0..GLYPH_H {
                let fy = self.y0 + r * GLYPH_H + y;
                if fy >= h {
                    break;
                }
                let line = &mut frame[fy * w..(fy + 1) * w];
                Self::cells_row(screen, r, y, &mut line[self.x0..]);
            }
        }
        self.fb.write_frame(&frame)
    }

    /// Pixels of scanline `y` of grid row `r` into `row_px`.
    fn cells_row(screen: &Screen, r: usize, y: usize, row_px: &mut [u32]) {
        for c in 0..COLS {
            let cell = screen.cell(r, c);
            if cell.attr.continuation {
                continue;
            }
            let fg = palette::rgb(cell.attr.color, cell.attr.high);
            let (fg, bg) = if cell.attr.reverse {
                (palette::BACKGROUND, fg)
            } else {
                (fg, palette::BACKGROUND)
            };
            let width = if cell.attr.double_width {
                2 * GLYPH_W
            } else {
                GLYPH_W
            };
            let Some(px) = row_px.get_mut(c * GLYPH_W..c * GLYPH_W + width) else {
                continue;
            };
            px.fill(bg);
            let bits = if cell.attr.nondisplay {
                0
            } else {
                glyph(cell.ch)[y]
            };
            for (x, p) in px.iter_mut().enumerate() {
                let col = if cell.attr.double_width { x / 2 } else { x };
                if bits & (1 << (GLYPH_W - 1 - col)) != 0 {
                    *p = fg;
                }
            }
            if cell.attr.underline && y == GLYPH_H - 2 {
                px.fill(fg);
            }
            if cell.attr.column_sep && y.is_multiple_of(4) {
                px[0] = fg;
                px[width - 1] = fg;
            }
            let (cr, cc) = screen.cursor;
            if cr == r && cc == c && y == GLYPH_H - 1 {
                px.fill(palette::WHITE);
            }
        }
    }
}
