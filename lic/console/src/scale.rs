// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The scaler: the cell grid drawn at 9×16 into an off-screen bitmap,
//! each glyph pixel blown up to an s×s block (nearest neighbour) and
//! packed into the framebuffer's own pixel format, the grid centered.
//! Only the cell rows that changed are scaled again; `draw` returns the
//! byte span of the frame to write. Pure logic, so the harness renders
//! its goldens with the same code.

use std::ops::Range;

use crate::font::{GLYPH_H, GLYPH_W, glyph};
use crate::palette;
use crate::screen::{Cell, Screen};

/// One color channel of a pixel: bit offset and length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Channel {
    /// Bit offset of the channel's least significant bit.
    pub offset: u32,
    /// Bits in the channel.
    pub length: u32,
}

/// How the framebuffer packs a pixel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelFormat {
    /// Bits per pixel: 16, 24 or 32.
    pub bpp: u32,
    /// Red.
    pub red: Channel,
    /// Green.
    pub green: Channel,
    /// Blue.
    pub blue: Channel,
}

impl PixelFormat {
    /// 32 bpp, `0x00RRGGBB`.
    pub const XRGB8888: Self = Self {
        bpp: 32,
        red: Channel {
            offset: 16,
            length: 8,
        },
        green: Channel {
            offset: 8,
            length: 8,
        },
        blue: Channel {
            offset: 0,
            length: 8,
        },
    };

    /// Bytes per pixel.
    #[must_use]
    pub fn bytes(&self) -> usize {
        (self.bpp / 8) as usize
    }

    /// Whether the scaler can pack it: 16, 24 or 32 bpp, channels of one
    /// to eight bits inside the pixel.
    #[must_use]
    pub fn supported(&self) -> bool {
        matches!(self.bpp, 16 | 24 | 32)
            && [self.red, self.green, self.blue]
                .iter()
                .all(|c| (1..=8).contains(&c.length) && c.offset + c.length <= self.bpp)
    }

    /// The pixel value of a `0x00RRGGBB` color.
    #[must_use]
    pub fn pack(&self, rgb: u32) -> u32 {
        let ch = |c: Channel, v: u32| ((v & 0xff) >> (8 - c.length)) << c.offset;
        ch(self.red, rgb >> 16) | ch(self.green, rgb >> 8) | ch(self.blue, rgb)
    }

    /// A color's bytes as the framebuffer stores them (little-endian).
    #[must_use]
    pub fn encode(&self, rgb: u32) -> [u8; 4] {
        self.pack(rgb).to_le_bytes()
    }

    /// The `0x00RRGGBB` color of the pixel at the start of `bytes`, with
    /// each channel widened back to eight bits.
    #[must_use]
    pub fn decode(&self, bytes: &[u8]) -> u32 {
        let mut b = [0u8; 4];
        b[..self.bytes()].copy_from_slice(&bytes[..self.bytes()]);
        let v = u32::from_le_bytes(b);
        let ch = |c: Channel| {
            let x = (v >> c.offset) & ((1 << c.length) - 1);
            (x << (8 - c.length)) | (x >> (2 * c.length).saturating_sub(8).min(c.length))
        };
        (ch(self.red) << 16) | (ch(self.green) << 8) | ch(self.blue)
    }
}

/// The framebuffer's mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    /// Visible width in pixels.
    pub xres: usize,
    /// Visible height in pixels.
    pub yres: usize,
    /// Bytes per scanline.
    pub stride: usize,
    /// Pixel format.
    pub format: PixelFormat,
}

impl Mode {
    /// A mode with the tightest stride for its width.
    #[must_use]
    pub fn packed(xres: usize, yres: usize, format: PixelFormat) -> Self {
        Self {
            xres,
            yres,
            stride: xres * format.bytes(),
            format,
        }
    }
}

/// How the scale is chosen: `qs400.console.scale=auto|N`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Scale {
    /// The largest integer scale that fits.
    #[default]
    Auto,
    /// This scale, as far as it fits.
    Fixed(usize),
}

