// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compiled glyph table and lookup.

include!(concat!(env!("OUT_DIR"), "/font.rs"));

/// The 5250 column separator glyph (private use).
pub const COLUMN_SEPARATOR: char = '\u{e000}';

/// Rows of a glyph, or the replacement box for characters the font lacks.
pub fn glyph(c: char) -> &'static [u16; GLYPH_H] {
    match GLYPHS.binary_search_by_key(&c, |(ch, _)| *ch) {
        Ok(i) => &GLYPHS[i].1,
        Err(_) => &REPLACEMENT,
    }
}

/// A hollow box, for characters with no glyph.
static REPLACEMENT: [u16; GLYPH_H] = [
    0x000, 0x000, 0x1fc, 0x104, 0x104, 0x104, 0x104, 0x104, 0x104, 0x104, 0x104, 0x104, 0x1fc,
    0x000, 0x000, 0x000,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_printable_ascii_glyph_exists_and_is_drawn() {
        for b in 0x21u8..=0x7e {
            let c = b as char;
            let rows = glyph(c);
            assert!(
                !std::ptr::eq(rows, &raw const REPLACEMENT),
                "{c:?} has no glyph"
            );
            assert!(rows.iter().any(|r| *r != 0), "{c:?} is empty");
        }
        assert!(glyph(' ').iter().all(|r| *r == 0));
    }

    #[test]
    fn box_drawing_and_separator_exist() {
        for c in [
            '\u{2500}',
            '\u{2502}',
            '\u{250c}',
            '\u{2510}',
            '\u{2514}',
            '\u{2518}',
            '\u{251c}',
            '\u{2524}',
            '\u{252c}',
            '\u{2534}',
            '\u{253c}',
            COLUMN_SEPARATOR,
        ] {
            assert!(
                !std::ptr::eq(glyph(c), &raw const REPLACEMENT),
                "{c:?} missing"
            );
        }
        assert!(std::ptr::eq(glyph('\u{0100}'), &raw const REPLACEMENT));
    }

    #[test]
    fn every_printable_character_of_the_ccsids_has_a_glyph() {
        use ccsid::conv::Codec;
        for id in [37u16, 273, 285, 297, 500] {
            let codec = Codec::of(id).unwrap().unwrap();
            let bytes: Vec<u8> = (0..=255).collect();
            let mut chars = Vec::new();
            codec.decode(&bytes, &mut chars);
            assert_eq!(chars.len(), 256);
            for (b, c) in chars.into_iter().enumerate() {
                if c.is_control() {
                    continue;
                }
                assert!(
                    !std::ptr::eq(glyph(c), &raw const REPLACEMENT),
                    "CCSID {id} byte {b:#04x} ({c:?}) has no glyph"
                );
            }
        }
    }

    #[test]
    fn latin1_and_euro_are_drawn() {
        for c in ('\u{a1}'..='\u{ff}').chain(['\u{20ac}']) {
            let rows = glyph(c);
            assert!(rows.iter().any(|r| *r != 0), "{c:?} is empty");
            assert!(rows.iter().all(|r| r & 1 == 0), "{c:?} fills the gap");
        }
        assert!(glyph('\u{a0}').iter().all(|r| *r == 0));
    }

    #[test]
    fn glyphs_fit_the_cell() {
        for (_, rows) in GLYPHS {
            assert!(rows.iter().all(|r| *r < 1 << GLYPH_W));
        }
    }
}
