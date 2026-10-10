// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Logical page numbers over the spans of an ASP. A span is the page
//! range one unit, or one mirrored pair, contributes; the allocator and
//! the WAL address spans, the I/O layer maps a span onto its members.

use crate::asp::superblock::{NO_PARTNER, UnitEntry};
use crate::page::pager::PageNo;
use crate::{Result, StoreError};

/// Pages at the head of every unit that are never allocated: the unit
/// header and two superblock copies.
pub const RESERVED_HEAD: u64 = 3;
/// Most units in an ASP (what fits in the superblock's unit table).
pub const MAX_UNITS: usize = 47;

/// Where a span sits in the logical page space and which units back it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitSpan {
    /// Logical page number of the span's page 0.
    pub start: PageNo,
    /// Span size in pages (the usable size of its units).
    pub pages: u64,
    /// The unit that backs the span, or the lower-numbered half.
    pub primary: u16,
    /// The other half of a mirrored pair.
    pub partner: Option<u16>,
}

impl UnitSpan {
    /// The units backing the span, primary first.
    pub fn members(&self) -> impl Iterator<Item = u16> {
        std::iter::once(self.primary).chain(self.partner)
    }
}

/// The spans as a mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    spans: Vec<UnitSpan>,
}

impl Layout {
    /// One unprotected unit per size, in unit order.
    #[must_use]
    pub fn new(sizes: &[u64]) -> Self {
        let mut start = 0;
        let spans = sizes
            .iter()
            .enumerate()
            .map(|(i, &pages)| {
                let s = UnitSpan {
                    start,
                    pages,
                    primary: u16::try_from(i).expect("at most MAX_UNITS"),
                    partner: None,
                };
                start += pages;
                s
            })
            .collect();
        Self { spans }
    }

    /// From a unit table: a span per unit, pairs sharing one. The
    /// spans must tile the page space in unit order.
    pub fn from_table(units: &[UnitEntry]) -> Result<Self> {
        let bad = |what: String| StoreError::Corrupt { page: 1, what };
        let mut spans: Vec<UnitSpan> = Vec::new();
        let mut next = 0;
        for (i, u) in units.iter().enumerate() {
            let unit = u16::try_from(i).map_err(|_| bad("too many units".into()))?;
            if u.mirror_partner != NO_PARTNER {
                let p = usize::from(u.mirror_partner);
                let Some(partner) = units.get(p) else {
                    return Err(bad(format!(
                        "unit {unit} pairs with a unit that is not in the table"
                    )));
                };
                if partner.mirror_partner != unit
                    || partner.start != u.start
                    || partner.usable_pages != u.usable_pages
                {
                    return Err(bad(format!("unit {unit} and its partner disagree")));
                }
                if p < i {
                    continue; // the partner's span already covers this unit
                }
            }
            if u.start != next || u.usable_pages == 0 || u.usable_pages > u.pages {
                return Err(bad(format!("unit {unit} does not continue the page space")));
            }
            spans.push(UnitSpan {
                start: u.start,
                pages: u.usable_pages,
                primary: unit,
                partner: (u.mirror_partner != NO_PARTNER).then_some(u.mirror_partner),
            });
            next += u.usable_pages;
        }
        if spans.is_empty() {
            return Err(bad("no units".into()));
        }
        Ok(Self { spans })
    }

    /// Number of spans.
    #[must_use]
    pub fn unit_count(&self) -> u16 {
        u16::try_from(self.spans.len()).expect("at most MAX_UNITS")
    }

    /// Pages in the whole ASP.
    #[must_use]
    pub fn total_pages(&self) -> u64 {
        self.spans.last().map_or(0, |s| s.start + s.pages)
    }

    /// A span.
    #[must_use]
    pub fn span(&self, span: u16) -> UnitSpan {
        self.spans[usize::from(span)]
    }

    /// All spans, in order.
    #[must_use]
    pub fn spans(&self) -> &[UnitSpan] {
        &self.spans
    }

    /// The span a unit belongs to.
    #[must_use]
    pub fn span_of_unit(&self, unit: u16) -> Option<u16> {
        self.spans
            .iter()
            .position(|s| s.primary == unit || s.partner == Some(unit))
            .map(|i| u16::try_from(i).expect("span index"))
    }

    /// Span and span-relative page of a logical page.
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
        Some((u16::try_from(i - 1).expect("span index"), pno - s.start))
    }

    /// Logical page of a span-relative page.
    #[must_use]
    pub fn logical(&self, span: u16, lba: u64) -> PageNo {
        self.span(span).start + lba
    }

    /// Append an unprotected unit as a new span at the end.
    pub fn push(&mut self, unit: u16, pages: u64) {
        let start = self.total_pages();
        self.spans.push(UnitSpan {
            start,
            pages,
            primary: unit,
            partner: None,
        });
    }

    /// Drop the last span.
    pub fn pop(&mut self) -> Option<UnitSpan> {
        self.spans.pop()
    }

    /// Unit `unit` left the table: every member numbered above it moves
    /// down by one.
    pub fn renumber_after(&mut self, unit: u16) {
        for s in &mut self.spans {
            if s.primary > unit {
                s.primary -= 1;
            }
            if let Some(p) = s.partner.as_mut()
                && *p > unit
            {
                *p -= 1;
            }
        }
    }

    /// Set a span's partner (or clear it) and its size.
    pub fn set_pair(&mut self, span: u16, partner: Option<u16>, pages: u64) {
        let s = &mut self.spans[usize::from(span)];
        s.partner = partner;
        s.pages = pages;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asp::superblock::UnitEntry;
    use crate::asp::unit::UnitState;

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

    fn entry(start: u64, pages: u64, usable: u64, partner: u16) -> UnitEntry {
        UnitEntry {
            unit_uuid: [0; 16],
            pages,
            start,
            usable_pages: usable,
            mirror_partner: partner,
            state: UnitState::Unprotected,
            serial: [0; 32],
        }
    }

    #[test]
    fn pairs_share_a_span() {
        let table = vec![
            entry(0, 100, 100, NO_PARTNER),
            entry(100, 60, 50, 2),
            entry(100, 50, 50, 1),
            entry(150, 20, 20, NO_PARTNER),
        ];
        let l = Layout::from_table(&table).unwrap();
        assert_eq!(l.unit_count(), 3);
        assert_eq!(l.total_pages(), 170);
        assert_eq!(l.span(1).members().collect::<Vec<_>>(), vec![1, 2]);
        assert_eq!(l.span_of_unit(2), Some(1));
        assert_eq!(l.span_of_unit(3), Some(2));
        assert_eq!(l.locate(149), Some((1, 49)));
        let mut broken = table.clone();
        broken[2].start = 101;
        assert!(Layout::from_table(&broken).is_err());
        let mut gap = table;
        gap[3].start = 151;
        assert!(Layout::from_table(&gap).is_err());
    }
}
