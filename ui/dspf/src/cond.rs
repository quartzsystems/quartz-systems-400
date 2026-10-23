// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Option indicator conditioning, positions 7-16: up to three
//! indicators per line, each `N` or blank then `01`-`99`, all of which
//! must hold; a line with `A` in position 7 adds more indicators to the
//! group and one with `O` starts another group, any of which may hold. A condition group
//! runs from a line with position 7 blank over the `A`/`O` lines that
//! follow it; the field or keyword on any line of the group is
//! conditioned by the whole group. `*DSn` in positions 8-16 conditions
//! on a display size instead.

use std::fmt;

use dds::Source;
use dds::diag::Diagnostics;

use crate::diag::ids;

/// One indicator of a condition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Ind {
    /// 1-99.
    pub number: u8,
    /// False for `N`: the indicator must be off.
    pub on: bool,
}

impl fmt::Display for Ind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.on {
            f.write_str("N")?;
        }
        write!(f, "{:02}", self.number)
    }
}

/// When a field or keyword is in effect.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Cond {
    /// `*DSn`: the index of the display size in `DSPSIZ`.
    pub size: Option<u8>,
    /// Groups of which any may hold, each holding when all its
    /// indicators do; empty means always.
    pub any: Vec<Vec<Ind>>,
}

impl Cond {
    /// No conditioning at all.
    #[must_use]
    pub fn always() -> Self {
        Self::default()
    }

    /// A single indicator.
    #[must_use]
    pub fn ind(number: u8, on: bool) -> Self {
        Self {
            size: None,
            any: vec![vec![Ind { number, on }]],
        }
    }

    /// Whether nothing conditions it.
    #[must_use]
    pub fn is_always(&self) -> bool {
        self.size.is_none() && self.any.is_empty()
    }

    /// Every indicator number used, in order of appearance.
    pub fn indicators(&self) -> impl Iterator<Item = u8> + '_ {
        self.any.iter().flatten().map(|i| i.number)
    }
}

impl fmt::Display for Cond {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::new();
        if let Some(s) = self.size {
            parts.push(format!("*DS{}", s + 3));
        }
        let groups: Vec<String> = self
            .any
            .iter()
            .map(|g| {
                g.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        if !groups.is_empty() {
            parts.push(groups.join(" O "));
        }
        f.write_str(&parts.join(" "))
    }
}

/// The source lines of one specification: its first line and the last
/// line before the next specification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// First line index.
    pub first: usize,
    /// Last line index, inclusive.
    pub last: usize,
}

/// Position 7.
fn joiner(source: &Source, line: usize) -> char {
    source.lines()[line].ch(7)
}

/// Whether the line takes part in conditioning at all.
fn is_code(source: &Source, line: usize) -> bool {
    let l = &source.lines()[line];
    !l.blank(1, dds::RECORD_LEN) && l.ch(7) != '*'
}

/// The condition in effect on `at`, a line of `entry`. `sizes` are the
/// `*DSn` names of `DSPSIZ` in order.
pub fn parse(
    source: &Source,
    entry: Entry,
    at: usize,
    sizes: &[String],
    diags: &mut Diagnostics,
) -> Cond {
    // Back to the line that starts the group.
    let mut start = at;
    while start > entry.first && matches!(joiner(source, start), 'A' | 'O') {
        let mut prev = start - 1;
        while prev > entry.first && !is_code(source, prev) {
            prev -= 1;
        }
        if !is_code(source, prev) {
            break;
        }
        start = prev;
    }
    let mut cond = Cond::always();
    let mut group: Vec<Ind> = Vec::new();
    let mut line = start;
    let mut first = true;
    while line <= entry.last {
        if !is_code(source, line) {
            line += 1;
            continue;
        }
        let j = joiner(source, line);
        match j {
            ' ' if first => {}
            'A' | 'O' if first => diags.push(ids::continuation_without_start(line)),
            ' ' => break,
            'A' => {}
            'O' => {
                if !group.is_empty() {
                    cond.any.push(std::mem::take(&mut group));
                }
            }
            other => diags.push(ids::bad_conditioning(line, &other.to_string())),
        }
        first = false;
        let text = source.lines()[line].cols(8, 16);
        if text.starts_with('*') {
            match sizes.iter().position(|s| *s == text.trim()) {
                Some(i) => cond.size = Some(u8::try_from(i).unwrap_or(0)),
                None => diags.push(ids::unknown_size_condition(line, text.trim())),
            }
        } else {
            slots(&text, line, &mut group, diags);
        }
        line += 1;
    }
    if !group.is_empty() {
        cond.any.push(group);
    }
    cond
}

