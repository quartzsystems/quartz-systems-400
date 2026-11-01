// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The output encoder: the cells a render must write, as orders.
//! Cells the client already shows are skipped with Set Buffer Address
//! when that is shorter than writing them, runs of four or more of one
//! character go as Repeat to Address. A run of nulls is a repeat too,
//! never an Erase to Address: that costs one byte more (its length and
//! attribute-type bytes) and erases nothing more on this model, which
//! keeps no extended attributes.

use crate::ds::NULL;
use crate::ds::build::Builder;

/// A field to start: the Start of Field order at its attribute cell.
pub(super) struct Start<'a> {
    /// The attribute cell, linear.
    pub at: usize,
    /// The field format word.
    pub ffw: u16,
    /// Field control words.
    pub fcws: &'a [u16],
    /// The field's length.
    pub len: u16,
}

/// What to write.
pub(super) struct Plan<'a> {
    /// Columns.
    pub cols: usize,
    /// The cells the client should show.
    pub cells: &'a [u8],
    /// The cells that differ from what the client shows.
    pub changed: &'a [bool],
    /// The fields to start, in screen order.
    pub starts: &'a [Start<'a>],
}

/// The shortest run worth a Repeat to Address (four bytes).
const MIN_RUN: usize = 4;
/// The longest gap of unchanged cells written through rather than
/// skipped with a Set Buffer Address (three bytes).
const MAX_GAP: usize = 3;

impl Plan<'_> {
    /// The orders, after a Write To Display (and its Start of Header).
    pub fn encode(&self, b: &mut Builder) {
        let mut sf = vec![None; self.cells.len()];
        for (k, s) in self.starts.iter().enumerate() {
            if let Some(slot) = sf.get_mut(s.at) {
                *slot = Some(k);
            }
        }
        let todo: Vec<usize> = (0..self.cells.len())
            .filter(|&i| self.changed[i] || sf[i].is_some())
            .collect();
        self.write(b, &todo, &sf);
    }

    fn rc(&self, i: usize) -> (u8, u8) {
        ((i / self.cols + 1) as u8, (i % self.cols + 1) as u8)
    }

    /// The cells at `todo` (sorted), with cheap gaps written through:
    /// each stretch after its own Set Buffer Address.
    fn write(&self, out: &mut Builder, todo: &[usize], sf: &[Option<usize>]) {
        let mut next = 0;
        while next < todo.len() {
            let first = todo[next];
            let mut last = first;
            next += 1;
            while next < todo.len() && self.through(last, todo[next]) {
                last = todo[next];
                next += 1;
            }
            let (row, col) = self.rc(first);
            out.sba(row, col);
            self.segment(out, first, last + 1, sf);
        }
    }

    /// Whether the unchanged cells between `e` and `s` are written
    /// rather than skipped: a short gap, or one that only lengthens a
    /// run on either side (a repeat costs the same however long).
    fn through(&self, e: usize, s: usize) -> bool {
        let gap = &self.cells[e + 1..s];
        let extends = |c: u8| repeatable(c) && gap.iter().all(|&g| g == c);
        gap.len() <= MAX_GAP || extends(self.cells[e]) || extends(self.cells[s])
    }

    /// Cells `from..to`: Start of Field orders where fields start,
    /// data between.
    fn segment(&self, b: &mut Builder, from: usize, to: usize, sf: &[Option<usize>]) {
        let mut i = from;
        while i < to {
            if let Some(k) = sf[i] {
                let s = &self.starts[k];
                b.sf(s.ffw, s.fcws, self.cells[i], s.len);
                i += 1;
                continue;
            }
            let stop = (i..to).find(|&j| sf[j].is_some()).unwrap_or(to);
            self.data(b, i, stop);
            i = stop;
        }
    }

    /// Data `from..to` with runs as Repeat to Address.
    fn data(&self, out: &mut Builder, from: usize, to: usize) {
        let cells = self.cells;
        let mut literal = from;
        let mut run = from;
        while run < to {
            let ch = cells[run];
            let mut end = run + 1;
            while end < to && cells[end] == ch {
                end += 1;
            }
            if end - run >= MIN_RUN && repeatable(ch) {
                out.data(&cells[literal..run]);
                let (row, col) = self.rc(end - 1);
                out.ra(row, col, ch);
                literal = end;
            }
            run = end;
        }
        out.data(&cells[literal..to]);
    }
}

/// Whether a byte may be repeated: a null or a character (not an
/// attribute or a control byte).
fn repeatable(c: u8) -> bool {
    c == NULL || c >= 0x40
}
