// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Pagers used by a checkpoint: one that hands out pages from chunks
//! taken in advance, and a read-only one for scanning.

use crate::asp::io::AspIo;
use crate::asp::layout::Layout;
use crate::obj::ObjectId;
use crate::page::pager::{PageNo, Pager};
use crate::page::{Page, PageType};
use crate::{Result, StoreError};

use super::Extent;

/// Pages for one checkpoint, handed out in order from its chunks.
pub struct ChunkPager<'a> {
    io: &'a mut AspIo,
    layout: &'a Layout,
    chunks: Vec<Extent>,
    /// Index of the chunk being consumed and the offset into it.
    cursor: (usize, u64),
    taken: Vec<PageNo>,
    generation: u64,
}

impl<'a> ChunkPager<'a> {
    /// Over `chunks`, already cleared in the bitmaps.
    pub fn new(
        io: &'a mut AspIo,
        layout: &'a Layout,
        chunks: Vec<Extent>,
        generation: u64,
    ) -> Self {
        Self {
            io,
            layout,
            chunks,
            cursor: (0, 0),
            taken: Vec::new(),
            generation,
        }
    }

    /// Pages handed out so far, in order.
    pub fn taken(&self) -> &[PageNo] {
        &self.taken
    }

    /// The unused part of every chunk, as runs.
    pub fn tails(&self) -> Vec<Extent> {
        let (ci, off) = self.cursor;
        self.chunks
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let used = match i.cmp(&ci) {
                    std::cmp::Ordering::Less => c.len,
                    std::cmp::Ordering::Equal => off,
                    std::cmp::Ordering::Greater => 0,
                };
                (used < c.len).then(|| Extent {
                    unit: c.unit,
                    lba: c.lba + used,
                    len: c.len - used,
                })
            })
            .collect()
    }
}

impl Pager for ChunkPager<'_> {
    fn read(&mut self, pno: PageNo) -> Result<Page> {
        self.io.read_page(pno)
    }

    fn alloc(&mut self) -> Result<PageNo> {
        loop {
            let (ci, off) = self.cursor;
            let Some(c) = self.chunks.get(ci) else {
                return Err(StoreError::NoSpace { asp: 0 });
            };
            if off < c.len {
                self.cursor = (ci, off + 1);
                let pno = self.layout.logical(c.unit, c.lba + off);
                self.taken.push(pno);
                return Ok(pno);
            }
            self.cursor = (ci + 1, 0);
        }
    }

    fn write(
        &mut self,
        pno: PageNo,
        page: &mut Page,
        kind: PageType,
        owner: ObjectId,
    ) -> Result<()> {
        self.io.write_page(pno, page, kind, owner, self.generation)
    }

    fn free(&mut self, pno: PageNo) -> Result<()> {
        Err(StoreError::Corrupt {
            page: pno,
            what: "free during a checkpoint".into(),
        })
    }
}

/// A pager that only reads, for scanning a tree at open.
pub struct ReadOnly<'a>(pub &'a mut AspIo);

impl Pager for ReadOnly<'_> {
    fn read(&mut self, pno: PageNo) -> Result<Page> {
        self.0.read_page(pno)
    }

    fn alloc(&mut self) -> Result<PageNo> {
        Err(StoreError::Corrupt {
            page: 0,
            what: "write through a read-only pager".into(),
        })
    }

    fn write(&mut self, pno: PageNo, _: &mut Page, _: PageType, _: ObjectId) -> Result<()> {
        Err(StoreError::Corrupt {
            page: pno,
            what: "write through a read-only pager".into(),
        })
    }

    fn free(&mut self, pno: PageNo) -> Result<()> {
        Err(StoreError::Corrupt {
            page: pno,
            what: "free through a read-only pager".into(),
        })
    }
}
