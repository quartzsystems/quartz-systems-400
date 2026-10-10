// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The free log: runs that are free but still allocated in the tree.
//! See `docs/STORE_FORMAT.md` §7.

use crate::asp::io::AspIo;
use crate::bytes::{get_u16, get_u64, put_u16, put_u64, u16_of};
use crate::obj::ObjectId;
use crate::page::pager::{PageNo, Pager};
use crate::page::{BODY_SIZE, Page, PageType};
use crate::spec::{Layout, field};
use crate::{Result, StoreError};

use super::Extent;

/// Header bytes.
pub const HEADER_LEN: usize = 16;
/// Bytes per entry.
pub const ENTRY_LEN: usize = 24;
/// Entries per page.
pub const PER_PAGE: usize = (BODY_SIZE - HEADER_LEN) / ENTRY_LEN;

const OFF_NEXT: usize = 0;
const OFF_COUNT: usize = 8;
const ENTRY_UNIT: usize = 0;
const ENTRY_LBA: usize = 8;
const ENTRY_LEN_OFF: usize = 16;

/// Header layout for the spec test.
pub static HEADER_LAYOUT: Layout = Layout {
    name: "FreeLogHeader",
    size: Some(HEADER_LEN),
    fields: &[
        field("next", OFF_NEXT, 8),
        field("count", OFF_COUNT, 2),
        field("reserved", 10, 6),
    ],
};

/// Entry layout for the spec test.
pub static ENTRY_LAYOUT: Layout = Layout {
    name: "FreeLogEntry",
    size: Some(ENTRY_LEN),
    fields: &[
        field("unit", ENTRY_UNIT, 2),
        field("reserved", 2, 6),
        field("lba", ENTRY_LBA, 8),
        field("len", ENTRY_LEN_OFF, 8),
    ],
};

/// Pages needed for `n` entries.
#[must_use]
pub fn pages_for(n: usize) -> usize {
    n.div_ceil(PER_PAGE)
}

/// Write `entries` over `pages` (at least one, and at least
/// `pages_for(entries.len())`), chained in order. Returns the head.
pub fn write(p: &mut dyn Pager, pages: &[PageNo], entries: &[Extent]) -> Result<Option<PageNo>> {
    let need = pages_for(entries.len()).max(1);
    assert!(pages.len() >= need, "not enough log pages");
    // An empty log still writes its page: the page is taken, so it must
    // be accounted for.
    let chunks: Vec<&[Extent]> = if entries.is_empty() {
        vec![&[]]
    } else {
        entries.chunks(PER_PAGE).collect()
    };
    for (i, chunk) in chunks.into_iter().enumerate() {
        let mut page = Page::zeroed();
        let body = page.body_mut();
        put_u64(body, OFF_NEXT, if i + 1 < need { pages[i + 1] } else { 0 });
        put_u16(body, OFF_COUNT, u16_of(chunk.len()));
        for (j, e) in chunk.iter().enumerate() {
            let at = HEADER_LEN + j * ENTRY_LEN;
            put_u16(body, at + ENTRY_UNIT, e.unit);
            put_u64(body, at + ENTRY_LBA, e.lba);
            put_u64(body, at + ENTRY_LEN_OFF, e.len);
        }
        p.write(pages[i], &mut page, PageType::FreeLog, ObjectId::NONE)?;
    }
    Ok(Some(pages[0]))
}

/// Read the chain from `head`: the entries and the pages they occupy.
pub fn read(io: &mut AspIo, head: Option<PageNo>) -> Result<(Vec<Extent>, Vec<PageNo>)> {
    let mut entries = Vec::new();
    let mut pages = Vec::new();
    let mut next = head;
    while let Some(pno) = next {
        if pages.contains(&pno) {
            return Err(StoreError::Corrupt {
                page: pno,
                what: "free log loops".into(),
            });
        }
        let page = io.read_page(pno)?;
        page.verify_type(pno, PageType::FreeLog)?;
        let body = page.body();
        let count = usize::from(get_u16(body, OFF_COUNT));
        if count > PER_PAGE {
            return Err(StoreError::Corrupt {
                page: pno,
                what: format!("free log page with {count} entries"),
            });
        }
        for j in 0..count {
            let at = HEADER_LEN + j * ENTRY_LEN;
            entries.push(Extent {
                unit: get_u16(body, at + ENTRY_UNIT),
                lba: get_u64(body, at + ENTRY_LBA),
                len: get_u64(body, at + ENTRY_LEN_OFF),
            });
        }
        pages.push(pno);
        let n = get_u64(body, OFF_NEXT);
        next = (n != 0).then_some(n);
    }
    Ok((entries, pages))
}
