// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RCLSTG: sweep every page of every unit, verify every checksum, and
//! either report (`*CHECK`) or rebuild the object directory, the library
//! directories and the allocator from the object headers, marking
//! objects with bad pages damaged and moving objects whose library is
//! gone to QRCL.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use qsname::Name;
use tracing::info;

use crate::alloc::{Extent, ReadOnly};
use crate::asp::layout::RESERVED_HEAD;
use crate::asp::unit::UnitState;
use crate::btree::BTree;
use crate::obj::dir::{DirKey, LibDir, ObjDir};
use crate::obj::header::{self, ATTR_OFF, ObjectHeader, text};
use crate::obj::library::{LibAttr, LibType};
use crate::obj::{ObjectId, ops, types, usridx};
use crate::page::pager::{PageNo, Pager};
use crate::page::trailer::{self, Trailer};
use crate::page::{PAGE_SIZE, Page, PageType};
use crate::store::Store;
use crate::{Result, StoreError};

/// What to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReclaimMode {
    /// `*ALL`: verify and rebuild.
    All,
    /// `*SYSBAS`: the same as `*ALL` in Phase 2.
    SysBas,
    /// `*CHECK`: verify only, write nothing.
    Check,
}

/// A page that failed verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BadPage {
    /// The page.
    pub page: PageNo,
    /// Owner from its trailer (may itself be corrupt).
    pub owner: ObjectId,
    /// What was wrong.
    pub what: String,
}

/// What a reclaim found and did.
#[derive(Debug, Clone, Default)]
pub struct ReclaimReport {
    /// Pages swept.
    pub pages_swept: u64,
    /// Pages the allocator calls allocated.
    pub pages_allocated: u64,
    /// Pages with a bad checksum or contents.
    pub bad_pages: Vec<BadPage>,
    /// Live objects (headers found on allocated pages).
    pub objects: usize,
    /// Valid headers on free pages: deleted objects, ignored.
    pub stale_headers: usize,
    /// Libraries.
    pub libraries: usize,
    /// Objects marked damaged.
    pub damaged: usize,
    /// Objects moved to QRCL.
    pub moved_to_qrcl: usize,
    /// Owned objects (file members) removed because their owner is gone.
    pub owned_removed: usize,
    /// Headers missing from the object directory, or at the wrong page.
    pub objdir_missing: usize,
    /// Object directory entries with no header.
    pub objdir_extra: usize,
    /// Objects missing from their library's directory.
    pub libdir_missing: usize,
    /// Library directory entries with no object.
    pub libdir_extra: usize,
    /// Pages allocated but referenced by nothing.
    pub leaked_pages: u64,
    /// Pages referenced but free.
    pub lost_pages: u64,
    /// Free pages before.
    pub free_before: u64,
    /// Free pages after (the same in `*CHECK`).
    pub free_after: u64,
    /// Whether directories and the allocator were rewritten.
    pub rebuilt: bool,
}

impl ReclaimReport {
    /// Everything that was wrong, as a count.
    #[must_use]
    pub fn problems(&self) -> usize {
        self.bad_pages.len()
            + self.damaged
            + self.moved_to_qrcl
            + self.owned_removed
            + self.objdir_missing
            + self.objdir_extra
            + self.libdir_missing
            + self.libdir_extra
            + usize::try_from(self.leaked_pages + self.lost_pages).unwrap_or(usize::MAX)
    }
}

