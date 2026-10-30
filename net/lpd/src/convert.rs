// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A received job as spooled files, one per data file. A control file
//! with the QS/400 extension (`YQS400=1`, `work/print`'s LPR sink with
//! `DESTTYPE(*OS400)`) carries the sender's attributes and SCS, kept as
//! they are. Otherwise text (print commands `f`, `l`, `p`) becomes SCS
//! through the line printer, FORTRAN carriage control (`r`) through its
//! FCFC, and anything else (`o` PostScript, `v` raster, ...) is kept as
//! it came under device type `*USERASCII`, as IBM's LPD keeps
//! everything. The file is named from the source name (`N`), the user
//! (`P`) goes into the user data.

use print::job::PrintJob;
use spool::printer::{Control, PageFormat, Printer};

use crate::session::Job;

/// The name a file gets when its source name is no system name.
pub const DEFAULT_NAME: &str = "QPRTLPD";

/// One spooled file to create.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spooled {
    /// Its attributes; `job` is left for the creating job to fill.
    pub attrs: PrintJob,
    /// The data: SCS, or the bytes as received for `*USERASCII`.
    pub data: Vec<u8>,
    /// Pages.
    pub pages: u32,
    /// Records (lines).
    pub records: u32,
    /// The sending user (`P`).
    pub user: String,
    /// The sending host (`H`).
    pub host: String,
}

/// A system name from a source file name: the base name before its
/// first period, letters and digits, upper case, at most 10.
#[must_use]
pub fn file_name(source: &str) -> String {
    let base = source.rsplit(['/', '\\']).next().unwrap_or("");
    let stem = base.split('.').next().unwrap_or("");
    let name: String = stem
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '#' | '@' | '$'))
        .map(|c| c.to_ascii_uppercase())
        .take(10)
        .collect();
    if name.starts_with(|c: char| c.is_ascii_alphabetic() || matches!(c, '#' | '@' | '$')) {
        name
    } else {
        DEFAULT_NAME.into()
    }
}

/// Received bytes as text: UTF-8 when they are, else Latin-1.
fn decode(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => b.iter().map(|&c| char::from(c)).collect(),
    }
}

/// Tabs to every eighth column; other controls dropped.
fn expand(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for c in line.chars() {
        if c == '\t' {
            let n = 8 - out.chars().count() % 8;
            out.extend(std::iter::repeat_n(' ', n));
        } else if !c.is_control() {
            out.push(c);
        }
    }
    out
}

/// The page width text needs: 80, 132, or its longest line up to 198.
fn width_for(lines: &[String]) -> u16 {
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let w = if longest <= 80 {
        80
    } else if longest <= 132 {
        132
    } else {
        longest.min(198)
    };
    u16::try_from(w).unwrap_or(198)
}

/// Text as SCS: a line per LF, a new page per form feed; with `fcfc`
/// each line's first character is its carriage control.
#[must_use]
pub fn text_to_scs(b: &[u8], fcfc: bool) -> (Vec<u8>, u32, u32, u16) {
    let text = decode(b);
    let text = text.strip_suffix('\n').unwrap_or(&text);
    // Each record: whether a form feed comes before it, and its text.
    let mut records: Vec<(bool, String)> = Vec::new();
    let mut ff = false;
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        let paged = raw.contains('\x0c');
        for (i, seg) in raw.split('\x0c').enumerate() {
            ff |= i > 0;
            // A form feed alone on its line moves to the next record.
            if paged && seg.is_empty() {
                continue;
            }
            records.push((ff, expand(seg)));
            ff = false;
        }
    }
    let lines: Vec<String> = records.iter().map(|(_, l)| l.clone()).collect();
    let width = width_for(&lines);
    let mut p = Printer::new(PageFormat {
        width,
        ..PageFormat::default()
    });
    for (ff, line) in &records {
        if fcfc {
            let (c, rest) = line.chars().next().map_or((' ', ""), |c| (c, &line[c.len_utf8()..]));
            let ctl = Control::fcfc(c, p.fmt.ovrflw).unwrap_or_else(Control::line);
            p.write_str(rest, ctl);
        } else if *ff && p.line() > 0 {
            p.write_str(
                line,
                Control {
                    skip_before: 1,
                    ..Control::default()
                },
            );
        } else {
            p.write_str(line, Control::line());
        }
    }
    let (pages, recs) = (p.pages, p.records);
    (p.take(), pages, recs, width)
}

