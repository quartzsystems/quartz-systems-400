// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The line printer a printer file drives: where the carriage is, page
//! breaks at the page length, the overflow line, spacing and skipping
//! before and after a line, and first-character forms control (FCFC).
//! It writes SCS; the caller takes the bytes as they accumulate.

use crate::scs::Encoder;

/// A printer file's page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFormat {
    /// Lines per page.
    pub len: u16,
    /// Print positions per line.
    pub width: u16,
    /// Lines per inch.
    pub lpi: u8,
    /// Characters per inch.
    pub cpi: u8,
    /// The overflow line.
    pub ovrflw: u16,
    /// Fold lines longer than the width onto the next line rather than
    /// truncate them.
    pub fold: bool,
}

impl Default for PageFormat {
    fn default() -> Self {
        Self {
            len: 66,
            width: 132,
            lpi: 6,
            cpi: 10,
            ovrflw: 60,
            fold: false,
        }
    }
}

/// How a line moves the carriage: `skip` goes to a line (on the next
/// page when it is not below the current one), `space` moves down.
/// Before applies before printing, after once printed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Control {
    /// Skip to this line before; 0 for none.
    pub skip_before: u16,
    /// Space this many lines before.
    pub space_before: u16,
    /// Space this many lines after.
    pub space_after: u16,
    /// Skip to this line after; 0 for none.
    pub skip_after: u16,
}

impl Control {
    /// Space one line before: a plain record.
    #[must_use]
    pub fn line() -> Self {
        Self {
            space_before: 1,
            ..Self::default()
        }
    }

    /// The control an FCFC character asks for, or `None` for an unknown
    /// one. Channel 1 is line 1, channels 2 to 11 lines 2 to 11, channel
    /// 12 the overflow line.
    #[must_use]
    pub fn fcfc(c: char, ovrflw: u16) -> Option<Self> {
        let space = |n| Self {
            space_before: n,
            ..Self::default()
        };
        let skip = |n| Self {
            skip_before: n,
            ..Self::default()
        };
        Some(match c {
            ' ' => space(1),
            '0' => space(2),
            '-' => space(3),
            '+' => space(0),
            '1' => skip(1),
            '2'..='9' => skip(u16::from(c as u8 - b'0')),
            'A' => skip(10),
            'B' => skip(11),
            'C' => skip(ovrflw),
            _ => return None,
        })
    }
}

/// The carriage and the SCS written so far.
#[derive(Clone, Debug)]
pub struct Printer {
    /// The page.
    pub fmt: PageFormat,
    /// The current line; 0 before the first page.
    line: u16,
    /// Something was printed on the current line.
    dirty: bool,
    /// Pages started.
    pub pages: u32,
    /// Lines written.
    pub records: u32,
    enc: Encoder,
}

impl Printer {
    /// A printer at the top of a new file.
    #[must_use]
    pub fn new(fmt: PageFormat) -> Self {
        let mut enc = Encoder::default();
        enc.header(fmt.width, fmt.len, fmt.lpi, fmt.cpi);
        Self {
            fmt,
            line: 0,
            dirty: false,
            pages: 0,
            records: 0,
            enc,
        }
    }

    /// The current line, 0 before the first page.
    #[must_use]
    pub fn line(&self) -> u16 {
        self.line
    }