impl fmt::Display for ReclaimReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "pages swept {}, allocated {}, bad {}",
            self.pages_swept,
            self.pages_allocated,
            self.bad_pages.len()
        )?;
        for b in self.bad_pages.iter().take(20) {
            writeln!(f, "  page {} owner {}: {}", b.page, b.owner, b.what)?;
        }
        writeln!(
            f,
            "objects {}, libraries {}, stale headers {}, damaged {}, moved to QRCL {}",
            self.objects, self.libraries, self.stale_headers, self.damaged, self.moved_to_qrcl
        )?;
        writeln!(
            f,
            "object directory missing {} extra {}; library directories missing {} extra {}",
            self.objdir_missing, self.objdir_extra, self.libdir_missing, self.libdir_extra
        )?;
        writeln!(
            f,
            "pages leaked {} lost {}; free before {} after {}; {}",
            self.leaked_pages,
            self.lost_pages,
            self.free_before,
            self.free_after,
            if self.rebuilt {
                "rebuilt"
            } else {
                "not rebuilt"
            }
        )?;
        write!(f, "{} problem(s)", self.problems())
    }
}

/// What the sweep knows about an allocated page.
#[derive(Clone, Copy)]
struct Tag {
    kind: u16,
    owner: ObjectId,
    ok: bool,
}

/// The sweep's findings.
struct Sweep {
    /// By logical page; `None` for free and reserved pages.
    tags: Vec<Option<Tag>>,
    /// Live headers.
    headers: Vec<(PageNo, ObjectHeader)>,
    /// Reserved and metadata pages the allocator must keep.
    fixed: Vec<PageNo>,
}

fn to_usize(p: PageNo) -> usize {
    usize::try_from(p).expect("page fits")
}

impl Store {
    /// Verify every allocated page's checksum. Writes nothing.
    pub fn check_pages(&mut self) -> Result<ReclaimReport> {
        self.checkpoint()?;
        let mut report = ReclaimReport::default();
        self.sweep(&mut report)?;
        report.free_before = self.asp().total_free();
        report.free_after = report.free_before;
        Ok(report)
    }

    /// Reclaim storage. See the module documentation.
    pub fn reclaim(&mut self, mode: ReclaimMode, now: u64) -> Result<ReclaimReport> {
        self.checkpoint()?;
        let mut report = ReclaimReport {
            free_before: self.asp().total_free(),
            ..ReclaimReport::default()
        };
        let sweep = self.sweep(&mut report)?;
        let total = self.asp().layout().total_pages();
        let mut referenced = vec![false; to_usize(total)];
        for &p in &sweep.fixed {
            referenced[to_usize(p)] = true;
        }
        let mut headers: BTreeMap<ObjectId, (PageNo, ObjectHeader)> = BTreeMap::new();
        for (pno, h) in &sweep.headers {
            referenced[to_usize(*pno)] = true;
            headers.insert(h.id, (*pno, h.clone()));
        }
        report.objects = headers.len();
        report.libraries = headers
            .values()
            .filter(|(_, h)| h.id.type_code == types::LIB)
            .count();

        // Payload pages: extents, overflow pages, user index trees.
        let mut damaged: Vec<ObjectId> = Vec::new();
        for (pno, h) in &sweep.headers {
            let reasons = self.verify_payload(&sweep, h, &mut referenced)?;
            if let Some(why) = reasons {
                info!("rclstg: {} at page {pno} is damaged: {why}", h.name);
                if !h.damaged {
                    damaged.push(h.id);
                }
            }
        }

        // Directories as they are, for the diff. Their pages count as
        // referenced here; a rebuild gives them back below.
        let mut dir_pages: Vec<PageNo> = Vec::new();
        self.diff_objdir(&headers, &mut report, &mut referenced, &mut dir_pages);
        self.diff_libdirs(&headers, &mut report, &mut referenced, &mut dir_pages)?;

        // Allocator against the references.
        for pno in 0..total {
            let allocated = !self.asp().allocator().is_free(pno);
            let expected = referenced[to_usize(pno)];
            if allocated && !expected {
                report.leaked_pages += 1;
            } else if expected && !allocated {
                report.lost_pages += 1;
            }
        }
        report.damaged = damaged.len();
        if mode == ReclaimMode::Check {
            report.free_after = report.free_before;
            return Ok(report);
        }

        // Rebuild: the allocator first, so the new trees land on pages
        // nothing references. The old directory pages are free now.
        for p in dir_pages {
            referenced[to_usize(p)] = false;
        }
        let free = free_runs(self.asp().layout(), &referenced);
        self.asp_mut().allocator_mut().rebuild(&free)?;
        let qsys = self.asp().superblock().qsys;
        if !headers.contains_key(&qsys) {
            return Err(StoreError::Corrupt {
                page: 0,
                what: "QSYS header not found; the store cannot be rebuilt".into(),
            });
        }
        let moved = self.rebuild_directories(headers, &damaged, qsys, now, &mut report)?;
        report.moved_to_qrcl = moved;
        {
            let sb = self.asp_mut().superblock_mut();
            sb.flags &= !crate::asp::superblock::FLAG_RCLSTG_REQUIRED;
            sb.last_rclstg_time = now;
        }
        self.checkpoint()?;
        report.rebuilt = true;
        report.free_after = self.asp().total_free();
        Ok(report)
    }

