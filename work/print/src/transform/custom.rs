// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SCS through a workstation customizing object: the same walk as the
//! PCL transform, every command taken from the object. A command the
//! object lacks is left out. The paper is the first of the object's
//! the page fits on, portrait before landscape; when none does, the
//! pitch and then the line density rise through the object's own
//! entries until one fits.

use super::ascii::ascii_line;
use super::page;
use crate::job::PrintJob;
use crate::wscst::Wscst;

/// The fit: pitch, density, and the paper command with its orientation.
struct Fit {
    cpi: u8,
    lpi: u8,
    paper: Option<(Vec<u8>, bool)>,
}

/// The pitch and paper for `job` under `w`: the file's pitch and
/// density when the page fits a paper at them, else the smallest
/// larger ones in the object that fit.
fn fit(w: &Wscst, job: &PrintJob) -> Fit {
    let cpi0 = page::font_pitch(&job.font).unwrap_or(job.cpi).max(1);
    let lpi0 = job.lpi.max(1);
    let mut pitches: Vec<u8> = vec![cpi0];
    pitches.extend(w.cpi.keys().copied().filter(|c| *c > cpi0));
    let mut densities: Vec<u8> = vec![lpi0];
    densities.extend(w.lpi.keys().copied().filter(|l| *l > lpi0));
    for &cpi in &pitches {
        for &lpi in &densities {
            let width = u32::from(job.page_width.max(1)) * 100 / u32::from(cpi);
            let height = u32::from(job.page_len.max(1)) * 100 / u32::from(lpi);
            if let Some((p, land)) = w.paper_for(width, height) {
                return Fit {
                    cpi,
                    lpi,
                    paper: Some((p.cmd.clone(), land)),
                };
            }
        }
    }
    Fit {
        cpi: cpi0,
        lpi: lpi0,
        paper: None,
    }
}

/// `scs` through `w`.
#[must_use]
pub fn render(w: &Wscst, job: &PrintJob, scs: &[u8]) -> Vec<u8> {
    let (pages, fmt) = spool::scs::decode(scs);
    let laid = PrintJob {
        page_width: if fmt.width > 0 { fmt.width } else { job.page_width },
        page_len: if fmt.len > 0 { fmt.len } else { job.page_len },
        ..job.clone()
    };
    let f = fit(w, &laid);
    let mut out = w.init.clone();
    if let Some((cmd, landscape)) = f.paper {
        out.extend_from_slice(&cmd);
        out.extend_from_slice(if landscape { &w.landscape } else { &w.portrait });
    }
    if job.copies > 1 && !w.copies.is_empty() {
        let s = String::from_utf8_lossy(&w.copies).replace("&n", &job.copies.to_string());
        out.extend_from_slice(s.as_bytes());
    }
    if let Some(cmd) = w.drawer.get(&job.drawer) {
        out.extend_from_slice(cmd);
    }
    if let Some(cmd) = w.duplex.get(job.duplex.name()) {
        out.extend_from_slice(cmd);
    }
    if let Some(cmd) = w.lpi_cmd(f.lpi) {
        out.extend_from_slice(cmd);
    }
    if let Some(cmd) = w.cpi_cmd(f.cpi) {
        out.extend_from_slice(cmd);
    }
    for page in &pages {
        for n in 1..=page.last_line() {
            if let Some(l) = page.lines.get(&n) {
                out.extend_from_slice(ascii_line(l, laid.page_width).as_bytes());
            }
            out.extend_from_slice(&w.newline);
        }
        out.extend_from_slice(&w.formfeed);
    }
    out.extend_from_slice(&w.exit);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use spool::{PageFormat, Printer};

    #[test]
    fn uses_the_objects_commands() {
        let w = Wscst::parse(Wscst::sample_pcl()).unwrap();
        let mut p = Printer::new(PageFormat {
            width: 80,
            ..PageFormat::default()
        });
        p.write_line(&ccsid::to_ebcdic("HELLO").unwrap());
        let job = PrintJob {
            page_width: 80,
            copies: 3,
            drawer: 2,
            ..PrintJob::default()
        };
        let out = render(&w, &job, &p.take());
        let text = String::from_utf8_lossy(&out);
        assert!(text.starts_with("\x1bE\x1b&l0E\x1b&a0L\x1b&s1C\x1b(10U\x1b&l2A\x1b&l0O\x1b&l3X\x1b&l4H\x1b&l0S\x1b&l6D\x1b(s0p10h12v0s0b4099T"));
        assert!(text.ends_with("HELLO\r\n\x0c\x1bE"));
    }

    #[test]
    fn reduces_through_the_objects_entries() {
        let w = Wscst::parse(Wscst::sample_pcl()).unwrap();
        // 132 by 66 at 10 and 6: at 8 lpi the page is 13.2 by 8.25 in,
        // legal landscape, the first paper that holds it.
        let f = fit(&w, &PrintJob::default());
        assert_eq!((f.cpi, f.lpi), (10, 8));
        assert_eq!(f.paper.unwrap(), (b"\x1b&l3A".to_vec(), true));
    }
}
