// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SCS to PCL5 for the HP Laserjet family. One job: a reset, the page
//! (paper, orientation, copies, input tray, duplex), the line pitch and
//! text length, the font (Courier at the file's pitch, or the pitch
//! that makes a wide page fit), the symbol set of the `CHRID`; then
//! every page as lines ended by CR LF and a form feed; a reset at the
//! end. The commands are PCL5's as published; a PCL4 model gets the
//! bitmap-font selection its generation understood. Not verified
//! against IBM's transform (`docs/PRINT.md`).

use std::fmt::Write as _;

use super::page::{self, Layout};
use super::{Capabilities, Model};
use crate::job::{Duplex, PrintJob};

/// The escape character.
const ESC: char = '\x1b';

/// The pitch as PCL writes it: 17 is 16.67.
fn pitch(cpi: u8) -> &'static str {
    match cpi {
        12 => "12",
        15 => "15",
        17 => "16.67",
        20 => "20",
        _ => "10",
    }
}

/// The point size that suits a pitch.
fn points(cpi: u8) -> &'static str {
    match cpi {
        12 => "10",
        15 => "8",
        17 => "7",
        20 => "6",
        _ => "12",
    }
}

/// The nearest pitch a PCL4 bitmap Courier comes in.
fn pcl4_pitch(cpi: u8) -> u8 {
    match cpi {
        0..=10 => 10,
        11..=13 => 12,
        _ => 17,
    }
}

/// `ESC &l#H` for a drawer: 1 the upper tray, 2 the lower, 3 the
/// large-capacity tray, more than that automatic selection.
fn tray(drawer: u8) -> u8 {
    match drawer {
        0 | 1 => 1,
        2 => 4,
        3 => 5,
        _ => 7,
    }
}

/// A character in the symbol set: ASCII as is, Latin-1 letters by
/// their PC-8 (CP437) or Latin-1 code, anything else `?`.
pub(crate) fn encode(symbol_set: &str, c: char) -> u8 {
    if c.is_ascii() && !c.is_ascii_control() {
        return c as u8;
    }
    let latin1 = matches!(symbol_set, "19U" | "0N");
    if latin1 {
        return u32::from(c)
            .try_into()
            .ok()
            .filter(|b: &u8| *b >= 0xA0)
            .unwrap_or(b'?');
    }
    // CP437's accented letters and the symbols reports use.
    match c {
        '\u{c7}' => 0x80,
        '\u{fc}' => 0x81,
        '\u{e9}' => 0x82,
        '\u{e2}' => 0x83,
        '\u{e4}' => 0x84,
        '\u{e0}' => 0x85,
        '\u{e5}' => 0x86,
        '\u{e7}' => 0x87,
        '\u{ea}' => 0x88,
        '\u{eb}' => 0x89,
        '\u{e8}' => 0x8A,
        '\u{ef}' => 0x8B,
        '\u{ee}' => 0x8C,
        '\u{ec}' => 0x8D,
        '\u{c4}' => 0x8E,
        '\u{c5}' => 0x8F,
        '\u{c9}' => 0x90,
        '\u{e6}' => 0x91,
        '\u{c6}' => 0x92,
        '\u{f4}' => 0x93,
        '\u{f6}' => 0x94,
        '\u{f2}' => 0x95,
        '\u{fb}' => 0x96,
        '\u{f9}' => 0x97,
        '\u{ff}' => 0x98,
        '\u{d6}' => 0x99,
        '\u{dc}' => 0x9A,
        '\u{a2}' => 0x9B,
        '\u{a3}' => 0x9C,
        '\u{a5}' => 0x9D,
        '\u{e1}' => 0xA0,
        '\u{ed}' => 0xA1,
        '\u{f3}' => 0xA2,
        '\u{fa}' => 0xA3,
        '\u{f1}' => 0xA4,
        '\u{d1}' => 0xA5,
        '\u{bf}' => 0xA8,
        '\u{ac}' => 0xAA,
        '\u{bd}' => 0xAB,
        '\u{bc}' => 0xAC,
        '\u{a1}' => 0xAD,
        '\u{ab}' => 0xAE,
        '\u{bb}' => 0xAF,
        '\u{df}' => 0xE1,
        '\u{b5}' => 0xE6,
        '\u{b1}' => 0xF1,
        '\u{f7}' => 0xF6,
        '\u{b0}' => 0xF8,
        '\u{b7}' => 0xFA,
        '\u{b2}' => 0xFD,
        _ => b'?',
    }
}

/// The byte encoder for `chrid`: the code page's own table when the
/// system has one (850, 819, 1252), else [`encode`] in its symbol set.
pub(crate) fn encoder(chrid: &str) -> impl Fn(char) -> u8 {
    let set = page::symbol_set(chrid);
    let codec = chrid
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse::<u16>().ok())
        .and_then(|c| ccsid::conv::Codec::of(c).ok().flatten())
        .filter(|c| !c.is_ebcdic());
    move |c| {
        if c.is_ascii() && !c.is_ascii_control() {
            return c as u8;
        }
        match &codec {
            Some(k) => {
                let mut b = Vec::with_capacity(1);
                if k.encode(c, &mut b) { b[0] } else { b'?' }
            }
            None => encode(set, c),
        }
    }
}