    /// A run of pages, and the same run from the other half when the
    /// span is an active pair: both must agree page for page.
    fn read_both_halves(
        &mut self,
        span: &crate::asp::layout::UnitSpan,
        start: PageNo,
        count: u64,
    ) -> Result<(blockdev::AlignedBuf, Option<blockdev::AlignedBuf>)> {
        let both_active = span.partner.is_some_and(|p| {
            self.asp().unit_state(span.primary) == UnitState::Active
                && self.asp().unit_state(p) == UnitState::Active
        });
        let io = self.asp_mut().io();
        if both_active {
            let partner = span.partner.expect("checked");
            if let Some(primary) = io.read_run_on(span.primary, start, count)? {
                let other = io.read_run_on(partner, start, count)?;
                return Ok((primary, other));
            }
        }
        Ok((io.read_run(start, count)?, None))
    }

    /// Read every page of every unit and classify the allocated ones.
    fn sweep(&mut self, report: &mut ReclaimReport) -> Result<Sweep> {
        let layout = self.asp().layout().clone();
        let wal_start = self.asp().superblock().wal_start;
        let wal_pages = self.asp().superblock().wal_pages;
        let total = layout.total_pages();
        let mut tags: Vec<Option<Tag>> = vec![None; to_usize(total)];
        let mut headers = Vec::new();
        let mut fixed = Vec::new();
        for (unit, span) in layout.spans().iter().enumerate() {
            let unit16 = u16::try_from(unit).expect("unit");
            let mut lba = 0;
            while lba < span.pages {
                let count = (span.pages - lba).min(256);
                let start = layout.logical(unit16, lba);
                let (buf, other) = self.read_both_halves(span, start, count)?;
                for i in 0..count {
                    let pno = start + i;
                    let at = lba + i;
                    let reserved = at < RESERVED_HEAD
                        || at == span.pages - 1
                        || (unit == 0 && at >= wal_start && at < wal_start + wal_pages);
                    report.pages_swept += 1;
                    if reserved {
                        fixed.push(pno);
                        continue;
                    }
                    let bytes = &buf[to_usize(i) * PAGE_SIZE..(to_usize(i) + 1) * PAGE_SIZE];
                    let t = Trailer::read(bytes);
                    let crc_ok = trailer::checksum(bytes) == trailer::stored_crc(bytes);
                    let allocated = !self.asp().allocator().is_free(pno);
                    if !allocated {
                        if crc_ok && t.page_type == PageType::ObjectHeader.as_u16() {
                            report.stale_headers += 1;
                        }
                        continue;
                    }
                    report.pages_allocated += 1;
                    if let Some(o) = other.as_ref()
                        && o[to_usize(i) * PAGE_SIZE..(to_usize(i) + 1) * PAGE_SIZE] != *bytes
                    {
                        report.bad_pages.push(BadPage {
                            page: pno,
                            owner: t.owner,
                            what: "mirrored halves differ".into(),
                        });
                        tags[to_usize(pno)] = Some(Tag {
                            kind: t.page_type,
                            owner: t.owner,
                            ok: false,
                        });
                        continue;
                    }
                    if !crc_ok {
                        report.bad_pages.push(BadPage {
                            page: pno,
                            owner: t.owner,
                            what: "checksum".into(),
                        });
                        tags[to_usize(pno)] = Some(Tag {
                            kind: t.page_type,
                            owner: t.owner,
                            ok: false,
                        });
                        continue;
                    }
                    tags[to_usize(pno)] = Some(Tag {
                        kind: t.page_type,
                        owner: t.owner,
                        ok: true,
                    });
                    if t.page_type == PageType::ObjectHeader.as_u16() {
                        match ObjectHeader::decode(&bytes[..crate::page::BODY_SIZE], pno) {
                            Ok(h) if h.id == t.owner => headers.push((pno, h)),
                            Ok(h) => report.bad_pages.push(BadPage {
                                page: pno,
                                owner: t.owner,
                                what: format!("header of {} owned by {}", h.id, t.owner),
                            }),
                            Err(e) => report.bad_pages.push(BadPage {
                                page: pno,
                                owner: t.owner,
                                what: e.to_string(),
                            }),
                        }
                    }
                }
                lba += count;
            }
        }
        // The current free tree and log stay allocated until the next
        // checkpoint releases them.
        fixed.extend_from_slice(self.asp().allocator().tree_pages());
        fixed.extend_from_slice(self.asp().allocator().log_pages());
        Ok(Sweep {
            tags,
            headers,
            fixed,
        })
    }

