// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A bounded page cache with CLOCK replacement. It only ever holds
//! pages as they are on the device: every write goes through to the
//! device and updates the copy here, so nothing in the cache is dirty.
//! `MIGRATE(phase5)`: this becomes the *MACHINE pool.

use std::collections::HashMap;

use crate::page::Page;
use crate::page::pager::PageNo;

/// Hit and miss counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Lookups that found the page.
    pub hits: u64,
    /// Lookups that did not.
    pub misses: u64,
    /// Pages evicted.
    pub evictions: u64,
}

struct Slot {
    page: PageNo,
    data: Page,
    referenced: bool,
}

/// The cache.
pub struct PageCache {
    capacity: usize,
    slots: Vec<Option<Slot>>,
    index: HashMap<PageNo, usize>,
    hand: usize,
    stats: CacheStats,
}

impl PageCache {
    /// A cache for at most `capacity` pages (at least 1).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            capacity,
            slots: Vec::with_capacity(capacity.min(4096)),
            index: HashMap::new(),
            hand: 0,
            stats: CacheStats::default(),
        }
    }

    /// Pages held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.index.len()
    }

    /// Whether nothing is held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Capacity in pages.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Counters.
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        self.stats
    }

    /// A copy of the page, if held.
    pub fn get(&mut self, page: PageNo) -> Option<Page> {
        let Some(&i) = self.index.get(&page) else {
            self.stats.misses += 1;
            return None;
        };
        let slot = self.slots[i].as_mut().expect("indexed slot");
        slot.referenced = true;
        self.stats.hits += 1;
        Some(slot.data.clone())
    }

    /// Hold `data` for `page`, replacing any earlier copy.
    pub fn insert(&mut self, page: PageNo, data: Page) {
        if let Some(&i) = self.index.get(&page) {
            let slot = self.slots[i].as_mut().expect("indexed slot");
            slot.data = data;
            slot.referenced = true;
            return;
        }
        let i = if self.slots.len() < self.capacity {
            self.slots.push(None);
            self.slots.len() - 1
        } else {
            self.evict()
        };
        self.slots[i] = Some(Slot {
            page,
            data,
            referenced: false,
        });
        self.index.insert(page, i);
    }

    /// Forget `page`.
    pub fn remove(&mut self, page: PageNo) {
        if let Some(i) = self.index.remove(&page) {
            self.slots[i] = None;
        }
    }

    /// Forget everything.
    pub fn clear(&mut self) {
        self.slots.clear();
        self.index.clear();
        self.hand = 0;
    }

    /// Free a slot by the CLOCK rule: sweep, clearing reference bits,
    /// until an unreferenced slot turns up.
    fn evict(&mut self) -> usize {
        loop {
            let i = self.hand;
            self.hand = (self.hand + 1) % self.slots.len();
            match self.slots[i].as_mut() {
                None => return i,
                Some(s) if s.referenced => s.referenced = false,
                Some(s) => {
                    self.index.remove(&s.page);
                    self.slots[i] = None;
                    self.stats.evictions += 1;
                    return i;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(fill: u8) -> Page {
        let mut p = Page::zeroed();
        p.body_mut()[0] = fill;
        p
    }

    #[test]
    fn bounded_with_clock_replacement() {
        let mut c = PageCache::new(3);
        for i in 0..3u64 {
            c.insert(i, page(u8::try_from(i).unwrap()));
        }
        assert_eq!(c.len(), 3);
        // Touch 0 and 1; 2 is the eviction candidate.
        assert_eq!(c.get(0).unwrap().body()[0], 0);
        assert!(c.get(1).is_some());
        c.insert(3, page(3));
        assert_eq!(c.len(), 3);
        assert!(c.get(2).is_none(), "2 was unreferenced");
        assert!(c.get(0).is_some());
        assert_eq!(c.stats().evictions, 1);
        c.insert(0, page(9));
        assert_eq!(c.get(0).unwrap().body()[0], 9, "insert replaces");
        c.remove(0);
        assert!(c.get(0).is_none());
        assert_eq!(c.len(), 2);
        c.insert(4, page(4));
        assert_eq!(c.len(), 3);
        assert_eq!(c.stats().hits, 4);
    }
}
