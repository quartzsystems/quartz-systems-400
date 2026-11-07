// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IFS map: a chain of pages listing the volume's extents in volume
//! order. See `docs/STORE_FORMAT.md` §12.

use crate::asp::io::AspIo;
use crate::bytes::{get_u16, get_u64, put_u16, put_u64, u16_of};
use crate::obj::ObjectId;
use crate::page::pager::PageNo;
use crate::page::{BODY_SIZE, Page, PageType};
use crate::spec::{Layout, field};
use crate::{Result, StoreError};

/// Magic at the start of every map page.
pub const MAGIC: &[u8; 8] = b"QS400IFS";
/// Header bytes.
pub const HEADER_LEN: usize = 32;
/// Bytes per extent.
pub const EXTENT_LEN: usize = 16;
/// Extents per page.
pub const PER_PAGE: usize = (BODY_SIZE - HEADER_LEN) / EXTENT_LEN;

const OFF_MAGIC: usize = 0;
const OFF_NEXT: usize = 8;
const OFF_COUNT: usize = 16;
const OFF_FIRST: usize = 24;
const EXT_START: usize = 0;
const EXT_PAGES: usize = 8;

/// Header layout for the spec test.
pub static HEADER_LAYOUT: Layout = Layout {
    name: "IfsMapHeader",
    size: Some(HEADER_LEN),
    fields: &[
        field("magic", OFF_MAGIC, 8),
        field("next", OFF_NEXT, 8),
        field("count", OFF_COUNT, 2),
        field("reserved", 18, 6),
        field("first_volume_page", OFF_FIRST, 8),
    ],
};

/// Extent layout for the spec test.
pub static EXTENT_LAYOUT: Layout = Layout {
    name: "IfsExtent",
    size: Some(EXTENT_LEN),
    fields: &[field("start", EXT_START, 8), field("pages", EXT_PAGES, 8)],
};

/// One run of the volume: logical pages `start .. start + pages`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IfsExtent {
    /// First logical page.
    pub start: PageNo,
    /// Length in pages.
    pub pages: u64,
}

impl IfsExtent {
    /// One past the last page.
    #[must_use]
    pub fn end(&self) -> PageNo {
        self.start + self.pages
    }
}

/// Pages a map of `n` extents takes (at least one).
#[must_use]
pub fn pages_for(n: usize) -> usize {
    n.div_ceil(PER_PAGE).max(1)
}

/// The map's pages, sealed, chained over `at` (one page number each,
/// `pages_for(extents.len())` of them).
#[must_use]
pub fn encode(at: &[PageNo], extents: &[IfsExtent], generation: u64) -> Vec<Page> {
    let need = pages_for(extents.len());
    assert_eq!(at.len(), need, "one page number per map page");
    let mut volume = 0u64;
    let mut out = Vec::with_capacity(need);
    for i in 0..need {
        let chunk = extents
            .get(i * PER_PAGE..((i + 1) * PER_PAGE).min(extents.len()))
            .unwrap_or(&[]);
        let mut page = Page::zeroed();
        let body = page.body_mut();
        body[OFF_MAGIC..OFF_MAGIC + 8].copy_from_slice(MAGIC);
        put_u64(body, OFF_NEXT, at.get(i + 1).copied().unwrap_or(0));
        put_u16(body, OFF_COUNT, u16_of(chunk.len()));
        put_u64(body, OFF_FIRST, volume);
        for (j, e) in chunk.iter().enumerate() {
            let o = HEADER_LEN + j * EXTENT_LEN;
            put_u64(body, o + EXT_START, e.start);
            put_u64(body, o + EXT_PAGES, e.pages);
            volume += e.pages;
        }
        page.seal(PageType::IfsMap, ObjectId::NONE, generation);
        out.push(page);
    }
    out
}

/// Write the map over `at`.
pub fn write(io: &mut AspIo, at: &[PageNo], extents: &[IfsExtent], generation: u64) -> Result<()> {
    for (pno, mut page) in at.iter().zip(encode(at, extents, generation)) {
        io.write_raw(*pno, &mut page)?;
    }
    Ok(())
}

/// Decode one map page: its extents and the next page.
pub fn decode(page: &Page, pno: PageNo, volume: u64) -> Result<(Vec<IfsExtent>, Option<PageNo>)> {
    let corrupt = |what: String| StoreError::Corrupt { page: pno, what };
    page.verify_type(pno, PageType::IfsMap)?;
    let body = page.body();
    if &body[OFF_MAGIC..OFF_MAGIC + 8] != MAGIC {
        return Err(corrupt("not an IFS map page".into()));
    }
    let count = usize::from(get_u16(body, OFF_COUNT));
    if count > PER_PAGE {
        return Err(corrupt(format!("IFS map page with {count} extents")));
    }
    if get_u64(body, OFF_FIRST) != volume {
        return Err(corrupt("IFS map page out of order".into()));
    }
    let extents = (0..count)
        .map(|j| {
            let o = HEADER_LEN + j * EXTENT_LEN;
            IfsExtent {
                start: get_u64(body, o + EXT_START),
                pages: get_u64(body, o + EXT_PAGES),
            }
        })
        .collect();
    let next = get_u64(body, OFF_NEXT);
    Ok((extents, (next != 0).then_some(next)))
}

/// Read the chain from `head`: the extents and the pages they occupy.
pub fn read(io: &mut AspIo, head: PageNo) -> Result<(Vec<IfsExtent>, Vec<PageNo>)> {
    let mut extents: Vec<IfsExtent> = Vec::new();
    let mut pages = Vec::new();
    let mut next = Some(head);
    while let Some(pno) = next {
        if pages.contains(&pno) {
            return Err(StoreError::Corrupt {
                page: pno,
                what: "IFS map loops".into(),
            });
        }
        let page = io.read_page(pno)?;
        let volume = extents.iter().map(|e| e.pages).sum();
        let (more, n) = decode(&page, pno, volume)?;
        extents.extend(more);
        pages.push(pno);
        next = n;
    }
    Ok((extents, pages))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_map_chains_and_decodes() {
        let extents: Vec<IfsExtent> = (0..300)
            .map(|i| IfsExtent {
                start: 1000 + i * 10,
                pages: 5,
            })
            .collect();
        assert_eq!(PER_PAGE, 252);
        assert_eq!(pages_for(300), 2);
        assert_eq!(pages_for(0), 1);
        let pages = encode(&[40, 41], &extents, 7);
        let (a, next) = decode(&pages[0], 40, 0).unwrap();
        assert_eq!(next, Some(41));
        assert_eq!(a.len(), 252);
        let (b, next) = decode(&pages[1], 41, 252 * 5).unwrap();
        assert_eq!(next, None);
        assert_eq!([a, b].concat(), extents);
        assert!(decode(&pages[1], 41, 0).is_err());
    }
}
