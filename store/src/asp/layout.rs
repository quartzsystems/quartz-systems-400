// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Logical page numbers over concatenated units.

use crate::page::pager::PageNo;

/// Pages at the head of every unit that are never allocated: the unit
/// header and two superblock copies.
pub const RESERVED_HEAD: u64 = 3;
/// Most units in an ASP (what fits in the superblock's unit table).
pub const MAX_UNITS: usize = 119;

/// Where a unit sits in the logical page space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitSpan {
    /// Logical page number of the unit's page 0.
    pub start: PageNo,
    /// Unit size in pages.
    pub pages: u64,
}

/// The unit table as a mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    spans: Vec<UnitSpan>,
}

impl Layout {
    /// From unit sizes in unit order.
    #[must_use]
    pub fn new(sizes: &[u64]) -> Self {
        let mut start = 0;
        let spans = sizes
            .iter()
            .map(|&pages| {
                let s = UnitSpan { start, pages };
                start += pages;
                s
            })
            .collect();
        Self { spans }
    }

    /// Number of units.
    #[must_use]
    pub fn unit_count(&self) -> u16 {
        u16::try_from(self.spans.len()).expect("at most MAX_UNITS")
    }

    /// Pages in the whole ASP.
    #[must_use]
    pub fn total_pages(&self) -> u64 {
        self.spans.last().map_or(0, |s| s.start + s.pages)
    }

    /// A unit's span.
    #[must_use]
    pub fn span(&self, unit: u16) -> UnitSpan {
        self.spans[usize::from(unit)]
    }

    /// All spans, in unit order.
    #[must_use]
    pub fn spans(&self) -> &[UnitSpan] {
        &self.spans
    }

    /// Unit and unit-relative page of a logical page.
    #[must_use]
    pub fn locate(&self, pno: PageNo) -> Option<(u16, u64)> {
        let i = self.spans.partition_point(|s| s.start <= pno);
        if i == 0 {
            return None;
        }
        let s = self.spans[i - 1];
        if pno >= s.start + s.pages {
            return None;
        }
        Some((u16::try_from(i - 1).expect("unit index"), pno - s.start))
    }

    /// Logical page of a unit-relative page.
    #[must_use]
    pub fn logical(&self, unit: u16, lba: u64) -> PageNo {
        self.span(unit).start + lba
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_both_ways() {
        let l = Layout::new(&[100, 50]);
        assert_eq!(l.total_pages(), 150);
        assert_eq!(l.unit_count(), 2);
        assert_eq!(l.locate(0), Some((0, 0)));
        assert_eq!(l.locate(99), Some((0, 99)));
        assert_eq!(l.locate(100), Some((1, 0)));
        assert_eq!(l.locate(149), Some((1, 49)));
        assert_eq!(l.locate(150), None);
        assert_eq!(l.logical(1, 7), 107);
    }
}
