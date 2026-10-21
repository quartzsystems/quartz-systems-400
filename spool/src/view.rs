// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a spooled file looks like to people and programs: plain text,
//! records with FCFC or `*PRTCTL` carriage control (`CPYSPLF`), and a
//! hex dump (`DSPSPLF *HEX`).

use std::fmt::Write as _;

use crate::scs::Page;

/// The pages as text: every line from 1 to the last printed one, pages
/// separated by a form feed.
#[must_use]
pub fn text(pages: &[Page]) -> String {
    let mut out = String::new();
    for (i, p) in pages.iter().enumerate() {
        if i > 0 {
            out.push('\x0c');
        }
        for n in 1..=p.last_line() {
            if let Some(l) = p.lines.get(&n) {
                out.push_str(l);
            }
            out.push('\n');
        }
    }
    out
}

/// The pages as `(page, line, text)` rows, blank lines included.
#[must_use]
pub fn rows(pages: &[Page]) -> Vec<(u32, u16, String)> {
    let mut out = Vec::new();
    for (i, p) in pages.iter().enumerate() {
        let page = u32::try_from(i + 1).unwrap_or(u32::MAX);
        for n in 1..=p.last_line() {
            out.push((page, n, p.lines.get(&n).cloned().unwrap_or_default()));
        }
    }
    out
}

/// One record per printed line, the first character its FCFC control:
/// `1` the first line of a page (skipping to it), blank, `0` and `-` for
/// one, two and three lines down, more blank lines as needed.
#[must_use]
pub fn fcfc(pages: &[Page]) -> Vec<String> {
    let mut out = Vec::new();
    for p in pages {
        let mut at = 0u16;
        for (&n, l) in &p.lines {
            if at == 0 {
                if n == 1 {
                    out.push(format!("1{l}"));
                    at = 1;
                    continue;
                }
                out.push("1".into());
                at = 1;
            }
            let mut gap = n - at;
            while gap > 3 {
                out.push("-".into());
                gap -= 3;
            }
            let c = match gap {
                0 => '+',
                1 => ' ',
                2 => '0',
                _ => '-',
            };
            out.push(format!("{c}{l}"));
            at = n;
        }
    }
    out
}

/// One record per printed line with a `*PRTCTL` prefix: the line to
/// skip to before (three digits, blank when none) and the lines to
/// space before (one digit).
#[must_use]
pub fn prtctl(pages: &[Page]) -> Vec<String> {
    let mut out = Vec::new();
    for p in pages {
        let mut at = 0u16;
        for (&n, l) in &p.lines {
            if at == 0 || n.saturating_sub(at) > 3 {
                out.push(format!("{n:03}0{l}"));
            } else {
                out.push(format!("   {}{l}", n - at));
            }
            at = n;
        }
    }
    out
}

/// A hex dump of SCS data, 16 bytes a row, with the characters.
#[must_use]
pub fn hex(data: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in data.chunks(16).enumerate() {
        let _ = write!(out, "{:08X}  ", i * 16);
        for b in chunk {
            let _ = write!(out, "{b:02X} ");
        }
        for _ in chunk.len()..16 {
            out.push_str("   ");
        }
        out.push(' ');
        for &b in chunk {
            let c = ccsid::char_of(b);
            out.push(if b >= 0x40 && !c.is_control() { c } else { '.' });
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printer::{Control, PageFormat, Printer};
    use crate::scs::decode;

    #[test]
    fn views() {
        let mut p = Printer::new(PageFormat::default());
        let e = |s: &str| ccsid::to_ebcdic(s).unwrap();
        p.write_line(&e("ONE"));
        p.write(&e("THREE"), Control { space_before: 2, ..Control::default() });
        p.write(&e("NINE"), Control { space_before: 6, ..Control::default() });
        p.write(&e("NEXT"), Control { skip_before: 2, ..Control::default() });
        let data = p.take();
        let (pages, _) = decode(&data);
        assert_eq!(text(&pages), "ONE\n\nTHREE\n\n\n\n\n\nNINE\n\x0c\nNEXT\n");
        assert_eq!(fcfc(&pages), ["1ONE", "0THREE", "-", "-NINE", "1", " NEXT"]);
        assert_eq!(prtctl(&pages), ["0010ONE", "   2THREE", "0090NINE", "0020NEXT"]);
        assert_eq!(rows(&pages).len(), 11);
        assert!(hex(&data).starts_with("00000000  2B D1"));
    }
}