    /// The SCS written since the last take.
    pub fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.enc.out)
    }

    /// Bytes waiting to be taken.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.enc.out.len()
    }

    fn new_page(&mut self) {
        if self.pages > 0 {
            self.enc.ff();
        }
        self.pages += 1;
        self.line = 1;
        self.dirty = false;
    }

    /// Move down `n` lines, onto a new page past the last line. From
    /// before the first page, the first line counts as one.
    pub fn space(&mut self, n: u16) {
        let mut n = n;
        if self.line == 0 && n > 0 {
            self.new_page();
            n -= 1;
        }
        for _ in 0..n {
            if self.line >= self.fmt.len {
                self.new_page();
            } else {
                self.enc.nl();
                self.line += 1;
                self.dirty = false;
            }
        }
    }

    /// Skip to line `n`: on this page when it is below the carriage,
    /// otherwise on the next.
    pub fn skip(&mut self, n: u16) {
        let n = n.clamp(1, self.fmt.len);
        if self.line == 0 || n <= self.line {
            self.new_page();
        }
        if n != self.line {
            self.enc.line(n);
            self.line = n;
            self.dirty = false;
        }
    }

    /// Print CP037 text at column 1 of the current line, overprinting
    /// what is there.
    fn print(&mut self, text: &[u8]) {
        if self.line == 0 {
            self.new_page();
        }
        if self.dirty {
            self.enc.cr();
        }
        let width = usize::from(self.fmt.width);
        let mut rest = text;
        loop {
            let n = rest.len().min(width);
            self.enc.text(&rest[..n]);
            self.dirty = true;
            rest = &rest[n..];
            if rest.is_empty() || !self.fmt.fold {
                break;
            }
            self.space(1);
        }
    }

    /// Print one line under `ctl`; whether the carriage is now at or
    /// past the overflow line.
    pub fn write(&mut self, text: &[u8], ctl: Control) -> bool {
        if ctl.skip_before > 0 {
            self.skip(ctl.skip_before);
        }
        self.space(ctl.space_before);
        self.print(text);
        self.records += 1;
        self.space(ctl.space_after);
        if ctl.skip_after > 0 {
            self.skip(ctl.skip_after);
        }
        self.line >= self.fmt.ovrflw
    }

    /// A record of a `CTLCHAR(*FCFC)` file: its first character is the
    /// control. An unknown control spaces one line.
    pub fn write_fcfc(&mut self, record: &[u8]) -> bool {
        let (first, text) = record
            .split_first()
            .map_or((' ', &[][..]), |(c, t)| (ccsid::char_of(*c), t));
        let ctl = Control::fcfc(first, self.fmt.ovrflw).unwrap_or_else(Control::line);
        self.write(text, ctl)
    }

    /// Print over the current line without counting a record:
    /// underlining and highlighting.
    pub fn overprint(&mut self, text: &[u8]) {
        self.print(text);
    }

    /// A plain record: one line down, then print.
    pub fn write_line(&mut self, text: &[u8]) -> bool {
        self.write(text, Control::line())
    }

    /// A line of UTF-8 text, for system-produced files (job logs).
    pub fn write_str(&mut self, text: &str, ctl: Control) -> bool {
        let bytes: Vec<u8> = text
            .chars()
            .map(|c| ccsid::byte_of(c).unwrap_or(0x40))
            .collect();
        self.write(&bytes, ctl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scs::decode;

    fn ebc(s: &str) -> Vec<u8> {
        ccsid::to_ebcdic(s).unwrap()
    }

    #[test]
    fn lines_pages_and_overflow() {
        let mut p = Printer::new(PageFormat {
            len: 10,
            ovrflw: 8,
            ..PageFormat::default()
        });
        let mut over = Vec::new();
        for i in 1..=12 {
            over.push(p.write_line(&ebc(&format!("LINE {i}"))));
        }
        assert_eq!(p.pages, 2);
        assert_eq!(p.records, 12);
        assert!(!over[6] && over[7], "line 8 is the overflow line");
        let (pages, fmt) = decode(&p.take());
        assert_eq!((fmt.len, fmt.width, fmt.lpi, fmt.cpi), (10, 132, 6, 10));
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].lines[&1], "LINE 1");
        assert_eq!(pages[0].lines[&10], "LINE 10");
        assert_eq!(pages[1].lines[&2], "LINE 12");
    }

    #[test]
    fn skip_space_overprint_fcfc() {
        let mut p = Printer::new(PageFormat::default());
        p.write(
            &ebc("TITLE"),
            Control {
                skip_before: 3,
                space_after: 2,
                ..Control::default()
            },
        );
        p.write(&ebc("BODY"), Control::default());
        p.write_fcfc(&ebc("+____"));
        p.write_fcfc(&ebc("0NEXT"));
        p.write_fcfc(&ebc("1PAGE TWO"));
        let (pages, _) = decode(&p.take());
        assert_eq!(pages[0].lines[&3], "TITLE");
        assert_eq!(pages[0].lines[&5], "BODY", "overprint keeps the letters");
        assert_eq!(pages[0].lines[&7], "NEXT");
        assert_eq!(pages[1].lines[&1], "PAGE TWO");
    }

    #[test]
    fn fold_and_transparent_controls() {
        let mut p = Printer::new(PageFormat {
            width: 5,
            fold: true,
            ..PageFormat::default()
        });
        p.write_line(&ebc("ABCDEFG"));
        let mut raw = ebc("X");
        raw.push(0x15);
        raw.extend(ebc("Y"));
        p.write_line(&raw);
        let (pages, _) = decode(&p.take());
        assert_eq!(pages[0].lines[&1], "ABCDE");
        assert_eq!(pages[0].lines[&2], "FG");
        assert_eq!(
            pages[0].lines[&3], "X Y",
            "a control byte prints as a blank"
        );
    }
}