impl Scale {
    /// The command-line value; anything else is `Auto`.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value.trim().parse::<usize>() {
            Ok(n) if n >= 1 => Self::Fixed(n),
            _ => Self::Auto,
        }
    }

    /// `qs400.console.scale=` from a kernel command line.
    #[must_use]
    pub fn from_cmdline(cmdline: &str) -> Self {
        cmdline
            .split_whitespace()
            .rev()
            .find_map(|w| w.strip_prefix("qs400.console.scale="))
            .map_or(Self::Auto, Self::parse)
    }
}

/// Where the grid sits on the display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Pixels per glyph pixel, each way.
    pub scale: usize,
    /// Left edge of the grid.
    pub x0: usize,
    /// Top edge of the grid.
    pub y0: usize,
    /// Grid rows.
    pub rows: usize,
    /// Grid columns.
    pub cols: usize,
}

impl Layout {
    /// `s = max(1, min(xres / (cols*9), yres / (rows*16)))`, a fixed
    /// scale clamped to it, the grid centered.
    #[must_use]
    pub fn compute(xres: usize, yres: usize, rows: usize, cols: usize, scale: Scale) -> Self {
        let fit = (xres / (cols * GLYPH_W)).min(yres / (rows * GLYPH_H)).max(1);
        let s = match scale {
            Scale::Auto => fit,
            Scale::Fixed(n) => n.clamp(1, fit),
        };
        Self {
            scale: s,
            x0: xres.saturating_sub(cols * GLYPH_W * s) / 2,
            y0: yres.saturating_sub(rows * GLYPH_H * s) / 2,
            rows,
            cols,
        }
    }

    /// Grid width in pixels, scaled.
    #[must_use]
    pub fn width(&self) -> usize {
        self.cols * GLYPH_W * self.scale
    }

    /// Grid height in pixels, scaled.
    #[must_use]
    pub fn height(&self) -> usize {
        self.rows * GLYPH_H * self.scale
    }
}

/// What a grid to draw offers: the console's screen, or a terminal's.
pub trait Cells {
    /// Rows.
    fn rows(&self) -> usize;
    /// Columns.
    fn cols(&self) -> usize;
    /// The cell at a 0-based position.
    fn cell(&self, row: usize, col: usize) -> Cell;
    /// The cursor, 0-based.
    fn cursor(&self) -> (usize, usize);
}

impl Cells for Screen {
    fn rows(&self) -> usize {
        crate::screen::ROWS
    }
    fn cols(&self) -> usize {
        crate::screen::COLS
    }
    fn cell(&self, row: usize, col: usize) -> Cell {
        Screen::cell(self, row, col)
    }
    fn cursor(&self) -> (usize, usize) {
        self.cursor
    }
}

/// The off-screen frame: the 9×16 cell-grid bitmap, the scaled frame in
/// the framebuffer's format, and what was drawn last.
pub struct Canvas {
    mode: Mode,
    layout: Layout,
    want: Scale,
    /// `0x00RRGGBB` pixels of the grid at 1:1.
    grid: Vec<u32>,
    /// The whole frame, `yres * stride` bytes.
    frame: Vec<u8>,
    /// Cells and cursor last drawn; `None` repaints everything.
    drawn: Option<(Vec<Cell>, (usize, usize))>,
}

impl Canvas {
    /// A frame for this mode and grid size, all background.
    #[must_use]
    pub fn new(mode: Mode, rows: usize, cols: usize, want: Scale) -> Self {
        let layout = Layout::compute(mode.xres, mode.yres, rows, cols, want);
        let mut c = Self {
            mode,
            layout,
            want,
            grid: Vec::new(),
            frame: Vec::new(),
            drawn: None,
        };
        c.reset();
        c
    }

    /// The mode drawn for.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The layout in use.
    #[must_use]
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// The frame bytes.
    #[must_use]
    pub fn frame(&self) -> &[u8] {
        &self.frame
    }

    /// The frame's `0x00RRGGBB` pixel at `(x, y)`.
    #[must_use]
    pub fn pixel(&self, x: usize, y: usize) -> u32 {
        let at = y * self.mode.stride + x * self.mode.format.bytes();
        self.mode.format.decode(&self.frame[at..])
    }

