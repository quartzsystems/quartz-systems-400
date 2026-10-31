// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The scaled console checked pixel by pixel: the guest boots on a
//! virtio VGA display of a given size, and its QMP screendump is
//! compared with the console scaler's own rendering of the `screen`
//! text, the golden. Colors are not in the text, so the comparison is
//! of lit and dark: per glyph pixel, an s×s block of one color, lit
//! where the golden is (or the inverse, for reverse image), the
//! underline and cursor scanlines allowed to be lit besides; and every
//! pixel outside the grid dark.

use std::fs;
use std::path::Path;
use std::time::Duration;

use console::font::{GLYPH_H, GLYPH_W};
use console::scale::{Canvas, Layout, Mode, PixelFormat, Scale};
use console::{COLS, ROWS, Screen};

use crate::qemu::Launch;
use crate::qemu::session::Session;
use crate::steps::{CMD, StepReport, Suite};

/// The display sizes the harness checks.
pub const SIZES: [(u32, u32); 2] = [(1920, 1080), (1024, 768)];

/// An RGB image.
pub struct Image {
    /// Width.
    pub w: usize,
    /// Height.
    pub h: usize,
    /// `w * h * 3` bytes, row-major.
    pub rgb: Vec<u8>,
}

impl Image {
    fn lit(&self, x: usize, y: usize) -> bool {
        let i = (y * self.w + x) * 3;
        self.rgb[i..i + 3] != [0, 0, 0]
    }

    fn at(&self, x: usize, y: usize) -> [u8; 3] {
        let i = (y * self.w + x) * 3;
        [self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]]
    }
}

/// The golden: the scaler's frame for the text, as RGB.
pub fn golden(text: &str, w: usize, h: usize) -> (Image, Layout) {
    let mut screen = Screen::new();
    for (r, line) in text.lines().take(ROWS).enumerate() {
        for (c, ch) in line.chars().take(COLS).enumerate() {
            screen.cell_mut(r, c).ch = ch;
        }
    }
    // No cursor in the text: put it where no glyph can be.
    screen.cursor = (ROWS, COLS);
    let mut canvas = Canvas::new(Mode::packed(w, h, PixelFormat::XRGB8888), ROWS, COLS, Scale::Auto);
    canvas.draw(&screen);
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        for x in 0..w {
            let [_, red, green, blue] = canvas.pixel(x, y).to_be_bytes();
            rgb.extend_from_slice(&[red, green, blue]);
        }
    }
    (Image { w, h, rgb }, canvas.layout())
}

/// How the capture compared.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Compared {
    /// Cells equal to the golden.
    pub exact: usize,
    /// Cells equal but for the underline or cursor scanline.
    pub underlined: usize,
    /// Cells the inverse of the golden (reverse image).
    pub reversed: usize,
    /// Cells that matched none of those, with their positions.
    pub wrong: Vec<(usize, usize)>,
    /// Glyph-pixel blocks not of one color.
    pub ragged: usize,
    /// Lit pixels outside the grid.
    pub margin: usize,
}

impl Compared {
    /// Whether the capture is the golden's.
    pub fn ok(&self) -> bool {
        self.wrong.is_empty() && self.ragged == 0 && self.margin == 0
    }
}

