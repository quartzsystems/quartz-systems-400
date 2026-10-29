// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SCS to plain text: every line of every page from line 1 to the last
//! printed one, trailing blanks dropped, CR LF line ends, a form feed
//! closing each page. The SCS presentation commands are honored by the
//! decoder: SHF clips the width, SVF ends the page, the horizontal and
//! vertical moves become the blanks and blank lines they stand for.
//! Characters outside ASCII take the code page `CHRID` names (437, 850,
//! 1252, 819 and their kin, as the PCL transform maps them); with
//! `*DEVD` or an unknown one they print as `?`.

use super::pcl;
use crate::job::PrintJob;

/// A line of text as 7-bit ASCII, clipped to `width` (0 for no clip).
#[must_use]
pub fn ascii_line(line: &str, width: u16) -> String {
    let mut out: String = line
        .chars()
        .map(|c| if c.is_ascii() && !c.is_ascii_control() { c } else { '?' })
        .collect();
    if width > 0 {
        out.truncate(usize::from(width));
    }
    out.trim_end().to_string()
}

/// Whether `chrid` names a code page (`gcgid cpgid`).
fn has_code_page(chrid: &str) -> bool {
    chrid
        .split_whitespace()
        .nth(1)
        .is_some_and(|c| c.parse::<u32>().is_ok())
}

/// A line in the code page of `chrid`, clipped and trimmed.
#[must_use]
pub fn encode_line(line: &str, width: u16, chrid: &str) -> Vec<u8> {
    if !has_code_page(chrid) {
        return ascii_line(line, width).into_bytes();
    }
    let enc = pcl::encoder(chrid);
    let take = if width > 0 { usize::from(width) } else { usize::MAX };
    let mut out: Vec<u8> = line.chars().take(take).map(enc).collect();
    while out.last() == Some(&b' ') {
        out.pop();
    }
    out
}

/// The text of `scs`.
#[must_use]
pub fn render(job: &PrintJob, scs: &[u8]) -> Vec<u8> {
    let (pages, fmt) = spool::scs::decode(scs);
    let width = if fmt.width > 0 { fmt.width } else { job.page_width };
    let mut out = Vec::new();
    for page in &pages {
        for n in 1..=page.last_line() {
            if let Some(l) = page.lines.get(&n) {
                out.extend_from_slice(&encode_line(l, width, &job.chrid));
            }
            out.extend_from_slice(b"\r\n");
        }
        out.push(0x0C);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use spool::{Control, PageFormat, Printer};

    #[test]
    fn text_with_form_feeds() {
        let mut p = Printer::new(PageFormat {
            len: 4,
            width: 20,
            ..PageFormat::default()
        });
        let e = |s: &str| ccsid::to_ebcdic(s).unwrap();
        p.write_line(&e("ONE"));
        p.write(
            &e("THREE is a long line past twenty"),
            Control {
                space_before: 2,
                ..Control::default()
            },
        );
        p.write_line(&e("FIVE"));
        let out = render(&PrintJob::default(), &p.take());
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "ONE\r\n\r\nTHREE is a long line\r\nFIVE\r\n\x0c"
        );
        assert_eq!(ascii_line("caf\u{e9}  ", 0), "caf?");
    }

    #[test]
    fn code_pages() {
        assert_eq!(encode_line("caf\u{e9}  ", 0, "*DEVD"), b"caf?");
        assert_eq!(encode_line("caf\u{e9}  ", 0, "697 850"), b"caf\x82");
        assert_eq!(encode_line("caf\u{e9}", 3, "695 1252"), b"caf");
        assert_eq!(encode_line("caf\u{e9}", 0, "695 1252"), b"caf\xe9");
    }
}