/// The job setup for `model`: everything before the first page.
#[must_use]
pub fn setup(model: Model, job: &PrintJob, layout: Layout) -> String {
    let caps: Capabilities = model.capabilities();
    let mut s = String::new();
    let _ = write!(s, "{ESC}E");
    let _ = write!(s, "{ESC}&l{}A", layout.paper.pcl);
    let _ = write!(s, "{ESC}&l{}O", u8::from(layout.landscape));
    if job.copies > 1 {
        let _ = write!(s, "{ESC}&l{}X", job.copies);
    }
    if caps.trays {
        let _ = write!(s, "{ESC}&l{}H", tray(job.drawer));
    }
    if caps.duplex {
        let d = match job.duplex {
            Duplex::No => 0,
            Duplex::Yes => 1,
            Duplex::Tumble => 2,
        };
        let _ = write!(s, "{ESC}&l{d}S");
    }
    let _ = write!(s, "{ESC}&l{}D", layout.lpi);
    let _ = write!(s, "{ESC}&l0E");
    let _ = write!(s, "{ESC}&l{}F", job.page_len.max(1));
    let _ = write!(s, "{ESC}&a0L");
    let _ = write!(s, "{ESC}&s1C");
    let _ = write!(s, "{ESC}({}", page::symbol_set(&job.chrid));
    if caps.pcl5 {
        let _ = write!(
            s,
            "{ESC}(s0p{}h{}v0s0b4099T",
            pitch(layout.cpi),
            points(layout.cpi)
        );
    } else {
        let p = pcl4_pitch(layout.cpi);
        let _ = write!(s, "{ESC}(s0p{}h{}v0s0b3T", pitch(p), points(p));
    }
    s
}

/// The pitch the file asks for: `FONT` when it names one, else `CPI`.
#[must_use]
pub fn requested_cpi(job: &PrintJob) -> u8 {
    page::font_pitch(&job.font).unwrap_or(job.cpi)
}

/// The PCL5 of `scs` for `model`.
#[must_use]
pub fn render(model: Model, job: &PrintJob, scs: &[u8]) -> Vec<u8> {
    let (pages, fmt) = spool::scs::decode(scs);
    let laid = PrintJob {
        cpi: requested_cpi(job),
        page_width: if fmt.width > 0 { fmt.width } else { job.page_width },
        page_len: if fmt.len > 0 { fmt.len } else { job.page_len },
        ..job.clone()
    };
    let layout = page::layout(&laid);
    let enc = encoder(&job.chrid);
    let mut out = setup(model, &laid, layout).into_bytes();
    for page in &pages {
        for n in 1..=page.last_line() {
            if let Some(l) = page.lines.get(&n) {
                let chars: Vec<u8> = l
                    .chars()
                    .take(usize::from(laid.page_width.max(1)))
                    .map(&enc)
                    .collect();
                let end = chars.iter().rposition(|b| *b != b' ').map_or(0, |i| i + 1);
                out.extend_from_slice(&chars[..end]);
            }
            out.extend_from_slice(b"\r\n");
        }
        out.push(0x0C);
    }
    out.extend_from_slice(format!("{ESC}E").as_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_commands() {
        let job = PrintJob {
            page_width: 80,
            copies: 2,
            drawer: 2,
            duplex: Duplex::Yes,
            chrid: "697 850".into(),
            ..PrintJob::default()
        };
        let s = setup(Model::Hp4, &job, page::layout(&job));
        assert_eq!(
            s,
            "\x1bE\x1b&l2A\x1b&l0O\x1b&l2X\x1b&l4H\x1b&l1S\x1b&l6D\x1b&l0E\x1b&l66F\x1b&a0L\x1b&s1C\x1b(12U\x1b(s0p10h12v0s0b4099T"
        );
        // A Laserjet II has no trays, no duplexer and bitmap fonts.
        let s = setup(Model::HpII, &job, page::layout(&job));
        assert!(!s.contains("&l4H") && !s.contains("&l1S"));
        assert!(s.ends_with("\x1b(s0p10h12v0s0b3T"));
    }

    #[test]
    fn encodes_accents() {
        assert_eq!(encode("10U", '\u{e9}'), 0x82);
        assert_eq!(encode("19U", '\u{e9}'), 0xE9);
        assert_eq!(encode("10U", '\u{20ac}'), b'?');
        assert_eq!(encode("10U", 'A'), b'A');
    }

    #[test]
    fn font_overrides_pitch() {
        let job = PrintJob {
            font: "252".into(),
            ..PrintJob::default()
        };
        assert_eq!(requested_cpi(&job), 17);
        assert_eq!(pitch(17), "16.67");
        assert_eq!(pcl4_pitch(15), 17);
    }
}
