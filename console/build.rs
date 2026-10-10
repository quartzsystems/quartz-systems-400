// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Compiles `font/qs400-9x16.txt` into a glyph table. Each row of nine
//! pixels becomes a `u16` with the leftmost pixel in bit 8.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

const SOURCE: &str = "font/qs400-9x16.txt";
const WIDTH: usize = 9;
const HEIGHT: usize = 16;

fn main() {
    println!("cargo:rerun-if-changed={SOURCE}");
    let text = fs::read_to_string(SOURCE).unwrap_or_else(|e| panic!("{SOURCE}: {e}"));
    let glyphs = parse(&text);

    let mut out = String::new();
    writeln!(
        out,
        "/// Cell width in pixels.\npub const GLYPH_W: usize = {WIDTH};"
    )
    .unwrap();
    writeln!(
        out,
        "/// Cell height in pixels.\npub const GLYPH_H: usize = {HEIGHT};"
    )
    .unwrap();
    writeln!(
        out,
        "/// Glyphs sorted by code point; rows have the leftmost pixel in bit 8."
    )
    .unwrap();
    writeln!(out, "pub static GLYPHS: &[(char, [u16; {HEIGHT}])] = &[").unwrap();
    for (cp, rows) in &glyphs {
        write!(out, "    ('\\u{{{cp:x}}}', [").unwrap();
        for r in rows {
            write!(out, "{r:#05x}, ").unwrap();
        }
        writeln!(out, "]),").unwrap();
    }
    writeln!(out, "];").unwrap();

    let dest = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("font.rs");
    fs::write(&dest, out).expect("write font.rs");
}

/// Blocks of `# U+XXXX name` followed by `HEIGHT` rows of `WIDTH` pixels.
fn parse(text: &str) -> Vec<(u32, [u16; HEIGHT])> {
    let mut glyphs: Vec<(u32, [u16; HEIGHT])> = Vec::new();
    let mut lines = text.lines().enumerate().peekable();
    while let Some((n, line)) = lines.next() {
        let Some(rest) = line.strip_prefix("# U+") else {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            panic!("{SOURCE}:{}: unexpected line {line:?}", n + 1);
        };
        let hex = rest.split_whitespace().next().unwrap_or("");
        let cp = u32::from_str_radix(hex, 16)
            .unwrap_or_else(|_| panic!("{SOURCE}:{}: bad code point", n + 1));
        let mut rows = [0u16; HEIGHT];
        for (i, row) in rows.iter_mut().enumerate() {
            let (m, pixels) = lines
                .next()
                .unwrap_or_else(|| panic!("{SOURCE}: glyph U+{cp:04X} is missing row {i}"));
            assert!(
                pixels.len() == WIDTH,
                "{SOURCE}:{}: row must be {WIDTH} characters",
                m + 1
            );
            let mut bits = 0u16;
            for (x, c) in pixels.chars().enumerate() {
                match c {
                    '#' => bits |= 1 << (WIDTH - 1 - x),
                    '.' => {}
                    other => panic!("{SOURCE}:{}: unexpected {other:?}", m + 1),
                }
            }
            *row = bits;
        }
        assert!(
            !glyphs.iter().any(|(c, _)| *c == cp),
            "{SOURCE}: duplicate glyph U+{cp:04X}"
        );
        glyphs.push((cp, rows));
    }
    glyphs.sort_by_key(|(cp, _)| *cp);
    glyphs
}
