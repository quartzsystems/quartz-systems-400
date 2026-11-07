// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Volume pages to logical pages: the extents in volume order.

use crate::asp::ifs::IfsExtent;
use crate::page::pager::PageNo;

/// The volume as a mapping.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Volume {
    extents: Vec<IfsExtent>,
    /// The volume page of each extent's first page.
    starts: Vec<u64>,
    pages: u64,
}

impl Volume {
    /// Over `extents`, in volume order.
    #[must_use]
    pub fn new(extents: Vec<IfsExtent>) -> Self {
        let mut starts = Vec::with_capacity(extents.len());
        let mut pages = 0;
        for e in &extents {
            starts.push(pages);
            pages += e.pages;
        }
        Self {
            extents,
            starts,
            pages,
        }
    }

    /// Pages of the volume.
    #[must_use]
    pub fn pages(&self) -> u64 {
        self.pages
    }

    /// Whether there is a volume.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.extents.is_empty()
    }

    /// The extents in volume order.
    #[must_use]
    pub fn extents(&self) -> &[IfsExtent] {
        &self.extents
    }

    /// The logical page of volume page `v` and the pages of its extent
    /// from there.
    #[must_use]
    pub fn locate(&self, v: u64) -> Option<(PageNo, u64)> {
        if v >= self.pages {
            return None;
        }
        let i = self.starts.partition_point(|&s| s <= v) - 1;
        let e = self.extents[i];
        let off = v - self.starts[i];
        Some((e.start + off, e.pages - off))
    }

    /// Logical runs covering volume pages `v .. v + count`, each within
    /// one extent; `None` past the end.
    #[must_use]
    pub fn runs(&self, v: u64, count: u64) -> Option<Vec<(PageNo, u64)>> {
        if v.checked_add(count)? > self.pages {
            return None;
        }
        let mut out = Vec::new();
        let mut at = v;
        while at < v + count {
            let (pno, left) = self.locate(at)?;
            let n = left.min(v + count - at);
            out.push((pno, n));
            at += n;
        }
        Some(out)
    }

    /// Whether a logical page is a volume page.
    #[must_use]
    pub fn contains(&self, pno: PageNo) -> bool {
        self.extents.iter().any(|e| pno >= e.start && pno < e.end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_pages_map_through_the_extents() {
        let v = Volume::new(vec![
            IfsExtent {
                start: 100,
                pages: 10,
            },
            IfsExtent {
                start: 500,
                pages: 5,
            },
        ]);
        assert_eq!(v.pages(), 15);
        assert_eq!(v.locate(0), Some((100, 10)));
        assert_eq!(v.locate(9), Some((109, 1)));
        assert_eq!(v.locate(10), Some((500, 5)));
        assert_eq!(v.locate(15), None);
        assert_eq!(v.runs(8, 4), Some(vec![(108, 2), (500, 2)]));
        assert_eq!(v.runs(14, 2), None);
        assert!(v.contains(504) && !v.contains(505));
    }
}