    /// Check an object's payload pages and mark them referenced (a bad
    /// page is still the object's). Returns why the object is damaged,
    /// if it is.
    fn verify_payload(
        &mut self,
        sweep: &Sweep,
        h: &ObjectHeader,
        referenced: &mut [bool],
    ) -> Result<Option<String>> {
        let mut why: Option<String> = None;
        let mut note = |w: String| {
            if why.is_none() {
                why = Some(w);
            }
        };
        let mut extents = h.extents.clone();
        if let Some(p) = h.extent_overflow {
            if let Some(w) = check_page(sweep, h.id, referenced, p, PageType::ExtentOverflow) {
                note(w);
            } else {
                let page = self.asp_mut().io().read_page(p)?;
                match header::decode_overflow(page.body(), p) {
                    Ok(more) => extents.extend(more),
                    Err(e) => note(e.to_string()),
                }
            }
        }
        if let Some(root) = h.private_overflow {
            let mut ro = ReadOnly(self.asp_mut().io());
            match crate::btree::walk_pages(&mut ro, root) {
                Ok(pages) => {
                    for p in pages {
                        if let Some(w) = check_page(sweep, h.id, referenced, p, PageType::BTreeNode)
                        {
                            note(w);
                        }
                    }
                }
                Err(e) => note(format!("private authority tree: {e}")),
            }
        }
        for run in &extents {
            for pno in run.start..run.start.saturating_add(run.len) {
                if let Some(w) = check_page(sweep, h.id, referenced, pno, PageType::ObjectData) {
                    note(w);
                }
            }
        }
        if h.id.type_code == types::USRIDX {
            for w in self.verify_index(sweep, h, referenced)? {
                note(w);
            }
        }
        for root in h.tree_roots.into_iter().flatten() {
            let mut ro = ReadOnly(self.asp_mut().io());
            match crate::btree::walk_pages(&mut ro, root) {
                Ok(pages) => {
                    for p in pages {
                        if let Some(w) = check_page(sweep, h.id, referenced, p, PageType::BTreeNode)
                        {
                            note(w);
                        }
                    }
                }
                Err(e) => note(format!("tree at {root}: {e}")),
            }
        }
        Ok(why)
    }

