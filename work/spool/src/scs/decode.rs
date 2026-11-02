// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading SCS back into pages of lines. Overprinted characters keep
//! the first non-blank one; unknown format commands are skipped by
//! their length. The same pass finds where each page starts ([`index`]),
//! so a page range of the data stream reads as it does in the whole.

use std::collections::BTreeMap;

use super::{
    AHPP, AVPP, BEL, BS, CR, CSP, FF, IRS, LF, NL, PAGE, PP, RHPP, RVPP, SCD, SHF, SLD, SVF, TRN,
};

/// One page: its printed lines by line number (1-based).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    /// Line number to text, right-trimmed.
    pub lines: BTreeMap<u16, String>,
    /// Each line's passes as printed: a carriage return starts another
    /// over the same line (what `CPYSPLF CTLCHAR(*FCFC)` keeps apart).
    pub passes: BTreeMap<u16, Vec<String>>,
}

impl Page {
    /// The last line with anything on it.
    #[must_use]
    pub fn last_line(&self) -> u16 {
        self.lines.keys().next_back().copied().unwrap_or(0)
    }
}

/// The page format the data asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Format {
    /// Maximum presentation position.
    pub width: u16,
    /// Maximum presentation line.
    pub len: u16,
    /// Lines per inch.
    pub lpi: u8,
    /// Characters per inch.
    pub cpi: u8,
}

impl Default for Format {
    fn default() -> Self {
        Self {
            width: 132,
            len: 66,
            lpi: 6,
            cpi: 10,
        }
    }
}

/// Where a page starts in the data stream, and what a reader starting
/// there needs first to be where the whole stream is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PageStart {
    /// The offset of the command that starts the page; 0 for page 1.
    pub offset: usize,
    /// The format commands in effect there (page format, line density,
    /// pitch, fonts), the last of each kind, as they were sent.
    pub formats: Vec<u8>,
    /// The line before that command.
    pub line: u16,
    /// The column before it.
    pub col: u16,
}

impl PageStart {
    /// The SCS a range starting here begins with: the format commands,
    /// then the position, so the range reads as it does in the whole
    /// stream. Nothing for page 1.
    #[must_use]
    pub fn prefix(&self) -> Vec<u8> {
        if self.offset == 0 {
            return Vec::new();
        }
        let byte = |n: u16| u8::try_from(n.clamp(1, 255)).unwrap_or(255);
        let mut out = self.formats.clone();
        out.extend_from_slice(&[PP, AVPP, byte(self.line)]);
        if self.col > 1 {
            out.extend_from_slice(&[PP, AHPP, byte(self.col)]);
        }
        out
    }
}

struct State {
    pages: Vec<BTreeMap<u16, Vec<char>>>,
    passes: Vec<BTreeMap<u16, Vec<Vec<char>>>>,
    line: u16,
    col: u16,
    fmt: Format,
    /// A carriage return since the last character: the next one starts a pass.
    cr: bool,
    /// Where each page starts.
    starts: Vec<PageStart>,
    /// The command being read: its offset, and the line and column before it.
    at: (usize, u16, u16),
    /// The last format command of each kind, oldest first.
    formats: Vec<((u8, u8), Vec<u8>)>,
}

impl State {
    /// A page starts with the command being read.
    fn mark(&mut self) {
        let first = self.pages.is_empty();
        let (offset, line, col) = self.at;
        self.starts.push(PageStart {
            offset: if first { 0 } else { offset },
            formats: if first {
                Vec::new()
            } else {
                self.formats.iter().flat_map(|(_, b)| b.iter().copied()).collect()
            },
            line,
            col,
        });
    }

    /// Format command `bytes` of class `class`: the last of its kind.
    fn remember(&mut self, class: u8, sub: u8, bytes: &[u8]) {
        let key = (class, if (0xD1..=0xD3).contains(&class) { sub } else { 0 });
        self.formats.retain(|(k, _)| *k != key);
        self.formats.push((key, bytes.to_vec()));
    }

    fn put(&mut self, ch: char) {
        if self.pages.is_empty() {
            self.mark();
            self.pages.push(BTreeMap::new());
            self.passes.push(BTreeMap::new());
        }
        let col = usize::from(self.col.max(1) - 1);
        let passes = self.passes.last_mut().expect("a page").entry(self.line.max(1)).or_default();
        if passes.is_empty() || self.cr && ch != ' ' {
            passes.push(Vec::new());
        }
        if ch != ' ' {
            self.cr = false;
        }
        let pass = passes.last_mut().expect("a pass");
        if pass.len() <= col {
            pass.resize(col + 1, ' ');
        }
        if pass[col] == ' ' {
            pass[col] = ch;
        }
        let page = self.pages.last_mut().expect("a page");
        let row = page.entry(self.line.max(1)).or_default();
        if row.len() <= col {
            row.resize(col + 1, ' ');
        }
        if row[col] == ' ' {
            row[col] = ch;
        }
        self.col += 1;
    }