/// Compare a capture with the golden at the same size.
pub fn compare(capture: &Image, golden: &Image, l: &Layout) -> Compared {
    let mut out = Compared::default();
    let s = l.scale;
    for y in 0..capture.h {
        for x in 0..capture.w {
            let inside =
                (l.x0..l.x0 + l.width()).contains(&x) && (l.y0..l.y0 + l.height()).contains(&y);
            if !inside && capture.lit(x, y) {
                out.margin += 1;
            }
        }
    }
    for r in 0..l.rows {
        for c in 0..l.cols {
            // Lit per glyph pixel, captured and golden.
            let mut cap = [[false; GLYPH_W]; GLYPH_H];
            let mut want = [[false; GLYPH_W]; GLYPH_H];
            for gy in 0..GLYPH_H {
                for gx in 0..GLYPH_W {
                    let x = l.x0 + (c * GLYPH_W + gx) * s;
                    let y = l.y0 + (r * GLYPH_H + gy) * s;
                    let first = capture.at(x, y);
                    let uniform = (0..s).all(|dy| (0..s).all(|dx| capture.at(x + dx, y + dy) == first));
                    if !uniform {
                        out.ragged += 1;
                    }
                    cap[gy][gx] = capture.lit(x, y);
                    want[gy][gx] = golden.lit(x, y);
                }
            }
            if cap == want {
                out.exact += 1;
            } else if cap.iter().zip(&want).all(|(a, b)| a.iter().zip(b).all(|(p, q)| p != q)) {
                out.reversed += 1;
            } else if (0..GLYPH_H).all(|gy| {
                cap[gy] == want[gy] || (gy >= GLYPH_H - 2 && cap[gy].iter().all(|&p| p))
            }) {
                out.underlined += 1;
            } else {
                out.wrong.push((r + 1, c + 1));
            }
        }
    }
    out
}

