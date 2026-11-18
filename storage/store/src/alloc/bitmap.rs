// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One unit's free bitmap: a set bit is a free page.

/// Free pages of a unit.
#[derive(Debug, Clone)]
pub struct Bitmap {
    words: Vec<u64>,
    pages: u64,
    free: u64,
}

fn idx(v: u64) -> usize {
    usize::try_from(v).expect("fits")
}

impl Bitmap {
    /// All pages allocated.
    #[must_use]
    pub fn new(pages: u64) -> Self {
        Self {
            words: vec![0; idx(pages.div_ceil(64))],
            pages,
            free: 0,
        }
    }

    /// Pages in the unit.
    #[must_use]
    pub fn pages(&self) -> u64 {
        self.pages
    }

    /// Free pages.
    #[must_use]
    pub fn free(&self) -> u64 {
        self.free
    }

    /// Whether page `lba` is free.
    #[must_use]
    pub fn is_free(&self, lba: u64) -> bool {
        lba < self.pages && self.words[idx(lba / 64)] >> (lba % 64) & 1 == 1
    }

    /// Whether every page of the run is allocated.
    #[must_use]
    pub fn is_clear(&self, lba: u64, len: u64) -> bool {
        lba + len <= self.pages && (lba..lba + len).all(|p| !self.is_free(p))
    }

    /// Mark a run free. Every page must be allocated.
    pub fn set(&mut self, lba: u64, len: u64) {
        for p in lba..lba + len {
            debug_assert!(!self.is_free(p), "page {p} already free");
            self.words[idx(p / 64)] |= 1 << (p % 64);
        }
        self.free += len;
    }

    /// Mark a run allocated. Every page must be free.
    pub fn clear(&mut self, lba: u64, len: u64) {
        for p in lba..lba + len {
            debug_assert!(self.is_free(p), "page {p} not free");
            self.words[idx(p / 64)] &= !(1 << (p % 64));
        }
        self.free -= len;
    }

    /// Free pages from `lba` on, up to `max`.
    #[must_use]
    pub fn free_run_from(&self, lba: u64, max: u64) -> u64 {
        (lba..self.pages.min(lba + max))
            .take_while(|&p| self.is_free(p))
            .count() as u64
    }

    /// Free runs as `(lba, len)`, ascending.
    pub fn runs(&self) -> impl Iterator<Item = (u64, u64)> + '_ {
        let mut at = 0;
        std::iter::from_fn(move || {
            let (start, len) = self.next_run(at)?;
            at = start + len;
            Some((start, len))
        })
    }

    /// The first free run starting at or after `from`.
    fn next_run(&self, from: u64) -> Option<(u64, u64)> {
        let mut p = from;
        // Skip allocated pages a word at a time.
        while p < self.pages {
            let w = self.words[idx(p / 64)] >> (p % 64);
            if w == 0 {
                p = (p / 64 + 1) * 64;
                continue;
            }
            p += u64::from(w.trailing_zeros());
            break;
        }
        if p >= self.pages {
            return None;
        }
        // Count free pages a word at a time. A run that reaches the end
        // of a word continues into the next.
        let start = p;
        while p < self.pages {
            let shift = p % 64;
            let allocated = !(self.words[idx(p / 64)] >> shift);
            let run = u64::from(allocated.trailing_zeros());
            if run >= 64 - shift {
                p = (p / 64 + 1) * 64;
                continue;
            }
            p += run;
            break;
        }
        Some((start, p.min(self.pages) - start))
    }

    /// First run of at least `len` pages.
    #[must_use]
    pub fn first_fit(&self, len: u64) -> Option<u64> {
        self.runs().find(|&(_, l)| l >= len).map(|(s, _)| s)
    }

    /// The longest run, capped at `max`: `(lba, len)`.
    #[must_use]
    pub fn largest_run(&self, max: u64) -> Option<(u64, u64)> {
        let mut best: Option<(u64, u64)> = None;
        for (s, l) in self.runs() {
            if l >= max {
                return Some((s, max));
            }
            if best.is_none_or(|(_, bl)| l > bl) {
                best = Some((s, l));
            }
        }
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_and_fits() {
        let mut b = Bitmap::new(200);
        assert_eq!(b.free(), 0);
        assert_eq!(b.runs().count(), 0);
        b.set(3, 10);
        b.set(70, 100);
        b.set(199, 1);
        assert_eq!(b.free(), 111);
        assert_eq!(
            b.runs().collect::<Vec<_>>(),
            vec![(3, 10), (70, 100), (199, 1)]
        );
        assert_eq!(b.first_fit(10), Some(3));
        assert_eq!(b.first_fit(11), Some(70));
        assert_eq!(b.first_fit(101), None);
        assert_eq!(b.largest_run(50), Some((70, 50)));
        assert_eq!(b.largest_run(1000), Some((70, 100)));
        assert_eq!(b.free_run_from(5, 100), 8);
        assert_eq!(b.free_run_from(13, 100), 0);
        assert!(b.is_clear(13, 57));
        assert!(!b.is_clear(12, 2));
        b.clear(3, 10);
        assert_eq!(b.first_fit(1), Some(70));
        assert_eq!(b.free(), 101);
    }

    #[test]
    fn word_boundaries() {
        let mut b = Bitmap::new(130);
        b.set(0, 130);
        assert_eq!(b.runs().collect::<Vec<_>>(), vec![(0, 130)]);
        b.clear(64, 1);
        assert_eq!(b.runs().collect::<Vec<_>>(), vec![(0, 64), (65, 65)]);
        b.clear(127, 2);
        assert_eq!(
            b.runs().collect::<Vec<_>>(),
            vec![(0, 64), (65, 62), (129, 1)]
        );
    }
}