    /// Check a user index's tree pages and its long entries' overflow
    /// pages, marking them referenced; what was wrong with them.
    fn verify_index(
        &mut self,
        sweep: &Sweep,
        h: &ObjectHeader,
        referenced: &mut [bool],
    ) -> Result<Vec<String>> {
        let mut wrong = Vec::new();
        let root = {
            let page = self
                .asp_mut()
                .io()
                .read_page(sweep_header_page(sweep, h.id))?;
            let a = &page.body()[ATTR_OFF..ATTR_OFF + usridx::ATTR_LEN];
            let root = u64::from_le_bytes(a[8..16].try_into().expect("8"));
            (root != 0).then_some(root)
        };
        let tree: BTree<usridx::IdxKey, Vec<u8>> = BTree::open(root, h.id);
        let mut ro = ReadOnly(self.asp_mut().io());
        match tree.pages(&mut ro) {
            Ok(pages) => {
                for p in pages {
                    if let Some(w) = check_page(sweep, h.id, referenced, p, PageType::BTreeNode) {
                        wrong.push(w);
                    }
                }
            }
            Err(e) => wrong.push(format!("index tree: {e}")),
        }
        let raws: Result<Vec<Vec<u8>>> = tree
            .entries(&mut ro)
            .and_then(|it| it.map(|r| r.map(|(_, v)| v)).collect());
        let raws = match raws {
            Ok(raws) => raws,
            Err(e) => {
                wrong.push(format!("index entries: {e}"));
                return Ok(wrong);
            }
        };
        for raw in &raws {
            match usridx::overflow_pages(&mut ro, raw) {
                Ok(pages) => {
                    for p in pages {
                        if let Some(w) =
                            check_page(sweep, h.id, referenced, p, PageType::IndexOverflow)
                        {
                            wrong.push(w);
                        }
                    }
                }
                Err(e) => wrong.push(format!("index entry overflow: {e}")),
            }
        }
        Ok(wrong)
    }

    fn diff_objdir(
        &mut self,
        headers: &BTreeMap<ObjectId, (PageNo, ObjectHeader)>,
        report: &mut ReclaimReport,
        referenced: &mut [bool],
        dir_pages: &mut Vec<PageNo>,
    ) {
        let dir = ObjDir::open(self.objdir_root(), ObjectId::NONE);
        let mut ro = ReadOnly(self.asp_mut().io());
        let entries: HashMap<ObjectId, PageNo> = match dir.entries(&mut ro) {
            Ok(it) => it.filter_map(Result::ok).collect(),
            Err(_) => HashMap::new(),
        };
        if let Ok(pages) = dir.pages(&mut ro) {
            for p in pages {
                referenced[to_usize(p)] = true;
                dir_pages.push(p);
            }
        }
        for (id, (pno, _)) in headers {
            if entries.get(id) != Some(pno) {
                report.objdir_missing += 1;
            }
        }
        report.objdir_extra = entries
            .keys()
            .filter(|id| !headers.contains_key(id))
            .count();
    }

    fn diff_libdirs(
        &mut self,
        headers: &BTreeMap<ObjectId, (PageNo, ObjectHeader)>,
        report: &mut ReclaimReport,
        referenced: &mut [bool],
        dir_pages: &mut Vec<PageNo>,
    ) -> Result<()> {
        for (lib_id, (lib_pno, lib_h)) in headers {
            if lib_h.id.type_code != types::LIB {
                continue;
            }
            let page = self.asp_mut().io().read_page(*lib_pno)?;
            let attr = LibAttr::decode(page.body());
            let dir = LibDir::open(attr.dir_root, *lib_id);
            let mut ro = ReadOnly(self.asp_mut().io());
            let entries: HashMap<DirKey, ObjectId> = match dir.entries(&mut ro) {
                Ok(it) => it.filter_map(Result::ok).collect(),
                Err(_) => HashMap::new(),
            };
            if let Ok(pages) = dir.pages(&mut ro) {
                for p in pages {
                    referenced[to_usize(p)] = true;
                    dir_pages.push(p);
                }
            }
            let members: HashMap<DirKey, ObjectId> = headers
                .values()
                .filter(|(_, h)| h.library == *lib_id)
                .map(|(_, h)| (DirKey::new(h.id.type_code, h.name), h.id))
                .collect();
            report.libdir_missing += members
                .iter()
                .filter(|(k, id)| entries.get(k) != Some(id))
                .count();
            report.libdir_extra += entries
                .iter()
                .filter(|(k, id)| members.get(k) != Some(id))
                .count();
        }
        Ok(())
    }

