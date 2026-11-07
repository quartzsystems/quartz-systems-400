// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The console's 9×16 bitmap font (`lic/console/font/qs400-9x16.txt`):
//! blocks of a `# U+XXXX name` line and 16 rows of nine `#` or `.`.
//! A character the font lacks draws as the console's hollow box.

use std::sync::OnceLock;

/// Cell width in pixels.
pub const GLYPH_W: usize = 9;
/// Cell height in pixels.
pub const GLYPH_H: usize = 16;

/// The font's text, as the console compiles it.
pub const SOURCE: &str = include_str!("../../../../../lic/console/font/qs400-9x16.txt");

/// The 5250 column separator glyph (private use).
pub const COLUMN_SEPARATOR: char = '\u{e000}';

/// The hollow box.
static REPLACEMENT: [u16; GLYPH_H] = [
    0x000, 0x000, 0x1fc, 0x104, 0x104, 0x104, 0x104, 0x104, 0x104, 0x104, 0x104, 0x104, 0x1fc,
    0x000, 0x000, 0x000,
];

/// Glyph rows, the leftmost pixel in bit 8, sorted by character.
type Glyphs = Vec<(char, [u16; GLYPH_H])>;

fn glyphs() -> &'static Glyphs {
    static G: OnceLock<Glyphs> = OnceLock::new();
    G.get_or_init(|| parse(SOURCE).unwrap_or_default())
}

/// Read the font format; other `#` lines are comments.
pub fn parse(text: &str) -> Result<Glyphs, String> {
    let mut out: Glyphs = Vec::new();
    let mut lines = text.lines().enumerate();
    while let Some((n, line)) = lines.next() {
        let Some(rest) = line.strip_prefix("# U+") else {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            return Err(format!("line {}: unexpected {line:?}", n + 1));
        };
        let hex = rest.split_whitespace().next().unwrap_or("");
        let cp = u32::from_str_radix(hex, 16)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(|| format!("line {}: bad code point", n + 1))?;
        let mut rows = [0u16; GLYPH_H];
        for row in &mut rows {
            let (m, px) = lines
                .next()
                .ok_or_else(|| format!("U+{:04X}: rows missing", cp as u32))?;
            if px.chars().count() != GLYPH_W {
                return Err(format!("line {}: a row is {GLYPH_W} wide", m + 1));
            }
            for (x, c) in px.chars().enumerate() {
                match c {
                    '#' => *row |= 1 << (GLYPH_W - 1 - x),
                    '.' => {}
                    _ => return Err(format!("line {}: unexpected {c:?}", m + 1)),
                }
            }
        }
        out.push((cp, rows));
    }
    out.sort_by_key(|g| g.0);
    Ok(out)
}

/// A character's rows, or the hollow box.
pub fn glyph(c: char) -> &'static [u16; GLYPH_H] {
    let g = glyphs();
    match g.binary_search_by_key(&c, |e| e.0) {
        Ok(i) => &g[i].1,
        Err(_) => &REPLACEMENT,
    }
}

/// Whether the font has a glyph for `c`.
pub fn has(c: char) -> bool {
    glyphs().binary_search_by_key(&c, |e| e.0).is_ok()
}

/// Every glyph as `(code point, 16 rows)`, for the session page's
/// script, which draws with the same font.
pub fn table() -> &'static Glyphs {
    glyphs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_console_glyph_set() {
        assert!(parse(SOURCE).is_ok());
        for b in 0x21u8..=0x7E {
            assert!(glyph(b as char).iter().any(|r| *r != 0), "{}", b as char);
        }
        assert!(glyph(' ').iter().all(|r| *r == 0));
        assert!(has(COLUMN_SEPARATOR) && has('\u{253c}'));
        assert!(std::ptr::eq(glyph('\u{4e00}'), &raw const REPLACEMENT));
    }
}
