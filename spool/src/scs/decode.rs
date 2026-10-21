// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading SCS back into pages of lines. Overprinted characters keep
//! the first non-blank one; unknown format commands are skipped by
//! their length.

use std::collections::BTreeMap;

use super::{
    AHPP, AVPP, BEL, BS, CR, CSP, FF, IRS, LF, NL, PAGE, PP, RHPP, RVPP, SCD, SHF, SLD, SVF,
    TRN,
};

/// One page: its printed lines by line number (1-based).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    /// Line number to text, right-trimmed.
    pub lines: BTreeMap<u16, String>,
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

struct State {
    pages: Vec<BTreeMap<u16, Vec<char>>>,
    line: u16,
    col: u16,
    fmt: Format,
}

impl State {
    fn put(&mut self, ch: char) {
        if self.pages.is_empty() {
            self.pages.push(BTreeMap::new());
        }
        let col = usize::from(self.col.max(1) - 1);
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
        for _ in 0..n {
            if self.line >= self.fmt.len {
                self.new_page();
            } else {
                self.line += 1;
            }
        }
    }

    fn new_page(&mut self) {
        self.pages.push(BTreeMap::new());
        self.line = 1;
        self.col = 1;
    }
}

/// The pages of `data` and the format it set.
#[must_use]
pub fn decode(data: &[u8]) -> (Vec<Page>, Format) {
    let mut st = State {
        pages: Vec::new(),
        line: 1,
        col: 1,
        fmt: Format::default(),
    };
    let mut i = 0;
    let at = |i: usize| data.get(i).copied().unwrap_or(0);
    while i < data.len() {
        let b = data[i];
        i += 1;
        match b {
            FF => st.new_page(),
            CR => st.col = 1,
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
            }
            b if b >= 0x40 => st.put(ccsid::char_of(b)),
            _ => {}
        }
    }
    let pages = st
        .pages
        .into_iter()
        .map(|p| Page {
            lines: p
                .into_iter()
                .map(|(n, chars)| (n, chars.into_iter().collect::<String>().trim_end().to_string()))
                .collect(),
        })
        .collect();
    (pages, st.fmt)
}