/// Positions 8-16 as three slots of `N`/blank and two digits.
fn slots(text: &str, line: usize, group: &mut Vec<Ind>, diags: &mut Diagnostics) {
    let chars: Vec<char> = text.chars().collect();
    for slot in chars.chunks(3) {
        let s: String = slot.iter().collect();
        if s.trim().is_empty() {
            continue;
        }
        let neg = match slot.first() {
            Some('N') => true,
            Some(' ') => false,
            _ => {
                diags.push(ids::bad_conditioning(line, text));
                return;
            }
        };
        let digits: String = slot.iter().skip(1).collect();
        match digits.parse::<u8>() {
            Ok(n) if (1..=99).contains(&n) => group.push(Ind {
                number: n,
                on: !neg,
            }),
            Ok(n) => diags.push(ids::indicator_range(line, n)),
            Err(_) => diags.push(ids::bad_conditioning(line, text)),
        }
    }
}

/// The `*DSn` name a specification line carries in positions 8-16, if
/// that is what it carries: such a line with nothing but a location is
/// an alternate location of the field before it.
#[must_use]
pub fn size_name(conditioning: &str) -> Option<&str> {
    conditioning.strip_prefix('*').map(|_| conditioning)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(lines: &[&str]) -> Source {
        let text: Vec<String> = lines.iter().map(|l| format!("     A{l}")).collect();
        Source::from_text(&text.join("\n")).0
    }

    fn names() -> Vec<String> {
        vec!["*DS3".into(), "*DS4".into()]
    }

    #[test]
    fn and_or_groups() {
        // Position 7 then 8-16, then the rest of the line.
        let s = src(&[
            "  50N51                         X",
            "A 52                            X",
            "O 53                            KW",
            "                                KW2",
        ]);
        let mut d = Diagnostics::default();
        let e = Entry { first: 0, last: 3 };
        let c = parse(&s, e, 2, &names(), &mut d);
        assert!(d.as_slice().is_empty(), "{:?}", d.as_slice());
        assert_eq!(c.to_string(), "50 N51 52 O 53");
        assert_eq!(c.indicators().collect::<Vec<_>>(), [50, 51, 52, 53]);
        let c0 = parse(&s, e, 0, &names(), &mut d);
        assert_eq!(c0, c);
        let c3 = parse(&s, e, 3, &names(), &mut d);
        assert!(c3.is_always());
    }

    #[test]
    fn size_and_errors() {
        let s = src(&[
            " *DS4                           X",
            "  5X                            X",
            "  00                            X",
            "A 10                            X",
            " *DS9                           X",
        ]);
        let mut d = Diagnostics::default();
        let c = parse(&s, Entry { first: 0, last: 0 }, 0, &names(), &mut d);
        assert_eq!(c.size, Some(1));
        assert_eq!(c.to_string(), "*DS4");
        parse(&s, Entry { first: 1, last: 1 }, 1, &names(), &mut d);
        parse(&s, Entry { first: 2, last: 2 }, 2, &names(), &mut d);
        parse(&s, Entry { first: 3, last: 3 }, 3, &names(), &mut d);
        parse(&s, Entry { first: 4, last: 4 }, 4, &names(), &mut d);
        let ids: Vec<_> = d.as_slice().iter().map(|d| d.id).collect();
        assert_eq!(ids, ["DDS6101", "DDS6102", "DDS6109", "DDS6103"]);
    }

    #[test]
    fn display() {
        assert_eq!(Cond::ind(7, false).to_string(), "N07");
        assert_eq!(Cond::always().to_string(), "");
        assert_eq!(size_name("*DS4"), Some("*DS4"));
        assert_eq!(size_name(" 50"), None);
    }
}
