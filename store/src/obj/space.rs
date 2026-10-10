// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An object's extents as one byte space of 4064-byte pages, read in
//! any span and changed in place through patches. See the format spec
//! §10.

use crate::alloc::Run;
use crate::obj::ObjectId;
use crate::obj::header::ObjectHeader;
use crate::obj::ops;
use crate::page::pager::{PageNo, Pager};
use crate::page::{BODY_SIZE, Page, PageType};
use crate::txn::Txn;
use crate::{Result, StoreError};

/// Bytes per page of a byte space.
pub const PAGE_BYTES: u64 = BODY_SIZE as u64;

/// The byte space of one object.
#[derive(Debug, Clone)]
pub struct Space {
    /// The object.
    pub id: ObjectId,
    /// Its header page.
    pub header_page: PageNo,
    /// Its extents, in order.
    pub extents: Vec<Run>,
}

impl Space {
    /// Load an object's space.
    pub fn load(txn: &mut Txn<'_>, id: ObjectId) -> Result<(Self, ObjectHeader)> {
        let (header_page, h) = ops::read_header(txn, id)?;
        let extents = ops::all_extents(txn, &h)?;
        Ok((
            Self {
                id,
                header_page,
                extents,
            },
            h,
        ))
    }

    /// Pages in the space.
    #[must_use]
    pub fn pages(&self) -> u64 {
        self.extents.iter().map(|r| r.len).sum()
    }

    /// Bytes in the space.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.pages() * PAGE_BYTES
    }

    /// The page holding byte `off` and the offset within its body.
    #[must_use]
    pub fn locate(&self, off: u64) -> Option<(PageNo, usize)> {
        let mut index = off / PAGE_BYTES;
        for r in &self.extents {
            if index < r.len {
                return Some((
                    r.start + index,
                    usize::try_from(off % PAGE_BYTES).expect("in page"),
                ));
            }
            index -= r.len;
        }
        None
    }

    fn out_of_space(&self, off: u64, len: usize) -> StoreError {
        StoreError::Corrupt {
            page: self.header_page,
            what: format!(
                "{off}+{len} is outside the {}-byte space of {}",
                self.bytes(),
                self.id
            ),
        }
    }

    /// Read `len` bytes at `off`.
    pub fn read(&self, txn: &mut Txn<'_>, off: u64, len: usize) -> Result<Vec<u8>> {
        if off + len as u64 > self.bytes() {
            return Err(self.out_of_space(off, len));
        }
        let mut out = Vec::with_capacity(len);
        let mut at = off;
        while out.len() < len {
            let (page, in_page) = self.locate(at).expect("checked");
            let take = (BODY_SIZE - in_page).min(len - out.len());
            let p = txn.read(page)?;
            p.verify_type(page, PageType::ObjectData)?;
            out.extend_from_slice(&p.body()[in_page..in_page + take]);
            at += take as u64;
        }
        Ok(out)
    }

    /// Change bytes at `off` in place.
    pub fn write(&self, txn: &mut Txn<'_>, off: u64, bytes: &[u8]) -> Result<()> {
        if off + bytes.len() as u64 > self.bytes() {
            return Err(self.out_of_space(off, bytes.len()));
        }
        let mut done = 0;
        while done < bytes.len() {
            let (page, in_page) = self.locate(off + done as u64).expect("checked");
            let take = (BODY_SIZE - in_page).min(bytes.len() - done);
            txn.patch(page, in_page, &bytes[done..done + take])?;
            done += take;
        }
        Ok(())
    }

    /// Grow the space to hold at least `bytes`, filling new pages with
    /// `fill`. Returns the space after growth.
    pub fn ensure(txn: &mut Txn<'_>, id: ObjectId, bytes: u64, fill: u8) -> Result<Self> {
        let (space, _) = Self::load(txn, id)?;
        if space.bytes() >= bytes {
            return Ok(space);
        }
        let need = bytes.div_ceil(PAGE_BYTES) - space.pages();
        for run in ops::grow(txn, id, need)? {
            for page in run.start..run.start + run.len {
                let mut p = Page::zeroed();
                p.body_mut().fill(fill);
                txn.write(page, &mut p, PageType::ObjectData, id)?;
            }
        }
        Ok(Self::load(txn, id)?.0)
    }

    /// Shrink the space to `bytes` (rounded up to pages).
    pub fn truncate(txn: &mut Txn<'_>, id: ObjectId, bytes: u64) -> Result<Self> {
        let (space, _) = Self::load(txn, id)?;
        let keep = bytes.div_ceil(PAGE_BYTES);
        if keep < space.pages() {
            ops::shrink(txn, id, space.pages() - keep)?;
        }
        Ok(Self::load(txn, id)?.0)
    }
}
