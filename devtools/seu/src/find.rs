// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Find and change: the options of the Find/Change display (`F14`) and
//! the search over the records, by column range, case, the excluded
//! records and the records' dates.

use std::collections::HashSet;

use crate::buffer::Buffer;

/// Occurrences to process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// 1: the next one after the cursor.
    Next,
    /// 2: all of them.
    All,
    /// 3: the one before the cursor.
    Previous,
    /// 4: the first in the member.
    First,
    /// 5: the last in the member.
    Last,
}

/// Records to search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// 1: all.
    All,
    /// 2: only the excluded ones.
    Excluded,
    /// 3: only the shown ones.
    NotExcluded,
}

/// The date comparison of `Search for date`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DateCmp {
    /// 1: before the date.
    Less,
    /// 2: on the date.
    Equal,
    /// 3: after it.
    Greater,
}

/// The find and change options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindOptions {
    /// The string to find.
    pub find: String,
    /// What it changes to.
    pub change: String,
    /// From column, 1-based.
    pub from: usize,
    /// To column; `None` for the record's end.
    pub to: Option<usize>,
    /// Occurrences.
    pub dir: Direction,
    /// Records.
    pub scope: Scope,
    /// Same case (`true`) or ignore case.
    pub same_case: bool,
    /// A change may move the data after it.
    pub shift: bool,
    /// Records by date.
    pub date: Option<(u32, DateCmp)>,
}

impl FindOptions {
    /// The defaults for records of `width` columns.
    #[must_use]
    pub fn new() -> Self {
        Self {
            find: String::new(),
            change: String::new(),
            from: 1,
            to: None,
            dir: Direction::Next,
            scope: Scope::All,
            same_case: false,
            shift: true,
            date: None,
        }
    }
}

impl Default for FindOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// A match: record position and 1-based column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hit {
    /// The record's position.
    pub rec: usize,
    /// The column where the string starts.
    pub col: usize,
}

fn fold(s: &str, same: bool) -> Vec<char> {
    if same {
        s.chars().collect()
    } else {
        s.chars().map(|c| c.to_ascii_uppercase()).collect()
    }
}

/// The columns of `data` where `what` starts, within `from..=to`.
fn hits_in(data: &str, what: &[char], o: &FindOptions, width: usize) -> Vec<usize> {
    let d = fold(data, o.same_case);
    let mut padded = d.clone();
    padded.resize(width.max(d.len()), ' ');
    let to = o.to.unwrap_or(width).min(padded.len());
    let from = o.from.max(1);
    let mut out = Vec::new();
    if what.is_empty() || from > to {
        return out;
    }
    let mut c = from;
    while c + what.len() - 1 <= to {
        if padded[c - 1..c - 1 + what.len()] == *what {
            out.push(c);
        }
        c += 1;
    }
    out
}

#[allow(clippy::implicit_hasher)]
fn record_ok(b: &Buffer, i: usize, o: &FindOptions, excluded: &HashSet<u64>) -> bool {
    let Some(r) = b.get(i) else {
        return false;
    };
    let x = excluded.contains(&r.id);
    let scope = match o.scope {
        Scope::All => true,
        Scope::Excluded => x,
        Scope::NotExcluded => !x,
    };
    let date = match o.date {
        None => true,
        Some((d, DateCmp::Less)) => r.date < d,
        Some((d, DateCmp::Equal)) => r.date == d,
        Some((d, DateCmp::Greater)) => r.date > d,
    };
    scope && date
}

/// Every match in the member, in order.
#[must_use]
#[allow(clippy::implicit_hasher)]
pub fn find_all(b: &Buffer, o: &FindOptions, excluded: &HashSet<u64>) -> Vec<Hit> {
    let what = fold(&o.find, o.same_case);
    let mut out = Vec::new();
    for i in 0..b.len() {
        if !record_ok(b, i, o, excluded) {
            continue;
        }
        for col in hits_in(&b.records()[i].data, &what, o, b.width) {
            out.push(Hit { rec: i, col });
        }
    }
    out
}

/// The match the options ask for, from a position (`None`: before the
/// first record). `All` gives the first.
#[must_use]
#[allow(clippy::implicit_hasher)]
pub fn find(b: &Buffer, o: &FindOptions, excluded: &HashSet<u64>, from: Option<Hit>) -> Option<Hit> {
    let all = find_all(b, o, excluded);
    match o.dir {
        Direction::First | Direction::All => all.first().copied(),
        Direction::Last => all.last().copied(),
        Direction::Next => all
            .iter()
            .find(|h| from.is_none_or(|f| (h.rec, h.col) > (f.rec, f.col)))
            .copied(),
        Direction::Previous => all
            .iter()
            .rev()
            .find(|h| from.is_some_and(|f| (h.rec, h.col) < (f.rec, f.col)))
            .copied(),
    }
}