    fn down(&mut self, n: u16) {
        if n > 0 {
            self.cr = false;
        }
        for _ in 0..n {
            if self.line >= self.fmt.len {
                self.new_page();
            } else {
                self.line += 1;
            }
        }
    }

    fn new_page(&mut self) {
        self.mark();
        self.cr = false;
        self.pages.push(BTreeMap::new());
        self.passes.push(BTreeMap::new());
        self.line = 1;
        self.col = 1;
    }
}

/// Read all of `data`.
fn scan(data: &[u8]) -> State {
    let mut st = State {
        pages: Vec::new(),
        passes: Vec::new(),
        line: 1,
        col: 1,
        fmt: Format::default(),
        cr: false,
        starts: Vec::new(),
        at: (0, 1, 1),
        formats: Vec::new(),
    };
    let mut i = 0;
    let at = |i: usize| data.get(i).copied().unwrap_or(0);
    while i < data.len() {
        st.at = (i, st.line, st.col);
        let b = data[i];
        i += 1;
        match b {
            FF => st.new_page(),
            CR => {
                st.col = 1;
                st.cr = true;
            }
            NL | IRS => {
                st.down(1);
                st.col = 1;
            }
            LF => st.down(1),
            BS => st.col = st.col.saturating_sub(1).max(1),
            BEL => {}
            PP => {
                let (sub, n) = (at(i), u16::from(at(i + 1)));
                i += 2;
                match sub {
                    AHPP => st.col = n.max(1),
                    RHPP => st.col += n,
                    AVPP => {
                        if n < st.line {
                            st.new_page();
                        }
                        st.line = n.max(1);
                        st.col = 1;
                    }
                    RVPP => st.down(n),
                    _ => {}
                }
            }
            TRN => {
                let n = usize::from(at(i));
                i += 1;
                for &c in data.get(i..i + n).unwrap_or(&[]) {
                    st.put(ccsid::char_of(c));
                }
                i += n;
            }
            CSP => {
                let start = i - 1;
                let (class, len) = (at(i), usize::from(at(i + 1)));
                let params = data.get(i + 2..i + 1 + len).unwrap_or(&[]);
                let p = |k: usize| params.get(k).copied().unwrap_or(0);
                match class {
                    SHF if !params.is_empty() => st.fmt.width = u16::from(p(0)),
                    SVF if !params.is_empty() => st.fmt.len = u16::from(p(0)).max(1),
                    SLD if !params.is_empty() => st.fmt.lpi = 72 / p(0).max(1),
                    PAGE if p(0) == SCD => st.fmt.cpi = p(2),
                    _ => {}
                }
                i += 1 + len.max(1);
                let end = i.min(data.len());
                st.remember(class, p(0), &data[start..end]);
            }
            b if b >= 0x40 => st.put(ccsid::char_of(b)),
            _ => {}
        }
    }
    st
}

/// The pages of `data` and the format it set.
#[must_use]
pub fn decode(data: &[u8]) -> (Vec<Page>, Format) {
    let st = scan(data);
    let text = |chars: Vec<char>| chars.into_iter().collect::<String>().trim_end().to_string();
    let pages = st
        .pages
        .into_iter()
        .zip(st.passes)
        .map(|(p, passes)| Page {
            lines: p.into_iter().map(|(n, chars)| (n, text(chars))).collect(),
            passes: passes.into_iter().map(|(n, v)| (n, v.into_iter().map(text).collect())).collect(),
        })
        .collect();
    (pages, st.fmt)
}

/// Where each page of `data` starts, one entry per page [`decode`]
/// finds.
#[must_use]
pub fn index(data: &[u8]) -> Vec<PageStart> {
    scan(data).starts
}

/// Pages `first` to `last` (1-based, inclusive) of `data` as SCS: the
/// state in effect at `first` ([`PageStart::prefix`]), then the data
/// stream from the start of `first` to the start of the page after
/// `last`, as it was. `None` when `first` is not a page or `last` is
/// before it; a `last` past the end reads to the end.
#[must_use]
pub fn range(data: &[u8], first: usize, last: usize) -> Option<Vec<u8>> {
    let idx = index(data);
    let from = idx.get(first.checked_sub(1)?)?;
    if last < first {
        return None;
    }
    let to = idx.get(last).map_or(data.len(), |p| p.offset);
    let mut out = from.prefix();
    out.extend_from_slice(&data[from.offset..to]);
    Some(out)
}
