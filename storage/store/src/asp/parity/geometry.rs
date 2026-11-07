// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where a set's rows, parity pages and partial-parity slots are on each
//! member, and which pages make a stripe. See `docs/STORE_FORMAT.md` §11.

use crate::asp::layout::RESERVED_HEAD;
use crate::asp::parity::table::Level;

/// Entries in one partial-parity slot.
pub const PPL_ENTRIES: usize = 126;
/// Pages of one slot: the header and two partial pages per entry.
pub const SLOT_PAGES: u64 = 1 + 2 * PPL_ENTRIES as u64;

/// A set's layout over its members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Geometry {
    /// The level.
    pub level: Level,
    /// Members.
    pub n: usize,
    /// Blocks: each member has `w` rows and `k` parity pages per block.
    pub blocks: u64,
}

impl Geometry {
    /// For `n` members whose smallest span has `span_pages` pages. `None`
    /// when the members are too small to hold a block.
    #[must_use]
    pub fn new(level: Level, n: usize, span_pages: u64) -> Option<Self> {
        let room = span_pages.checked_sub(1 + RESERVED_HEAD + 2 * SLOT_PAGES + 1)?;
        let blocks = room / n as u64;
        (blocks > 0 && n >= level.min_units()).then_some(Self { level, n, blocks })
    }

    /// Parity pages per stripe.
    #[must_use]
    pub fn k(&self) -> usize {
        self.level.parity_pages()
    }

    /// Data pages per stripe, rows per block.
    #[must_use]
    pub fn w(&self) -> usize {
        self.n - self.k()
    }

    /// One past the last row.
    #[must_use]
    pub fn rows_end(&self) -> u64 {
        RESERVED_HEAD + self.blocks * self.w() as u64
    }

    /// First page of the partial-parity slots.
    #[must_use]
    pub fn ppl_start(&self) -> u64 {
        RESERVED_HEAD + self.blocks * self.n as u64
    }

    /// First page of slot 0 or 1.
    #[must_use]
    pub fn slot(&self, i: usize) -> u64 {
        self.ppl_start() + i as u64 * SLOT_PAGES
    }

    /// The state page: which members failed, written when one does.
    #[must_use]
    pub fn state_page(&self) -> u64 {
        self.ppl_start() + 2 * SLOT_PAGES
    }

    /// One past the last page the set uses on each member.
    #[must_use]
    pub fn end(&self) -> u64 {
        self.state_page() + 1
    }

    /// Whether a member page is a row.
    #[must_use]
    pub fn is_row(&self, lba: u64) -> bool {
        (RESERVED_HEAD..self.rows_end()).contains(&lba)
    }

    /// Stripes in the set.
    #[must_use]
    pub fn stripes(&self) -> u64 {
        self.blocks * self.n as u64
    }

    /// The block of a stripe.
    #[must_use]
    pub fn block_of(&self, stripe: u64) -> u64 {
        stripe / self.n as u64
    }

    /// The member holding P, and for RAID 6 the one holding Q.
    #[must_use]
    pub fn parity_members(&self, stripe: u64) -> (usize, Option<usize>) {
        let j = usize::try_from(stripe % self.n as u64).expect("small");
        let q = (self.level == Level::Raid6).then_some((j + 1) % self.n);
        (j, q)
    }

    /// Whether `member` holds parity of the `j`th stripe of a block.
    fn holds_parity(&self, member: usize, j: usize) -> bool {
        j == member || (self.level == Level::Raid6 && (j + 1) % self.n == member)
    }

    /// The page of P (`q` false) or Q of a stripe on its member.
    #[must_use]
    pub fn parity_lba(&self, stripe: u64, q: bool) -> u64 {
        self.rows_end() + self.k() as u64 * self.block_of(stripe) + u64::from(q)
    }

    /// The members giving the stripe a data page, ascending.
    #[must_use]
    pub fn data_members(&self, stripe: u64) -> Vec<usize> {
        let j = usize::try_from(stripe % self.n as u64).expect("small");
        (0..self.n).filter(|&m| !self.holds_parity(m, j)).collect()
    }