/// The `P6` screendump as an image.
pub fn read_ppm(path: &Path) -> Result<Image, String> {
    let data = fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let (w, h, px) = crate::png::parse_ppm(&data)?;
    Ok(Image {
        w: w as usize,
        h: h as usize,
        rgb: px.to_vec(),
    })
}

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Steps 71 and 72: a guest at each size reaches sign-on; its display
    /// must be the scaler's rendering of the screen text.
    pub(crate) fn display_steps(&self) -> Result<Vec<StepReport>, crate::HarnessError> {
        type Check = fn(&Suite, &mut Session) -> Result<String, String>;
        let steps: [(&'static str, Check); 2] = [
            ("71 display 1920x1080", Self::scaled_large),
            ("72 display 1024x768", Self::scaled_small),
        ];
        let mut reports = Vec::new();
        for ((name, check), size) in steps.into_iter().zip(SIZES) {
            if let Some(only) = &self.only
                && !only.iter().any(|p| name.starts_with(p.as_str()))
            {
                continue;
            }
            let launch = Launch {
                display: Some(size),
                ..self.launch.clone()
            };
            let mut s = Session::spawn(&launch)?;
            reports.push(self.step(&mut s, name, check)?);
            s.command("pwrdwn", CMD).ok();
            s.wait_exit(Duration::from_secs(20));
        }
        Ok(reports)
    }

    fn scaled_large(&self, s: &mut Session) -> Result<String, String> {
        self.scaled(s, SIZES[0])
    }

    fn scaled_small(&self, s: &mut Session) -> Result<String, String> {
        self.scaled(s, SIZES[1])
    }

    /// One size: the display line of `svc.hw`, then the screendump
    /// against the golden, both written beside the suite's output.
    fn scaled(&self, s: &mut Session, (w, h): (u32, u32)) -> Result<String, String> {
        s.expect_in_order(crate::steps::SRC_SEQUENCE, self.boot_timeout)?;
        s.expect(crate::PROMPT, CMD)?;
        let text = crate::phase5::screen_wait(s, "Sign On")?;
        let hw = s.command("hw", CMD)?;
        let line = hw
            .lines()
            .find(|l| l.starts_with("Display: "))
            .ok_or_else(|| format!("svc.hw has no display line:\n{hw}"))?
            .to_string();
        let want = Layout::compute(w as usize, h as usize, ROWS, COLS, Scale::Auto);
        let says = format!("{w}x{h} at 32 bpp");
        let grid = format!(
            "grid at scale {} from ({}, {})",
            want.scale, want.x0, want.y0
        );
        if !line.contains(&says) || !line.contains(&grid) {
            return Err(format!("want {says} and {grid}; {line}"));
        }
        // The text again right before the capture: the screen may move on.
        let text = s.command("screen", CMD).unwrap_or(text);
        let raw = self.out_dir.join(format!("display-{w}x{h}.ppm"));
        let mut qmp = crate::qemu::qmp::Qmp::connect(&self.launch.qmp)?;
        qmp.screendump_ppm(&raw)?;
        let capture = read_ppm(&raw)?;
        if (capture.w, capture.h) != (w as usize, h as usize) {
            return Err(format!(
                "the display is {}x{}, not {w}x{h}",
                capture.w, capture.h
            ));
        }
        let (gold, layout) = golden(&text, capture.w, capture.h);
        let png = self.out_dir.join(format!("display-{w}x{h}.png"));
        let gold_png = self.out_dir.join(format!("display-{w}x{h}-golden.png"));
        crate::png::write_png(&png, &capture)?;
        crate::png::write_png(&gold_png, &gold)?;
        let _ = fs::remove_file(&raw);
        let got = compare(&capture, &gold, &layout);
        if !got.ok() {
            return Err(format!(
                "{got:?}\n{line}\ncapture {}, golden {}",
                png.display(),
                gold_png.display()
            ));
        }
        Ok(format!(
            "scale {}: {} cells exact, {} underlined, {} reversed; margins dark",
            layout.scale, got.exact, got.underlined, got.reversed
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen_text() -> String {
        let mut rows = vec![" ".repeat(COLS); ROWS];
        rows[0] = format!("{:<80}", "                             Sign On");
        rows[5] = format!("{:<80}", " User  . . . . . . . . . . . .   ");
        rows.join("\n")
    }

    #[test]
    fn the_golden_matches_itself_and_a_shift_does_not() {
        let text = screen_text();
        let (gold, l) = golden(&text, 1920, 1080);
        assert_eq!(l.scale, 2);
        let c = compare(&gold, &gold, &l);
        assert!(c.ok(), "{c:?}");
        assert_eq!(c.exact, ROWS * COLS);
        // The same picture two pixels to the right.
        let mut shifted = Image {
            w: gold.w,
            h: gold.h,
            rgb: vec![0; gold.rgb.len()],
        };
        for y in 0..gold.h {
            for x in 2..gold.w {
                let (a, b) = ((y * gold.w + x) * 3, (y * gold.w + x - 2) * 3);
                shifted.rgb[a..a + 3].copy_from_slice(&gold.rgb[b..b + 3]);
            }
        }
        assert!(!compare(&shifted, &gold, &l).ok());
        // One pixel of a 2×2 block off: ragged.
        let mut ragged = Image {
            w: gold.w,
            h: gold.h,
            rgb: gold.rgb.clone(),
        };
        let i = ((l.y0 + 1) * gold.w + l.x0 + 1) * 3;
        ragged.rgb[i] ^= 0xff;
        assert_eq!(compare(&ragged, &gold, &l).ragged, 1);
        // A lit margin pixel.
        let mut margin = Image {
            w: gold.w,
            h: gold.h,
            rgb: gold.rgb.clone(),
        };
        margin.rgb[0] = 0xff;
        assert_eq!(compare(&margin, &gold, &l).margin, 1);
    }

    #[test]
    fn underlines_and_reverse_image_are_allowed() {
        let text = screen_text();
        let (gold, l) = golden(&text, 1024, 768);
        assert_eq!(l.scale, 1);
        let mut cap = Image {
            w: gold.w,
            h: gold.h,
            rgb: gold.rgb.clone(),
        };
        // An underline under row 6, column 35; row 10 column 1 reversed.
        let (x, y) = (l.x0 + 34 * GLYPH_W, l.y0 + 5 * GLYPH_H + GLYPH_H - 2);
        for dx in 0..GLYPH_W {
            let i = (y * cap.w + x + dx) * 3;
            cap.rgb[i..i + 3].copy_from_slice(&[0x33, 0xcc, 0x33]);
        }
        let (x, y) = (l.x0, l.y0 + 9 * GLYPH_H);
        for dy in 0..GLYPH_H {
            for dx in 0..GLYPH_W {
                let i = ((y + dy) * cap.w + x + dx) * 3;
                cap.rgb[i..i + 3].copy_from_slice(&[0x33, 0xcc, 0x33]);
            }
        }
        let c = compare(&cap, &gold, &l);
        assert!(c.ok(), "{c:?}");
        assert_eq!((c.underlined, c.reversed), (1, 1));
    }
}