    /// Mark damaged objects, move orphans to QRCL, and bulk-load the
    /// object directory and every library directory. Returns the number
    /// moved.
    fn rebuild_directories(
        &mut self,
        mut headers: BTreeMap<ObjectId, (PageNo, ObjectHeader)>,
        damaged: &[ObjectId],
        qsys: ObjectId,
        now: u64,
        report: &mut ReclaimReport,
    ) -> Result<usize> {
        let asp = self.asp_number();
        let mut txn = self.begin()?;
        for id in damaged {
            if let Some((pno, h)) = headers.get_mut(id) {
                h.damaged = true;
                ops::write_header(&mut txn, *pno, h)?;
            }
        }
        let libraries: Vec<ObjectId> = headers
            .values()
            .filter(|(_, h)| h.id.type_code == types::LIB)
            .map(|(_, h)| h.id)
            .collect();
        // QRCL: an existing library of that name, else a new one.
        let qrcl_name = Name::parse("QRCL").expect("valid");
        // An object whose `library` is another live object is owned by
        // it (a file member) and belongs to no library.
        let orphans: Vec<ObjectId> = headers
            .values()
            .filter(|(_, h)| {
                h.id != h.library
                    && !libraries.contains(&h.library)
                    && !(h.library.type_code != types::LIB && headers.contains_key(&h.library))
            })
            .map(|(_, h)| h.id)
            .collect();
        // Owned objects whose owner is gone are removed, not moved.
        let (lost_owned, orphans): (Vec<ObjectId>, Vec<ObjectId>) = orphans
            .into_iter()
            .partition(|id| id.type_code == types::MBR);
        for id in lost_owned {
            let (pno, h) = headers.remove(&id).expect("listed");
            remove_owned(&mut txn, pno, &h)?;
            report.owned_removed += 1;
        }
        let mut moved = 0;
        if !orphans.is_empty() {
            let existing = headers
                .values()
                .find(|(_, h)| h.id.type_code == types::LIB && h.name == qrcl_name)
                .map(|(_, h)| h.id);
            let qrcl = if let Some(id) = existing {
                id
            } else {
                {
                    let id = ObjectId {
                        asp,
                        type_code: types::LIB,
                        serial: txn.take_serial(),
                        generation: 0,
                    };
                    let pno = txn.alloc()?;
                    let mut h = ObjectHeader::new(id, qsys, qrcl_name, now);
                    h.text = text("Reclaimed objects")?;
                    h.owner = text("QSYS")?;
                    h.creator = text("QSYS")?;
                    let mut page = Page::zeroed();
                    h.encode(page.body_mut());
                    LibAttr::new(LibType::Prod, asp)?.encode(page.body_mut());
                    txn.write(pno, &mut page, PageType::ObjectHeader, id)?;
                    headers.insert(id, (pno, h));
                    report.libraries += 1;
                    id
                }
            };
            for id in orphans {
                let (pno, h) = headers.get_mut(&id).expect("listed");
                info!("rclstg: {} ({}) moved to QRCL", h.name, h.id);
                h.library = qrcl;
                h.changed = now;
                ops::write_header(&mut txn, *pno, h)?;
                moved += 1;
            }
        }
        // Object directory.
        let objdir = ObjDir::bulk_load(
            &mut txn,
            ObjectId::NONE,
            headers.iter().map(|(id, (pno, _))| (*id, *pno)),
        )?;
        txn.set_root(ObjectId::NONE, objdir.root());
        // Library directories.
        let mut by_lib: BTreeMap<ObjectId, BTreeMap<DirKey, ObjectId>> = BTreeMap::new();
        for (_, h) in headers.values() {
            if h.id.type_code == types::LIB {
                by_lib.entry(h.id).or_default();
            }
        }
        for (_, h) in headers.values() {
            if let Some(members) = by_lib.get_mut(&h.library) {
                members.insert(DirKey::new(h.id.type_code, h.name), h.id);
            }
        }
        for (lib, members) in by_lib {
            let (pno, _) = headers[&lib];
            let page = txn.read(pno)?;
            let mut attr = LibAttr::decode(page.body());
            let count = members.len() as u64;
            let dir = LibDir::bulk_load(&mut txn, lib, members)?;
            attr.dir_root = dir.root();
            attr.object_count = count;
            txn.patch(pno, ATTR_OFF, &attr.to_bytes())?;
        }
        txn.commit()?;
        Ok(moved)
    }
}

