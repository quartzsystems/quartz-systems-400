// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Page I/O over the units of an ASP, addressed by logical page number.

use blockdev::BlockDev;

use crate::asp::layout::Layout;
use crate::obj::ObjectId;
use crate::page::pager::PageNo;
use crate::page::{Page, PageType};
use crate::{Result, StoreError};

/// The units and the mapping onto them.
pub struct AspIo {
    devs: Vec<Box<dyn BlockDev>>,
    layout: Layout,
}

impl AspIo {
    /// Over `devs` in unit order.
    #[must_use]
    pub fn new(devs: Vec<Box<dyn BlockDev>>, layout: Layout) -> Self {
        Self { devs, layout }
    }

    /// The layout.
    #[must_use]
    pub fn layout(&self) -> &Layout {
        &self.layout
    }

    fn locate(&self, pno: PageNo) -> Result<(usize, u64)> {
        self.layout
            .locate(pno)
            .map(|(u, lba)| (usize::from(u), lba))
            .ok_or_else(|| StoreError::Corrupt {
                page: pno,
                what: "page number beyond the ASP".into(),
            })
    }

    /// Read and verify page `pno`.
    pub fn read_page(&mut self, pno: PageNo) -> Result<Page> {
        let (u, lba) = self.locate(pno)?;
        let mut page = Page::zeroed();
        self.devs[u].read(lba, page.bytes_mut())?;
        page.verify(pno)?;
        Ok(page)
    }

    /// Seal and write page `pno`.
    pub fn write_page(
        &mut self,
        pno: PageNo,
        page: &mut Page,
        kind: PageType,
        owner: ObjectId,
        generation: u64,
    ) -> Result<()> {
        page.seal(kind, owner, generation);
        self.write_raw(pno, page)
    }

    /// Write page `pno` as is, trailer included.
    pub fn write_raw(&mut self, pno: PageNo, page: &mut Page) -> Result<()> {
        let (u, lba) = self.locate(pno)?;
        self.devs[u].write(lba, page.bytes())?;
        Ok(())
    }

    /// Flush one unit.
    pub fn flush_unit(&mut self, unit: u16) -> Result<()> {
        self.devs[usize::from(unit)].flush()?;
        Ok(())
    }

    /// Flush every unit.
    pub fn flush(&mut self) -> Result<()> {
        for d in &self.devs {
            d.flush()?;
        }
        Ok(())
    }

    /// Tell the units a run is unused.
    pub fn discard(&mut self, start: PageNo, len: u64) -> Result<()> {
        let (u, lba) = self.locate(start)?;
        self.devs[u].discard(lba, len)?;
        Ok(())
    }
}
