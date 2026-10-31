// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Cells to pixels on the framebuffer, through the scaler: one write per
//! frame spanning the dirty scanlines, then `fsync`.

use std::io;

use crate::fb::Framebuffer;
use crate::scale::{Canvas, Cells, Scale};

/// Draws a grid onto a framebuffer.
pub struct Renderer {
    fb: Framebuffer,
    canvas: Canvas,
}

impl Renderer {
    /// Lay the grid out for the framebuffer's mode; clear the screen.
    pub fn new(fb: Framebuffer, rows: usize, cols: usize, scale: Scale) -> io::Result<Self> {
        let canvas = Canvas::new(fb.mode, rows, cols, scale);
        let r = Self { fb, canvas };
        r.fb.write_span(r.canvas.frame(), 0)?;
        Ok(r)
    }

    /// One line for the IPL log and `svc.hw`.
    pub fn describe(&self) -> String {
        let m = self.canvas.mode();
        let l = self.canvas.layout();
        let f = m.format;
        format!(
            "{}x{} at {} bpp (r{}/{} g{}/{} b{}/{}), stride {}, {}x{} grid at scale {} from ({}, {}), fbcon {}",
            m.xres,
            m.yres,
            f.bpp,
            f.red.offset,
            f.red.length,
            f.green.offset,
            f.green.length,
            f.blue.offset,
            f.blue.length,
            m.stride,
            l.cols,
            l.rows,
            l.scale,
            l.x0,
            l.y0,
            if self.fb.fbcon_unbound {
                "unbound"
            } else {
                "still bound"
            }
        )
    }

    /// Draw a grid: the rows that changed go out as one write. A failed
    /// write asks for the mode again and repaints in the new one.
    pub fn draw(&mut self, cells: &dyn Cells) -> io::Result<()> {
        let Some(span) = self.canvas.draw(cells) else {
            return Ok(());
        };
        let start = span.start;
        match self.fb.write_span(&self.canvas.frame()[span], start) {
            Ok(()) => Ok(()),
            Err(e) => {
                self.check_mode();
                self.canvas.invalidate();
                match self.canvas.draw(cells) {
                    Some(span) => {
                        let start = span.start;
                        self.fb.write_span(&self.canvas.frame()[span], start)
                    }
                    None => Err(e),
                }
            }
        }
    }

    /// Ask the framebuffer for its mode; on a change, lay the grid out
    /// again so the next draw repaints. True when it changed.
    pub fn check_mode(&mut self) -> bool {
        match self.fb.requery() {
            Ok(true) => {
                let l = self.canvas.layout();
                self.canvas.resize(self.fb.mode, l.rows, l.cols);
                true
            }
            _ => false,
        }
    }
}