/// Change the string at a hit; whether the record changed (a change
/// that would push data past the record's end is not made).
pub fn change_at(b: &mut Buffer, o: &FindOptions, hit: Hit, date: u32) -> bool {
    let Some(r) = b.get(hit.rec) else {
        return false;
    };
    let mut chars: Vec<char> = r.data.chars().collect();
    let width = b.width;
    chars.resize(width.max(chars.len()), ' ');
    let old_len = o.find.chars().count();
    let new: Vec<char> = o.change.chars().collect();
    let at = hit.col - 1;
    if at + old_len > chars.len() {
        return false;
    }
    let mut out: Vec<char> = chars[..at].to_vec();
    if o.shift {
        out.extend(&new);
        out.extend(&chars[at + old_len..]);
        // Data pushed past the end must be blanks.
        if out.len() > width && out[width..].iter().any(|c| *c != ' ') {
            return false;
        }
    } else {
        // No shift: the new string takes the old one's columns and
        // overlays what follows when longer.
        let mut tail: Vec<char> = chars[at..].to_vec();
        for (k, c) in new.iter().enumerate() {
            if k < tail.len() {
                tail[k] = *c;
            }
        }
        for slot in tail.iter_mut().take(old_len).skip(new.len()) {
            *slot = ' ';
        }
        out.extend(tail);
    }
    out.truncate(width);
    let text: String = out.into_iter().collect();
    b.set_data(hit.rec, &text, date)
}

/// Change every match (from the first, or the one asked for); the
/// number of records changed.
#[allow(clippy::implicit_hasher)]
pub fn change_all(b: &mut Buffer, o: &FindOptions, excluded: &HashSet<u64>, date: u32) -> usize {
    let mut changed = 0;
    for i in 0..b.len() {
        if !record_ok(b, i, o, excluded) {
            continue;
        }
        let what = fold(&o.find, o.same_case);
        let mut any = false;
        // Columns move as earlier changes shift the data: search again
        // after each one.
        let mut start = o.from.max(1);
        loop {
            let mut oo = o.clone();
            oo.from = start;
            let cols = hits_in(&b.records()[i].data, &what, &oo, b.width);
            let Some(&c) = cols.first() else {
                break;
            };
            if !change_at(b, o, Hit { rec: i, col: c }, date) {
                break;
            }
            any = true;
            start = c + o.change.chars().count().max(1);
        }
        if any {
            changed += 1;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf() -> Buffer {
        Buffer::new(
            20,
            vec![
                (100, 260101, "PGM".into()),
                (200, 260201, "  DCL VAR(&A)".into()),
                (300, 260301, "  dcl var(&b)".into()),
                (400, 260401, "ENDPGM".into()),
            ],
        )
    }

    fn opts(s: &str) -> FindOptions {
        FindOptions {
            find: s.into(),
            ..FindOptions::new()
        }
    }

    #[test]
    fn finds_ignoring_case() {
        let b = buf();
        let x = HashSet::new();
        let o = opts("dcl");
        let h = find(&b, &o, &x, None).unwrap();
        assert_eq!((h.rec, h.col), (1, 3));
        let h2 = find(&b, &o, &x, Some(h)).unwrap();
        assert_eq!(h2.rec, 2);
        assert!(find(&b, &o, &x, Some(h2)).is_none());
        let mut p = o.clone();
        p.dir = Direction::Previous;
        assert_eq!(find(&b, &p, &x, Some(h2)).unwrap().rec, 1);
        let mut s = o;
        s.same_case = true;
        assert_eq!(find(&b, &s, &x, None).unwrap().rec, 2);
    }

    #[test]
    fn columns_scope_and_dates() {
        let b = buf();
        let mut x = HashSet::new();
        let mut o = opts("PGM");
        o.from = 2;
        assert_eq!(find_all(&b, &o, &x).len(), 1);
        x.insert(b.get(3).unwrap().id);
        o.from = 1;
        o.scope = Scope::NotExcluded;
        assert_eq!(find_all(&b, &o, &x).len(), 1);
        o.scope = Scope::All;
        o.date = Some((260301, DateCmp::Greater));
        assert_eq!(find_all(&b, &o, &x)[0].rec, 3);
    }

    #[test]
    fn changes() {
        let mut b = buf();
        let x = HashSet::new();
        let mut o = opts("VAR");
        o.change = "VARIABLE".into();
        assert_eq!(change_all(&mut b, &o, &x, 261023), 2);
        assert_eq!(b.get(1).unwrap().data, "  DCL VARIABLE(&A)");
        assert_eq!(b.get(1).unwrap().date, 261023);
        let mut o = opts("ENDPGM");
        o.change = "X".into();
        o.shift = false;
        let h = find(&b, &o, &x, None).unwrap();
        assert!(change_at(&mut b, &o, h, 0));
        assert_eq!(b.get(3).unwrap().data, "X");
    }

    #[test]
    fn change_never_loses_data() {
        let mut b = Buffer::new(6, vec![(100, 0, "AB CDE".into())]);
        let mut o = opts("AB");
        o.change = "ABCD".into();
        let h = find(&b, &o, &HashSet::new(), None).unwrap();
        assert!(!change_at(&mut b, &o, h, 0));
        assert_eq!(b.get(0).unwrap().data, "AB CDE");
    }
}