    /// A new mode or grid size (DSPMOD, a mode change): the layout is
    /// computed again and the next draw repaints the whole frame.
    pub fn resize(&mut self, mode: Mode, rows: usize, cols: usize) {
        if mode == self.mode && rows == self.layout.rows && cols == self.layout.cols {
            return;
        }
        self.mode = mode;
        self.layout = Layout::compute(mode.xres, mode.yres, rows, cols, self.want);
        self.reset();
    }

    /// Repaint everything on the next draw.
    pub fn invalidate(&mut self) {
        self.drawn = None;
    }

    fn reset(&mut self) {
        let l = self.layout;
        self.grid = vec![palette::BACKGROUND; l.cols * GLYPH_W * l.rows * GLYPH_H];
        let bg = self.mode.format.encode(palette::BACKGROUND);
        let n = self.mode.format.bytes();
        let mut line = vec![0u8; self.mode.stride];
        for px in line.chunks_exact_mut(n) {
            px.copy_from_slice(&bg[..n]);
        }
        self.frame = line.repeat(self.mode.yres);
        self.drawn = None;
    }

    /// Draw a grid. Returns the span of the frame that changed, whole
    /// scanlines from the first dirty row's top to the last one's
    /// bottom; the whole frame after a reset; `None` when nothing did.
    pub fn draw(&mut self, cells: &dyn Cells) -> Option<Range<usize>> {
        let l = self.layout;
        if cells.rows() != l.rows || cells.cols() != l.cols {
            let mode = self.mode;
            self.resize(mode, cells.rows(), cells.cols());
        }
        let l = self.layout;
        let now: Vec<Cell> = (0..l.rows)
            .flat_map(|r| (0..l.cols).map(move |c| (r, c)))
            .map(|(r, c)| cells.cell(r, c))
            .collect();
        let cursor = cells.cursor();
        let full = self.drawn.is_none();
        let dirty: Vec<usize> = match &self.drawn {
            None => (0..l.rows).collect(),
            Some((before, was)) => (0..l.rows)
                .filter(|&r| {
                    before[r * l.cols..(r + 1) * l.cols] != now[r * l.cols..(r + 1) * l.cols]
                        || (was.0 == r) != (cursor.0 == r)
                        || (cursor.0 == r && was.1 != cursor.1)
                })
                .collect(),
        };
        self.drawn = Some((now, cursor));
        if dirty.is_empty() {
            return None;
        }
        for &r in &dirty {
            self.render_row(cells, r);
            self.blit_row(r);
        }
        if full {
            return Some(0..self.frame.len());
        }
        let (first, last) = (dirty[0], dirty[dirty.len() - 1]);
        let span = GLYPH_H * l.scale;
        let top = (l.y0 + first * span).min(self.mode.yres);
        let bottom = (l.y0 + (last + 1) * span).min(self.mode.yres);
        Some(top * self.mode.stride..bottom * self.mode.stride)
    }

    /// Row `r` into the 1:1 bitmap.
    fn render_row(&mut self, cells: &dyn Cells, r: usize) {
        let w = self.layout.cols * GLYPH_W;
        let cursor = cells.cursor();
        for y in 0..GLYPH_H {
            let at = (r * GLYPH_H + y) * w;
            let line = &mut self.grid[at..at + w];
            cells_line(cells, r, y, cursor, line);
        }
    }

    /// Row `r` of the bitmap, scaled and packed into the frame.
    fn blit_row(&mut self, row: usize) {
        let layout = self.layout;
        let scale = layout.scale;
        let bytes = self.mode.format.bytes();
        let width = layout.cols * GLYPH_W;
        let visible = width.min(self.mode.xres.saturating_sub(layout.x0) / scale);
        let mut packed = vec![0u8; visible * scale * bytes];
        for y in 0..GLYPH_H {
            let src = &self.grid[(row * GLYPH_H + y) * width..][..visible];
            for (x, &rgb) in src.iter().enumerate() {
                let px = self.mode.format.encode(rgb);
                for k in 0..scale {
                    packed[(x * scale + k) * bytes..][..bytes].copy_from_slice(&px[..bytes]);
                }
            }
            for k in 0..scale {
                let fy = layout.y0 + (row * GLYPH_H + y) * scale + k;
                if fy >= self.mode.yres {
                    return;
                }
                let at = fy * self.mode.stride + layout.x0 * bytes;
                self.frame[at..at + packed.len()].copy_from_slice(&packed);
            }
        }
    }
}

