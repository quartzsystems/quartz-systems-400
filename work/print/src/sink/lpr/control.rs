// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The LPR control file (RFC 1179 section 7): one command per line, a
//! letter and its operand. What is sent: `H` host, `P` user, `J` job
//! name, `C` class, `L` banner (with `SEPPAGE(*YES)`), `N` source
//! name, the print commands (`l`, the data printed as it is, once per
//! copy with `XAIX`), `U` unlink. For `DESTTYPE(*OS400)` the spooled
//! file's attributes follow as `Y` lines, `YQS400=1` first, then
//! `YKEY=value` with `%XX` escapes, so a receiving QS/400 recreates the
//! file (`docs/PRINT.md`). Other LPDs ignore the letter.

use std::fmt::Write as _;

use crate::job::{Duplex, PrintJob};

/// The extension's version line.
pub const VERSION_LINE: &str = "QS400=1";

/// The command letter of the extension.
pub const EXT: u8 = b'Y';

/// A control file as lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ControlFile {
    /// `(command, operand)` in order.
    pub lines: Vec<(u8, String)>,
}

/// `%XX` for `%`, controls, DEL and every non-ASCII byte.
#[must_use]
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b == b'%' || !(0x20..0x7F).contains(&b) {
            let _ = write!(out, "%{b:02X}");
        } else {
            out.push(char::from(b));
        }
    }
    out
}

/// The inverse of [`escape`]; a malformed escape stays as it is.
#[must_use]
pub fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// An operand: one line of printable ASCII, at most `max` long (RFC
/// 1179 allows 31 for `C`, `H`, `L`, `P` and 99 for `J`, `N`).
fn operand(s: &str, max: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .map(|c| if c.is_ascii() { c } else { '?' })
        .take(max)
        .collect()
}

/// The attributes as `(KEY, value)`, in the order sent.
#[must_use]
pub fn attributes(job: &PrintJob) -> Vec<(&'static str, String)> {
    vec![
        ("FILE", job.file.clone()),
        ("SPLNBR", job.number.to_string()),
        ("JOB", job.job.0.clone()),
        ("USER", job.job.1.clone()),
        ("JOBNBR", format!("{:06}", job.job.2)),
        ("SYSTEM", job.system.clone()),
        ("USRDTA", job.usrdta.clone()),
        ("FORMTYPE", job.formtype.clone()),
        ("COPIES", job.copies.to_string()),
        ("PAGES", job.pages.to_string()),
        ("PAGELEN", job.page_len.to_string()),
        ("PAGEWIDTH", job.page_width.to_string()),
        ("LPI", job.lpi.to_string()),
        ("CPI", job.cpi.to_string()),
        ("OVRFLW", job.ovrflw.to_string()),
        ("DRAWER", job.drawer.to_string()),
        ("DUPLEX", job.duplex.name().into()),
        ("OUTPTY", job.outpty.to_string()),
        ("FONT", job.font.clone()),
        ("CHRID", job.chrid.clone()),
        ("PRTQLTY", job.prtqlty.clone()),
        ("ALIGN", yes(job.align)),
        ("DEVTYPE", job.devtype.clone()),
        ("PRTF", job.prtf.clone()),
        ("USRDFNDTA", job.usrdfndta.clone()),
        ("SAVE", yes(job.save)),
    ]
}

fn yes(b: bool) -> String {
    if b { "*YES" } else { "*NO" }.into()
}

/// How a control file is put together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Build<'a> {
    /// This system's host name (`H`, and the file names).
    pub host: &'a str,
    /// The three-digit job number.
    pub number: u16,
    /// `L`: print a banner page.
    pub banner: bool,
    /// Print commands per data file: the copies with `XAIX`, else 1.
    pub prints: u16,
    /// Carry the attributes (`DESTTYPE(*OS400)`).
    pub attributes: bool,
    /// `C`: the class.
    pub class: &'a str,
}

/// The data file's name: `dfA`, the number, the host.
#[must_use]
pub fn data_name(host: &str, number: u16) -> String {
    format!("dfA{:03}{}", number % 1000, operand(host, 31))
}

/// The control file's name: `cfA`, the number, the host.
#[must_use]
pub fn control_name(host: &str, number: u16) -> String {
    format!("cfA{:03}{}", number % 1000, operand(host, 31))
}

impl ControlFile {
    /// The control file for `job`.
    #[must_use]
    pub fn build(job: &PrintJob, b: &Build<'_>) -> Self {
        let mut lines = vec![
            (b'H', operand(b.host, 31)),
            (b'P', operand(&job.job.1, 31)),
            (b'J', operand(&job.title(), 99)),
        ];
        if !b.class.is_empty() {
            lines.push((b'C', operand(b.class, 31)));
        }
        if b.banner {
            lines.push((b'L', operand(&job.job.1, 31)));
        }
        if b.attributes {
            lines.push((EXT, VERSION_LINE.into()));
            for (k, v) in attributes(job) {
                lines.push((EXT, format!("{k}={}", escape(&v))));
            }
        }
        let df = data_name(b.host, b.number);
        for _ in 0..b.prints.max(1) {
            lines.push((b'l', df.clone()));
        }
        lines.push((b'U', df.clone()));
        lines.push((b'N', operand(&job.file, 99)));
        Self { lines }
    }