    /// The row of `member` in `stripe`; the member must give it data.
    #[must_use]
    pub fn row_of(&self, stripe: u64, member: usize) -> u64 {
        let j = usize::try_from(stripe % self.n as u64).expect("small");
        let i = (0..j).filter(|&x| !self.holds_parity(member, x)).count();
        RESERVED_HEAD + self.block_of(stripe) * self.w() as u64 + i as u64
    }

    /// The stripe a row of `member` belongs to.
    #[must_use]
    pub fn stripe_of(&self, member: usize, lba: u64) -> Option<u64> {
        if !self.is_row(lba) {
            return None;
        }
        let rel = lba - RESERVED_HEAD;
        let w = self.w() as u64;
        let block = rel / w;
        let i = usize::try_from(rel % w).expect("small");
        let j = (0..self.n)
            .filter(|&x| !self.holds_parity(member, x))
            .nth(i)
            .expect("w stripes per member");
        Some(block * self.n as u64 + j as u64)
    }

    /// What a member page is: a row's stripe, or the stripe whose P or Q
    /// it holds.
    #[must_use]
    pub fn page_of(&self, member: usize, lba: u64) -> Option<PageRole> {
        if let Some(s) = self.stripe_of(member, lba) {
            return Some(PageRole::Data(s));
        }
        if lba < self.rows_end() || lba >= self.ppl_start() {
            return None;
        }
        let rel = lba - self.rows_end();
        let block = rel / self.k() as u64;
        let q = rel % self.k() as u64 == 1;
        // P of stripe j sits on member j; Q of stripe j on member j + 1.
        let j = if q {
            (member + self.n - 1) % self.n
        } else {
            member
        };
        Some(PageRole::Parity {
            stripe: block * self.n as u64 + j as u64,
            q,
        })
    }
}

/// What a page of a member holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRole {
    /// A data page of the stripe.
    Data(u64),
    /// The P or Q page of the stripe.
    Parity {
        /// The stripe.
        stripe: u64,
        /// Q rather than P.
        q: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn check(level: Level, n: usize) {
        let g = Geometry::new(level, n, 1001 + 2 * SLOT_PAGES).unwrap();
        let mut seen = HashSet::new();
        for s in 0..g.stripes() {
            let (p, q) = g.parity_members(s);
            let data = g.data_members(s);
            assert_eq!(data.len(), g.w());
            assert!(!data.contains(&p));
            if let Some(q) = q {
                assert!(!data.contains(&q) && q != p);
                assert_eq!(
                    g.page_of(q, g.parity_lba(s, true)),
                    Some(PageRole::Parity { stripe: s, q: true })
                );
            }
            assert_eq!(
                g.page_of(p, g.parity_lba(s, false)),
                Some(PageRole::Parity {
                    stripe: s,
                    q: false
                })
            );
            for m in data {
                let lba = g.row_of(s, m);
                assert!(g.is_row(lba));
                assert_eq!(g.stripe_of(m, lba), Some(s));
                assert!(seen.insert((m, lba)), "row used twice");
            }
        }
        // Every row of every member is in exactly one stripe.
        assert_eq!(
            seen.len() as u64,
            g.n as u64 * (g.rows_end() - RESERVED_HEAD)
        );
        assert!(g.end() < 1001 + 2 * SLOT_PAGES);
    }

    #[test]
    fn every_row_in_one_stripe() {
        for n in 3..=18 {
            check(Level::Raid5, n);
        }
        for n in 4..=18 {
            check(Level::Raid6, n);
        }
    }

    #[test]
    fn too_small_or_too_few() {
        assert!(Geometry::new(Level::Raid5, 3, 100).is_none());
        assert!(Geometry::new(Level::Raid6, 3, 10_000).is_none());
        let g = Geometry::new(Level::Raid5, 4, 5 + 2 * SLOT_PAGES + 400).unwrap();
        assert_eq!(g.blocks, 100);
        assert_eq!(g.rows_end(), 3 + 300);
        assert_eq!(g.ppl_start(), 403);
    }
}
