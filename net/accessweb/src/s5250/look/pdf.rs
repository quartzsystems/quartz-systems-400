// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Print Screen: one A4 landscape page, the screen traced as filled
//! rectangles in its own colours (each run of lit pixels on a pixel row
//! one rectangle), so it prints sharp at any size.

// Bit, byte and pixel arithmetic on values the formats bound.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::many_single_char_names
)]

use std::fmt::Write as _;

use super::deflate;
use super::render::Canvas;

/// A4 landscape, in points.
const PAGE: (f64, f64) = (842.0, 595.0);
/// Margin, in points.
const MARGIN: f64 = 36.0;

/// The picture `c` (drawn at scale 1, background `bg`) as a PDF titled
/// `title`.
pub fn print_screen(c: &Canvas, bg: u32, title: &str) -> Vec<u8> {
    let k = ((PAGE.0 - 2.0 * MARGIN) / c.w as f64).min((PAGE.1 - 2.0 * MARGIN) / c.h as f64);
    let (w, h) = (c.w as f64 * k, c.h as f64 * k);
    let (x0, y0) = ((PAGE.0 - w) / 2.0, (PAGE.1 - h) / 2.0);
    let mut ops = String::new();
    let _ = writeln!(ops, "q {k:.5} 0 0 {k:.5} {x0:.3} {y0:.3} cm");
    let mut color = u32::MAX;
    let mut set = |ops: &mut String, rgb: u32| {
        if rgb != color {
            let f = |s: u32| f64::from((rgb >> s) & 0xFF) / 255.0;
            let _ = writeln!(ops, "{:.4} {:.4} {:.4} rg", f(16), f(8), f(0));
            color = rgb;
        }
    };
    set(&mut ops, bg);
    let _ = writeln!(ops, "0 0 {} {} re f", c.w, c.h);
    for y in 0..c.h {
        let mut x = 0;
        while x < c.w {
            let px = c.at(x, y);
            let start = x;
            while x < c.w && c.at(x, y) == px {
                x += 1;
            }
            if px != bg {
                set(&mut ops, px);
                // PDF's y grows upwards.
                let _ = writeln!(ops, "{start} {} {} 1 re f", c.h - 1 - y, x - start);
            }
        }
    }
    ops.push_str("Q\n");
    let content = deflate::zlib(ops.as_bytes());
    let objects: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources << >> >>", PAGE.0, PAGE.1).into_bytes(),
        {
            let mut s = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", content.len()).into_bytes();
            s.extend_from_slice(&content);
            s.extend_from_slice(b"\nendstream");
            s
        },
        format!("<< /Title ({}) /Producer (QS/400 Access for Web) >>", pdf_text(title)).into_bytes(),
    ];
    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    let mut t = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for o in offsets {
        let _ = writeln!(t, "{o:010} 00000 n ");
    }
    let _ = write!(
        t,
        "trailer\n<< /Size {} /Root 1 0 R /Info 5 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.extend_from_slice(t.as_bytes());
    out
}

/// A PDF literal string's body.
fn pdf_text(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii() && !c.is_ascii_control())
        .flat_map(|c| match c {
            '(' | ')' | '\\' => vec!['\\', c],
            _ => vec![c],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_landscape_page() {
        let mut c = Canvas::new(18, 16, 0);
        c.fill(2, 3, 4, 1, 0x0033_CC33);
        let p = print_screen(&c, 0, "QSHQ400 (x)");
        let t = String::from_utf8_lossy(&p);
        assert!(t.starts_with("%PDF-1.4") && t.ends_with("%%EOF\n"));
        assert!(
            t.contains("/MediaBox [0 0 842 595]")
                && t.contains("/Count 1")
                && t.contains("(QSHQ400 \\(x\\))")
        );
    }
}