/// Pixels of scanline `y` of grid row `r`, at 1:1.
fn cells_line(cells: &dyn Cells, r: usize, y: usize, cursor: (usize, usize), line: &mut [u32]) {
    for c in 0..cells.cols() {
        let cell = cells.cell(r, c);
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
        let Some(px) = line.get_mut(c * GLYPH_W..c * GLYPH_W + width) else {
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
        if cursor == (r, c) && y == GLYPH_H - 1 {
            px.fill(palette::WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{Attr, COLS, ROWS};

    fn ch(offset: u32, length: u32) -> Channel {
        Channel { offset, length }
    }

    const RGB565: PixelFormat = PixelFormat {
        bpp: 16,
        red: Channel {
            offset: 11,
            length: 5,
        },
        green: Channel {
            offset: 5,
            length: 6,
        },
        blue: Channel {
            offset: 0,
            length: 5,
        },
    };

    #[test]
    fn the_scale_is_the_largest_integer_that_fits() {
        let s = |x, y| Layout::compute(x, y, ROWS, COLS, Scale::Auto).scale;
        assert_eq!(s(640, 480), 1);
        assert_eq!(s(1024, 768), 1);
        assert_eq!(s(1280, 800), 1);
        assert_eq!(s(1440, 900), 2);
        assert_eq!(s(1920, 1080), 2);
        assert_eq!(s(1920, 1200), 2);
        assert_eq!(s(2560, 1440), 3);
        assert_eq!(s(3840, 2160), 5);
        // Smaller than the grid: still 1.
        assert_eq!(s(320, 200), 1);
        // 27×132 needs 1188 by 432.
        assert_eq!(Layout::compute(1920, 1080, 27, 132, Scale::Auto).scale, 1);
        assert_eq!(Layout::compute(2560, 1440, 27, 132, Scale::Auto).scale, 2);
    }

    #[test]
    fn a_fixed_scale_is_clamped_to_what_fits() {
        assert_eq!(Layout::compute(1920, 1080, 24, 80, Scale::Fixed(1)).scale, 1);
        assert_eq!(Layout::compute(1920, 1080, 24, 80, Scale::Fixed(9)).scale, 2);
        assert_eq!(Scale::parse("3"), Scale::Fixed(3));
        assert_eq!(Scale::parse("auto"), Scale::Auto);
        assert_eq!(Scale::parse("0"), Scale::Auto);
        let line = "console=ttyS0 qs400.console.scale=2 quiet";
        assert_eq!(Scale::from_cmdline(line), Scale::Fixed(2));
        assert_eq!(Scale::from_cmdline("quiet"), Scale::Auto);
    }

    #[test]
    fn the_grid_is_centered() {
        let l = Layout::compute(1920, 1080, 24, 80, Scale::Auto);
        assert_eq!((l.width(), l.height()), (1440, 768));
        assert_eq!((l.x0, l.y0), (240, 156));
        let l = Layout::compute(1024, 768, 24, 80, Scale::Auto);
        assert_eq!((l.x0, l.y0), (152, 192));
        let l = Layout::compute(700, 300, 24, 80, Scale::Auto);
        assert_eq!((l.x0, l.y0), (0, 0));
    }

    #[test]
    fn pixels_pack_for_each_depth() {
        let x = PixelFormat::XRGB8888;
        assert_eq!(x.pack(0x0012_3456), 0x0012_3456);
        assert_eq!(x.encode(0x0012_3456), [0x56, 0x34, 0x12, 0]);
        let xbgr = PixelFormat {
            red: ch(0, 8),
            blue: ch(16, 8),
            ..x
        };
        assert_eq!(xbgr.pack(0x0012_3456), 0x0056_3412);
        assert_eq!(RGB565.pack(0x00FF_FFFF), 0xFFFF);
        assert_eq!(RGB565.pack(0x00FF_0000), 0xF800);
        assert_eq!(RGB565.pack(0x0000_FF00), 0x07E0);
        assert_eq!(RGB565.encode(0x0000_00FF)[..2], [0x1F, 0x00]);
        let bgr24 = PixelFormat {
            bpp: 24,
            ..x
        };
        assert_eq!(bgr24.bytes(), 3);
        assert_eq!(bgr24.encode(0x0012_3456)[..3], [0x56, 0x34, 0x12]);
        for f in [x, xbgr, RGB565, bgr24] {
            assert!(f.supported());
            assert_eq!(f.decode(&f.encode(0)), 0);
        }
        assert_eq!(RGB565.decode(&RGB565.encode(0x00FF_FFFF)), 0x00FF_FFFF);
        assert_eq!(x.decode(&x.encode(palette::GREEN)), palette::GREEN);
        assert!(!PixelFormat { bpp: 8, ..x }.supported());
        assert!(!PixelFormat { red: ch(28, 8), ..x }.supported());
    }

    fn text(s: &mut Screen, row: usize, t: &str) {
        s.put(row, 1, t, Attr::GREEN);
    }

    #[test]
    fn only_dirty_rows_are_blitted() {
        let mode = Mode::packed(1920, 1080, PixelFormat::XRGB8888);
        let mut c = Canvas::new(mode, ROWS, COLS, Scale::Auto);
        let mut s = Screen::new();
        s.cursor = (0, 0);
        text(&mut s, 3, "Hello");
        assert_eq!(c.draw(&s), Some(0..1080 * 1920 * 4));
        assert_eq!(c.draw(&s), None);
        // One cell on row 10 (0-based 9): its 32 scanlines only.
        text(&mut s, 10, "x");
        let span = c.draw(&s).unwrap();
        let line = 1920 * 4;
        assert_eq!(span, (156 + 9 * 32) * line..(156 + 10 * 32) * line);
        // The cursor moving between rows dirties both.
        s.cursor = (5, 3);
        let span = c.draw(&s).unwrap();
        assert_eq!(span, 156 * line..(156 + 6 * 32) * line);
        // Along a row: that row alone.
        s.cursor = (5, 4);
        let span = c.draw(&s).unwrap();
        assert_eq!(span, (156 + 5 * 32) * line..(156 + 6 * 32) * line);
        // A resize repaints everything.
        c.resize(Mode::packed(1024, 768, PixelFormat::XRGB8888), ROWS, COLS);
        assert_eq!(c.layout().scale, 1);
        assert_eq!(c.draw(&s), Some(0..1024 * 768 * 4));
    }

    #[test]
    fn glyph_pixels_become_uniform_blocks_and_margins_stay_dark() {
        let mode = Mode {
            xres: 1920,
            yres: 1080,
            stride: 1920 * 2 + 64,
            format: RGB565,
        };
        let mut c = Canvas::new(mode, ROWS, COLS, Scale::Auto);
        let mut screen = Screen::new();
        screen.cursor = (23, 79);
        text(&mut screen, 1, "H");
        c.draw(&screen);
        let l = c.layout();
        for (y, bits) in glyph('H').iter().enumerate() {
            for x in 0..GLYPH_W {
                let lit = bits & (1 << (GLYPH_W - 1 - x)) != 0;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let px = c.pixel(l.x0 + x * 2 + dx, l.y0 + y * 2 + dy);
                    assert_eq!(px != 0, lit, "glyph pixel ({x}, {y})");
                }
            }
        }
        assert_eq!(c.pixel(l.x0 - 1, l.y0), 0);
        assert_eq!(c.pixel(0, 0), 0);
        assert_eq!(c.pixel(1919, 1079), 0);
        // The cursor's underline, bottom right of the grid.
        assert_ne!(c.pixel(l.x0 + l.width() - 1, l.y0 + l.height() - 1), 0);
    }
}
