// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sort-merge join's matching: both sides' row numbers sorted on
//! the join column (stably, so rows with one value keep their order),
//! then merged, which gives each left row the run of right rows with
//! its value. A side already in order is not sorted. The runs are
//! handed back by left row, so the join still walks the left rows in
//! their order and each one's right rows in theirs.

use std::cmp::Ordering;

use super::super::eval::Row;
use super::super::mem::Charge;
use super::class;
use crate::error::Result;
use crate::value::Value;

/// Each left row's run of right rows.
pub(super) struct Runs {
    /// Right row numbers in the join column's order.
    sorted: Vec<usize>,
    /// By left row: its run in `sorted`; `None` when any right row may
    /// match (a value of another kind).
    by_left: Vec<Option<(usize, usize)>>,
    _charge: Charge,
}

fn cmp(a: &Value, b: &Value) -> Option<Ordering> {
    a.compare(b).ok().flatten()
}

/// Whether the values, nulls left out, never go down.
pub(super) fn in_order<'a>(mut values: impl Iterator<Item = &'a Value>) -> bool {
    let mut last: Option<&Value> = None;
    values.all(|v| {
        if v.is_null() {
            return true;
        }
        let ok = last.is_none_or(|l| cmp(l, v).is_some_and(|o| o != Ordering::Greater));
        last = Some(v);
        ok
    })
}

/// Row numbers of `rows` with a value at `col` of one kind, sorted on
/// it; `None` when the values are of several kinds or do not compare.
fn sorted(rows: &[Row], col: usize, presorted: bool) -> Option<(Vec<usize>, Option<u8>)> {
    let mut kind = None;
    let mut ids = Vec::with_capacity(rows.len());
    for (i, r) in rows.iter().enumerate() {
        let v = &r[col];
        if v.is_null() {
            continue;
        }
        let k = class(v)?;
        if kind.is_some_and(|x| x != k) {
            return None;
        }
        kind = Some(k);
        ids.push(i);
    }
    if !presorted {
        let mut bad = false;
        ids.sort_by(|&a, &b| {
            cmp(&rows[a][col], &rows[b][col]).unwrap_or_else(|| {
                bad = true;
                Ordering::Equal
            })
        });
        if bad {
            return None;
        }
    }
    Some((ids, kind))
}

impl Runs {
    /// Runs for an equality of left column `lcol` and right column
    /// `rcol`; `None` when the right values do not sort.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn build(
        mut charge: Charge,
        left: &[Row],
        lcol: usize,
        left_sorted: bool,
        right: &[Row],
        rcol: usize,
        right_sorted: bool,
    ) -> Result<Option<Self>> {
        let Some((rids, rkind)) = sorted(right, rcol, right_sorted) else {
            return Ok(None);
        };
        charge.add(rids.len() * std::mem::size_of::<usize>())?;
        let mut runs: Vec<Option<(usize, usize)>> = vec![Some((0, 0)); left.len()];
        charge.add(left.len() * std::mem::size_of::<Option<(usize, usize)>>())?;
        // The left rows of the right kind, in order; others match anything.
        let mut lids = Vec::with_capacity(left.len());
        for (i, r) in left.iter().enumerate() {
            let v = &r[lcol];
            if v.is_null() || rkind.is_none() {
                continue;
            }
            if class(v) != rkind {
                runs[i] = None;
                continue;
            }
            lids.push(i);
        }
        if !left_sorted {
            let mut bad = false;
            lids.sort_by(|&a, &b| {
                cmp(&left[a][lcol], &left[b][lcol]).unwrap_or_else(|| {
                    bad = true;
                    Ordering::Equal
                })
            });
            if bad {
                return Ok(None);
            }
        }
        charge.add(lids.len() * std::mem::size_of::<usize>())?;
        // The merge: one pass over both.
        let mut at = 0;
        for &li in &lids {
            let v = &left[li][lcol];
            while at < rids.len() && cmp(&right[rids[at]][rcol], v) == Some(Ordering::Less) {
                at += 1;
            }
            let mut end = at;
            while end < rids.len() && cmp(&right[rids[end]][rcol], v) == Some(Ordering::Equal) {
                end += 1;
            }
            runs[li] = Some((at, end));
        }
        Ok(Some(Self {
            sorted: rids,
            by_left: runs,
            _charge: charge,
        }))
    }

    /// The right rows left row `li` may match, in their order; `None`
    /// when any may.
    pub(super) fn lookup(&self, li: usize) -> Option<&[usize]> {
        let (a, b) = self.by_left[li]?;
        Some(&self.sorted[a..b])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::mem::Meter;

    fn rows(vals: &[Option<i64>]) -> Vec<Row> {
        vals.iter()
            .map(|v| vec![v.map_or(Value::Null, Value::int)])
            .collect()
    }

    #[test]
    fn runs_keep_each_sides_order() {
        let meter = Meter::default();
        let left = rows(&[Some(3), Some(1), None, Some(3), Some(9)]);
        let right = rows(&[Some(3), Some(1), Some(3), None, Some(2)]);
        let r = Runs::build(
            Charge::new(&meter, usize::MAX),
            &left,
            0,
            false,
            &right,
            0,
            false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(r.lookup(0), Some(&[0, 2][..]));
        assert_eq!(r.lookup(1), Some(&[1][..]));
        assert_eq!(r.lookup(2), Some(&[][..]));
        assert_eq!(r.lookup(3), Some(&[0, 2][..]));
        assert_eq!(r.lookup(4), Some(&[][..]));
        assert!(in_order(
            rows(&[Some(1), None, Some(2), Some(2)])
                .iter()
                .map(|r| &r[0])
        ));
        assert!(!in_order(rows(&[Some(2), Some(1)]).iter().map(|r| &r[0])));
    }

    #[test]
    fn another_kind_matches_anything() {
        let meter = Meter::default();
        let left = vec![vec![Value::Str("1".into())]];
        let right = rows(&[Some(1)]);
        let r = Runs::build(
            Charge::new(&meter, usize::MAX),
            &left,
            0,
            false,
            &right,
            0,
            false,
        )
        .unwrap()
        .unwrap();
        assert_eq!(r.lookup(0), None);
    }
}
