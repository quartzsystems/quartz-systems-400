// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Page geometry: the paper a spooled page fits on, its orientation,
//! and the pitch and line density that make a wide report fit letter
//! paper (computer output reduction), plus the symbol set a `CHRID`
//! maps to.

use crate::job::PrintJob;

/// A paper size the transform knows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paper {
    /// Its name.
    pub name: &'static str,
    /// PCL `ESC &l#A` code.
    pub pcl: u8,
    /// Width in hundredths of an inch.
    pub width: u32,
    /// Height in hundredths of an inch.
    pub height: u32,
    /// The IPP `media` keyword.
    pub media: &'static str,
}

/// Letter.
pub const LETTER: Paper = Paper {
    name: "Letter",
    pcl: 2,
    width: 850,
    height: 1100,
    media: "na_letter_8.5x11in",
};
/// A4.
pub const A4: Paper = Paper {
    name: "A4",
    pcl: 26,
    width: 827,
    height: 1169,
    media: "iso_a4_210x297mm",
};
/// Legal.
pub const LEGAL: Paper = Paper {
    name: "Legal",
    pcl: 3,
    width: 850,
    height: 1400,
    media: "na_legal_8.5x14in",
};
/// A3.
pub const A3: Paper = Paper {
    name: "A3",
    pcl: 27,
    width: 1169,
    height: 1654,
    media: "iso_a3_297x420mm",
};
/// Ledger.
pub const LEDGER: Paper = Paper {
    name: "Ledger",
    pcl: 6,
    width: 1100,
    height: 1700,
    media: "na_ledger_11x17in",
};

/// The papers a page is placed on before anything is reduced, in
/// order of preference.
pub const SMALL: [Paper; 3] = [LETTER, A4, LEGAL];
/// The papers tried when nothing fits letter even reduced.
pub const LARGE: [Paper; 2] = [A3, LEDGER];

/// The paper and orientation a page was laid out for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    /// The paper.
    pub paper: Paper,
    /// Printed across the long edge.
    pub landscape: bool,
    /// Characters per inch to print at: the file's, or the pitch that
    /// makes the width fit.
    pub cpi: u8,
    /// Lines per inch: the file's, or the density that makes the
    /// length fit.
    pub lpi: u8,
}

/// Whether a page `w` by `h` (hundredths of an inch) fits `p`.
fn fits(p: Paper, landscape: bool, w: u32, h: u32) -> bool {
    if landscape {
        w <= p.height && h <= p.width
    } else {
        w <= p.width && h <= p.height
    }
}

/// The page in hundredths of an inch at `cpi` and `lpi`.
fn extent(job: &PrintJob, cpi: u8, lpi: u8) -> (u32, u32) {
    let w = u32::from(job.page_width.max(1)) * 100 / u32::from(cpi.max(1));
    let h = u32::from(job.page_len.max(1)) * 100 / u32::from(lpi.max(1));
    (w, h)
}

/// The pitches tried from `cpi` up, then the densities from `lpi` up.
fn steps(cpi: u8, lpi: u8) -> (Vec<u8>, Vec<u8>) {
    let mut cpis = vec![cpi];
    cpis.extend([12, 15, 17, 20].into_iter().filter(|c| *c > cpi));
    let mut lpis = vec![lpi];
    lpis.extend([8, 9, 12].into_iter().filter(|l| *l > lpi));
    (cpis, lpis)
}

/// Where the page goes. At its own pitch and density, letter, A4 or
/// legal, portrait before landscape. Failing that, computer output
/// reduction onto letter: the pitch rises through 12, 15, 16.67 and
/// 20 and the density through 8, 9 and 12 until the page fits. Failing
/// that, A3 or ledger at the file's pitch; and last, ledger landscape
/// at 20 and 12, clipped.
#[must_use]
pub fn layout(job: &PrintJob) -> Layout {
    let (cpi, lpi) = (job.cpi.max(1), job.lpi.max(1));
    let at = |paper, landscape, cpi, lpi| Layout {
        paper,
        landscape,
        cpi,
        lpi,
    };
    let (w, h) = extent(job, cpi, lpi);
    for p in SMALL {
        for landscape in [false, true] {
            if fits(p, landscape, w, h) {
                return at(p, landscape, cpi, lpi);
            }
        }
    }
    let (cpis, lpis) = steps(cpi, lpi);
    for &c in &cpis {
        for &l in &lpis {
            let (w, h) = extent(job, c, l);
            for landscape in [false, true] {
                if fits(LETTER, landscape, w, h) {
                    return at(LETTER, landscape, c, l);
                }
            }
        }
    }
    for p in LARGE {
        for landscape in [false, true] {
            if fits(p, landscape, w, h) {
                return at(p, landscape, cpi, lpi);
            }
        }
    }
    at(LEDGER, true, 20, 12)
}

