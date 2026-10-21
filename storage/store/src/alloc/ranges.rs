// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A set of page runs in unit coordinates, kept coalesced.

use std::collections::BTreeMap;

use super::Extent;

/// Disjoint, non-touching runs keyed by `(unit, lba)`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RangeSet {
    map: BTreeMap<(u16, u64), u64>,
}

impl RangeSet {
    /// Number of runs.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Runs in order.
    pub fn iter(&self) -> impl Iterator<Item = Extent> + '_ {
        self.map
            .iter()
            .map(|(&(unit, lba), &len)| Extent { unit, lba, len })
    }

    /// Whether any run shares a page with `e`.
    pub fn overlaps(&self, e: Extent) -> bool {
        if let Some((&(_, lba), &len)) = self.map.range(..=(e.unit, e.lba)).next_back()
            && self
                .map
                .range(..=(e.unit, e.lba))
                .next_back()
                .is_some_and(|(k, _)| k.0 == e.unit)
            && lba + len > e.lba
        {
            return true;
        }
        self.map
            .range((e.unit, e.lba)..(e.unit, e.end()))
            .next()
            .is_some()
    }

    /// Add `e`, merging with neighbours. `e` must not overlap.
    pub fn insert(&mut self, e: Extent) {
        debug_assert!(!self.overlaps(e), "overlapping insert {e:?}");
        if e.len == 0 {
            return;
        }
        let mut lba = e.lba;
        let mut end = e.end();
        if let Some((&(u, l), &n)) = self.map.range(..(e.unit, e.lba)).next_back()
            && u == e.unit
            && l + n == lba
        {
            self.map.remove(&(u, l));
            lba = l;
        }
        if let Some(&n) = self.map.get(&(e.unit, end)) {
            self.map.remove(&(e.unit, end));
            end += n;
        }
        self.map.insert((e.unit, lba), end - lba);
    }

    /// Take the pages of `e` out of the set. Returns the parts of `e`
    /// that were not in the set, in order.
    pub fn remove(&mut self, e: Extent) -> Vec<Extent> {
        let end = e.end();
        let mut uncovered = Vec::new();
        let mut cur = e.lba;
        // Runs that can overlap: the one starting before e.lba, and those
        // starting inside e.
        let mut hits: Vec<(u64, u64)> = Vec::new();
        if let Some((&(u, l), &n)) = self.map.range(..(e.unit, e.lba)).next_back()
            && u == e.unit
            && l + n > e.lba
        {
            hits.push((l, n));
        }
        hits.extend(
            self.map
                .range((e.unit, e.lba)..(e.unit, end))
                .map(|(&(_, l), &n)| (l, n)),
        );
        for (l, n) in hits {
            let run_end = l + n;
            if l > cur {
                uncovered.push(Extent {
                    unit: e.unit,
                    lba: cur,
                    len: l - cur,
                });
                cur = l;
            }
            let cut_end = run_end.min(end);
            self.map.remove(&(e.unit, l));
            if l < cur {
                self.map.insert((e.unit, l), cur - l);
            }
            if run_end > cut_end {
                self.map.insert((e.unit, cut_end), run_end - cut_end);
            }
            cur = cut_end;
        }
        if cur < end {
            uncovered.push(Extent {
                unit: e.unit,
                lba: cur,
                len: end - cur,
            });
        }
        uncovered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(lba: u64, len: u64) -> Extent {
        Extent { unit: 0, lba, len }
    }

    #[test]
    fn insert_coalesces() {
        let mut s = RangeSet::default();
        s.insert(e(10, 5));
        s.insert(e(20, 5));
        s.insert(e(15, 5));
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![e(10, 15)]);
        s.insert(Extent {
            unit: 1,
            lba: 25,
            len: 1,
        });
        assert_eq!(s.len(), 2);
        assert!(s.overlaps(e(24, 1)));
        assert!(!s.overlaps(e(25, 1)));
        assert!(!s.overlaps(e(0, 10)));
        assert!(s.overlaps(e(0, 11)));
    }

    #[test]
    fn remove_reports_gaps() {
        let mut s = RangeSet::default();
        s.insert(e(10, 10));
        s.insert(e(30, 10));
        let gaps = s.remove(e(15, 20));
        assert_eq!(gaps, vec![e(20, 10)]);
        assert_eq!(s.iter().collect::<Vec<_>>(), vec![e(10, 5), e(35, 5)]);
        assert_eq!(s.remove(e(0, 5)), vec![e(0, 5)]);
        assert_eq!(s.remove(e(12, 2)), vec![]);
        assert_eq!(
            s.iter().collect::<Vec<_>>(),
            vec![e(10, 2), e(14, 1), e(35, 5)]
        );
    }
}