    /// The bytes: each line and LF.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for (c, s) in &self.lines {
            out.push(*c);
            out.extend_from_slice(s.as_bytes());
            out.push(b'\n');
        }
        out
    }

    /// Parse received bytes (an LPD's side); empty lines are skipped.
    #[must_use]
    pub fn parse(b: &[u8]) -> Self {
        let lines = b
            .split(|c| *c == b'\n')
            .filter(|l| !l.is_empty())
            .map(|l| {
                let l = l.strip_suffix(b"\r").unwrap_or(l);
                (l[0], String::from_utf8_lossy(&l[1..]).to_string())
            })
            .collect();
        Self { lines }
    }

    /// The operand of the first `c` line.
    #[must_use]
    pub fn get(&self, c: u8) -> Option<&str> {
        self.lines.iter().find(|(k, _)| *k == c).map(|(_, v)| v.as_str())
    }

    /// The print commands: `(letter, data file name)`.
    #[must_use]
    pub fn prints(&self) -> Vec<(u8, &str)> {
        self.lines
            .iter()
            .filter(|(k, _)| k.is_ascii_lowercase())
            .map(|(k, v)| (*k, v.as_str()))
            .collect()
    }

    /// The extension's `(KEY, value)` pairs, unescaped; `None` when the
    /// file has no `YQS400=1` line.
    #[must_use]
    pub fn extension(&self) -> Option<Vec<(String, String)>> {
        let mut ext = self.lines.iter().filter(|(k, _)| *k == EXT).map(|(_, v)| v.as_str());
        if ext.next()? != VERSION_LINE {
            return None;
        }
        Some(
            ext.filter_map(|l| l.split_once('='))
                .map(|(k, v)| (k.to_string(), unescape(v)))
                .collect(),
        )
    }

    /// The spooled file the extension describes, over the defaults for
    /// what it leaves out; `None` without one.
    #[must_use]
    pub fn job(&self) -> Option<PrintJob> {
        let ext = self.extension()?;
        let mut j = PrintJob::default();
        for (k, v) in ext {
            let n = |d: u32| v.trim().parse::<u32>().unwrap_or(d);
            let n8 = |d: u8| v.trim().parse::<u8>().unwrap_or(d);
            let n16 = |d: u16| v.trim().parse::<u16>().unwrap_or(d);
            match k.as_str() {
                "FILE" => j.file = v,
                "SPLNBR" => j.number = n(j.number),
                "JOB" => j.job.0 = v,
                "USER" => j.job.1 = v,
                "JOBNBR" => j.job.2 = n(j.job.2),
                "SYSTEM" => j.system = v,
                "USRDTA" => j.usrdta = v,
                "FORMTYPE" => j.formtype = v,
                "COPIES" => j.copies = n16(j.copies),
                "PAGES" => j.pages = n(j.pages),
                "PAGELEN" => j.page_len = n16(j.page_len),
                "PAGEWIDTH" => j.page_width = n16(j.page_width),
                "LPI" => j.lpi = n8(j.lpi),
                "CPI" => j.cpi = n8(j.cpi),
                "OVRFLW" => j.ovrflw = n16(j.ovrflw),
                "DRAWER" => j.drawer = n8(j.drawer),
                "DUPLEX" => j.duplex = Duplex::parse(&v).unwrap_or_default(),
                "OUTPTY" => j.outpty = n8(j.outpty),
                "FONT" => j.font = v,
                "CHRID" => j.chrid = v,
                "PRTQLTY" => j.prtqlty = v,
                "ALIGN" => j.align = v == "*YES",
                "DEVTYPE" => j.devtype = v,
                "PRTF" => j.prtf = v,
                "USRDFNDTA" => j.usrdfndta = v,
                "SAVE" => j.save = v == "*YES",
                _ => {}
            }
        }
        Some(j)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PrintJob {
        PrintJob {
            file: "QPRINT".into(),
            number: 4,
            job: ("PAYROLL".into(), "ANNE".into(), 123_456),
            usrdta: "100% done".into(),
            usrdfndta: "line one\nline two \u{e9}".into(),
            copies: 2,
            duplex: Duplex::Yes,
            save: true,
            ..PrintJob::default()
        }
    }

    #[test]
    fn builds_the_lines() {
        let cf = ControlFile::build(
            &sample(),
            &Build {
                host: "QS400A",
                number: 456,
                banner: true,
                prints: 2,
                attributes: false,
                class: "",
            },
        );
        let text = String::from_utf8(cf.encode()).unwrap();
        assert_eq!(
            text,
            "HQS400A\nPANNE\nJQPRINT.123456.4\nLANNE\nldfA456QS400A\nldfA456QS400A\nUdfA456QS400A\nNQPRINT\n"
        );
        assert!(cf.extension().is_none());
        assert_eq!(data_name("h", 1234), "dfA234h");
        assert_eq!(control_name("h", 7), "cfA007h");
    }

    #[test]
    fn attributes_round_trip() {
        let job = sample();
        let cf = ControlFile::build(
            &job,
            &Build {
                host: "QS400A",
                number: 1,
                banner: false,
                prints: 1,
                attributes: true,
                class: "A",
            },
        );
        let bytes = cf.encode();
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.contains("YQS400=1\nYFILE=QPRINT\n"));
        assert!(text.contains("YUSRDTA=100%25 done\n"));
        assert!(text.contains("YUSRDFNDTA=line one%0Aline two %C3%A9\n"));
        let back = ControlFile::parse(&bytes);
        assert_eq!(back, cf);
        assert_eq!(back.job().unwrap(), job);
        assert_eq!(back.get(b'C'), Some("A"));
        assert_eq!(back.prints(), vec![(b'l', "dfA001QS400A")]);
        assert_eq!(unescape("50%"), "50%");
    }
}