/// The PCL symbol set for a `CHRID`: the code page's PC or Windows
/// set, PC-8 (`10U`) when nothing better is known.
#[must_use]
pub fn symbol_set(chrid: &str) -> &'static str {
    let cpgid = chrid
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse::<u32>().ok());
    match cpgid {
        Some(850) => "12U",
        Some(852) => "17U",
        Some(857) => "9T",
        Some(860) => "8U",
        Some(1252) => "19U",
        Some(819 | 1051 | 1275) => "0N",
        _ => "10U",
    }
}

/// The IBM font identifier's pitch, when the identifier fixes one:
/// `011` Courier 10, `085` Courier 12, `222` Gothic 15, `223` Courier
/// 15, `252` Courier 17, `254` Courier 17 (compressed), `290` Gothic 20.
#[must_use]
pub fn font_pitch(font: &str) -> Option<u8> {
    match font.trim().trim_start_matches('0') {
        "11" | "12" | "18" | "19" | "20" | "3" | "5" => Some(10),
        "85" | "86" | "87" | "91" | "92" | "95" => Some(12),
        "222" | "223" | "225" | "226" | "229" | "230" | "245" => Some(15),
        "252" | "253" | "254" | "255" | "256" | "258" => Some(17),
        "290" | "293" => Some(20),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(width: u16, len: u16, cpi: u8, lpi: u8) -> (&'static str, bool, u8, u8) {
        let l = layout(&PrintJob {
            page_width: width,
            page_len: len,
            cpi,
            lpi,
            ..PrintJob::default()
        });
        (l.paper.name, l.landscape, l.cpi, l.lpi)
    }

    #[test]
    fn letter_portrait_for_a_narrow_page() {
        assert_eq!(place(80, 66, 10, 6), ("Letter", false, 10, 6));
        assert_eq!(place(120, 66, 15, 6), ("Letter", false, 15, 6));
        // 132 at 15 cpi is 8.8 in: too wide for portrait.
        assert_eq!(place(132, 66, 15, 6), ("Letter", true, 15, 8));
    }

    #[test]
    fn wide_reports_are_reduced_onto_letter() {
        // 132 at 10 cpi is 13.2 by 11 in: 12 cpi and 8 lpi make it 11
        // by 8.25, letter landscape.
        assert_eq!(place(132, 66, 10, 6), ("Letter", true, 12, 8));
        // 198 columns need 20 cpi (9.9 in).
        assert_eq!(place(198, 66, 10, 6), ("Letter", true, 20, 8));
        // 100 columns at 10 cpi fit letter landscape at 12 lpi... but 8
        // lpi (8.25 in) comes first.
        assert_eq!(place(100, 66, 10, 6), ("Letter", true, 10, 8));
    }

    #[test]
    fn other_papers() {
        assert_eq!(place(80, 84, 10, 6), ("Legal", false, 10, 6));
        assert_eq!(place(72, 70, 10, 6), ("A4", false, 10, 6));
        // 400 columns fit nothing: ledger landscape, clipped.
        assert_eq!(place(400, 66, 10, 6), ("Ledger", true, 20, 12));
    }

    #[test]
    fn symbol_sets_and_fonts() {
        assert_eq!(symbol_set("*DEVD"), "10U");
        assert_eq!(symbol_set("697 850"), "12U");
        assert_eq!(symbol_set("695 1252"), "19U");
        assert_eq!(font_pitch("011"), Some(10));
        assert_eq!(font_pitch("252"), Some(17));
        assert_eq!(font_pitch("*CPI"), None);
    }
}