/// The spooled files of a job.
#[must_use]
pub fn spooled(job: &Job) -> Vec<Spooled> {
    let cf = &job.control;
    let user = cf.get(b'P').unwrap_or("").to_string();
    let host = cf.get(b'H').unwrap_or("").to_string();
    let sent = cf.job();
    let source = cf.get(b'N').unwrap_or("");
    let prints = cf.prints();
    job.data
        .iter()
        .map(|(name, bytes)| {
            let mine: Vec<u8> = prints.iter().filter(|(_, n)| n == name).map(|(c, _)| *c).collect();
            let letter = mine.first().copied().unwrap_or(b'f');
            let copies = u16::try_from(mine.len().max(1)).unwrap_or(1);
            if let Some(a) = &sent {
                let (pages, _) = spool::scs::decode(bytes);
                return Spooled {
                    attrs: a.clone(),
                    data: bytes.clone(),
                    pages: u32::try_from(pages.len()).unwrap_or(0).max(a.pages),
                    records: 0,
                    user: user.clone(),
                    host: host.clone(),
                };
            }
            let mut attrs = PrintJob {
                file: file_name(source),
                number: 0,
                usrdta: user.chars().take(10).collect(),
                copies,
                prtf: String::new(),
                ..PrintJob::default()
            };
            let (data, pages, records) = match letter {
                b'f' | b'l' | b'p' | b'r' => {
                    let (scs, pages, records, width) = text_to_scs(bytes, letter == b'r');
                    attrs.page_width = width;
                    (scs, pages, records)
                }
                _ => {
                    attrs.devtype = "*USERASCII".into();
                    (bytes.clone(), 1, 0)
                }
            };
            attrs.pages = pages;
            Spooled {
                attrs,
                data,
                pages,
                records,
                user: user.clone(),
                host: host.clone(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use print::sink::lpr::control::{Build, ControlFile};

    #[test]
    fn names() {
        assert_eq!(file_name("/home/anne/report.txt"), "REPORT");
        assert_eq!(file_name("C:\\x\\Quarterly-Sales.ps"), "QUARTERLYS");
        assert_eq!(file_name("123.txt"), DEFAULT_NAME);
        assert_eq!(file_name(""), DEFAULT_NAME);
    }

    #[test]
    fn text_becomes_scs() {
        let (scs, pages, records, width) = text_to_scs(b"one\r\ntwo\tx\n\x0cthree\n", false);
        assert_eq!((pages, records, width), (2, 3, 80));
        let (decoded, _) = spool::scs::decode(&scs);
        let text = spool::view::text(&decoded);
        assert!(text.contains("one") && text.contains("two     x") && text.contains("three"), "{text}");
        let wide = "w".repeat(100);
        assert_eq!(text_to_scs(wide.as_bytes(), false).3, 132);
    }

    fn job(cf: &[u8], letter: u8, data: &[u8]) -> Job {
        let mut cf = ControlFile::parse(cf);
        cf.lines.push((letter, "dfA001h".into()));
        Job {
            queue: "QPRINT".into(),
            control_name: "cfA001h".into(),
            control: cf,
            data: vec![("dfA001h".into(), data.to_vec())],
        }
    }

    #[test]
    fn plain_text_and_binary() {
        let s = spooled(&job(b"Hpc1\nPanne\nNnotes.txt\n", b'f', b"hello\n"));
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].attrs.file.as_str(), s[0].attrs.usrdta.as_str()), ("NOTES", "anne"));
        assert_eq!(s[0].attrs.devtype, "*SCS");
        assert_eq!((s[0].user.as_str(), s[0].host.as_str()), ("anne", "pc1"));
        let s = spooled(&job(b"Hpc1\nPanne\n", b'o', b"%!PS\n"));
        assert_eq!(s[0].attrs.devtype, "*USERASCII");
        assert_eq!(s[0].data, b"%!PS\n");
        assert_eq!(s[0].attrs.file, DEFAULT_NAME);
    }

    #[test]
    fn a_qs400_sender_keeps_its_attributes() {
        let sent = PrintJob {
            file: "PAYROLL".into(),
            copies: 2,
            page_len: 51,
            ..PrintJob::default()
        };
        let cf = ControlFile::build(
            &sent,
            &Build {
                host: "QS400B",
                number: 1,
                banner: false,
                prints: 1,
                attributes: true,
                class: "",
            },
        );
        let scs = text_to_scs(b"pay\n", false).0;
        let j = Job {
            queue: "QPRINT".into(),
            control_name: "cfA001QS400B".into(),
            data: vec![(print::sink::lpr::control::data_name("QS400B", 1), scs.clone())],
            control: cf,
        };
        let s = spooled(&j);
        assert_eq!((s[0].attrs.file.as_str(), s[0].attrs.copies, s[0].attrs.page_len), ("PAYROLL", 2, 51));
        assert_eq!(s[0].data, scs);
    }
}