/// Free everything an owned object holds: its pages, trees and header.
fn remove_owned(txn: &mut crate::txn::Txn<'_>, pno: PageNo, h: &ObjectHeader) -> Result<()> {
    info!(
        "rclstg: member {} ({}) of a missing file removed",
        h.name, h.id
    );
    for run in &h.extents {
        txn.free_run(run.start, run.len)?;
    }
    if let Some(p) = h.extent_overflow {
        txn.free_run(p, 1)?;
    }
    if let Some(root) = h.private_overflow {
        crate::btree::free_tree(txn, root)?;
    }
    for root in h.tree_roots.into_iter().flatten() {
        crate::btree::free_tree(txn, root)?;
    }
    txn.free_run(pno, 1)
}

/// Whether `pno` is a sound `kind` page of `owner`; the page is marked
/// referenced whenever it lies in the ASP.
fn check_page(
    sweep: &Sweep,
    owner: ObjectId,
    referenced: &mut [bool],
    pno: PageNo,
    kind: PageType,
) -> Option<String> {
    let total = sweep.tags.len() as u64;
    if pno >= total {
        return Some(format!("page {pno} beyond the ASP"));
    }
    referenced[to_usize(pno)] = true;
    match sweep.tags[to_usize(pno)] {
        None => Some(format!("page {pno} is free")),
        Some(t) if !t.ok => Some(format!("page {pno} is bad")),
        Some(t) if t.owner != owner => Some(format!("page {pno} belongs to {}", t.owner)),
        Some(t) if t.kind != kind.as_u16() => Some(format!("page {pno} is not {kind:?}")),
        Some(_) => None,
    }
}

fn sweep_header_page(sweep: &Sweep, id: ObjectId) -> PageNo {
    sweep
        .headers
        .iter()
        .find(|(_, h)| h.id == id)
        .map(|(p, _)| *p)
        .expect("header listed")
}

/// Allocatable pages that nothing references, as runs in unit
/// coordinates.
fn free_runs(layout: &crate::asp::layout::Layout, referenced: &[bool]) -> Vec<Extent> {
    let mut out = Vec::new();
    for (unit, span) in layout.spans().iter().enumerate() {
        let unit16 = u16::try_from(unit).expect("unit");
        let mut run: Option<Extent> = None;
        for lba in RESERVED_HEAD..span.pages - 1 {
            let pno = layout.logical(unit16, lba);
            if referenced[to_usize(pno)] {
                if let Some(r) = run.take() {
                    out.push(r);
                }
            } else {
                match run.as_mut() {
                    Some(r) => r.len += 1,
                    None => {
                        run = Some(Extent {
                            unit: unit16,
                            lba,
                            len: 1,
                        });
                    }
                }
            }
        }
        if let Some(r) = run {
            out.push(r);
        }
    }
    out
}
