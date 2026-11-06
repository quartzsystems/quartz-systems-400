// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Moving everything off a span so its unit can leave the ASP: object
//! headers, payload pages and overflow pages are copied to other spans,
//! and every B+tree with a node on the span is bulk-loaded again.

use tracing::info;

use crate::btree::{self, BTree};
use crate::obj::auth::ProfileKey;
use crate::obj::dir::{DirKey, LibDir, ObjDir, objdir_entries, objdir_update};
use crate::obj::library::Library;
use crate::obj::{ObjectId, ops, types, usridx};
use crate::page::PageType;
use crate::page::pager::{PageNo, Pager};
use crate::store::Store;
use crate::txn::Txn;
use crate::{Refusal, Result, StoreError};

/// What a drain moved.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DrainReport {
    /// Objects with a page on the span.
    pub objects_moved: usize,
    /// Pages copied.
    pub pages_moved: u64,
    /// Directory and index trees rebuilt.
    pub trees_rebuilt: usize,
}

/// How far, in percent used, a span may stand above the ASP's average
/// before balancing by capacity moves objects off it.
pub const BALANCE_TOLERANCE: f64 = 1.0;

/// Where balancing by capacity is in its pass over the objects.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BalanceCursor {
    /// The span being emptied towards the average.
    pub span: Option<u16>,
    /// The next object directory entry to look at.
    pub next: usize,
    /// Whether this pass moved anything.
    pub moved_this_pass: bool,
}

/// What one balancing step did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BalanceStep {
    /// Objects moved off the full span.
    pub objects_moved: usize,
    /// Pages copied.
    pub pages_moved: u64,
    /// The ASP is balanced, or nothing more can move.
    pub done: bool,
}

impl Store {
    /// Move everything off a unit's span and keep the allocator off it.
    /// Afterwards the unit holds nothing and can be removed. On failure
    /// the allocator may use the span again.
    pub fn drain_unit(&mut self, unit: u16) -> Result<DrainReport> {
        let span = self.asp().span_of(unit)?;
        self.asp_mut().exclude_unit(unit, true)?;
        let result = self.drain_span(span);
        if result.is_err() {
            let _ = self.asp_mut().exclude_unit(unit, false);
        }
        result
    }

    fn drain_span(&mut self, span: u16) -> Result<DrainReport> {
        // The allocator's own tree and log leave with a checkpoint.
        self.checkpoint()?;
        let mut report = DrainReport::default();
        let entries = {
            let mut t = self.begin()?;
            let e = objdir_entries(&mut t)?;
            t.abort();
            e
        };
        for (id, _) in entries {
            let mut txn = self.begin()?;
            let moved = move_object(&mut txn, span, id, &mut report)?;
            if moved {
                report.objects_moved += 1;
                txn.commit()?;
            } else {
                txn.abort();
            }
        }
        // The object directory itself.
        {
            let mut txn = self.begin()?;
            let dir = ObjDir::open(txn.objdir_root(), ObjectId::NONE);
            if any_on_span(&dir, &mut txn, span)? {
                let items: Vec<(ObjectId, PageNo)> =
                    dir.entries(&mut txn)?.collect::<Result<_>>()?;
                let new = ObjDir::bulk_load(&mut txn, ObjectId::NONE, items)?;
                let mut old = dir;
                old.free_all(&mut txn)?;
                txn.set_root(ObjectId::NONE, new.root());
                report.trees_rebuilt += 1;
                txn.commit()?;
            } else {
                txn.abort();
            }
        }
        self.checkpoint()?;
        let used = self.asp().allocator().span_used(span);
        if used > 0 {
            let left = self.leftovers(span);
            tracing::warn!("drain: span {span}: {left}");
            let unit = self.asp().layout().span(span).primary;
            return Err(StoreError::refused(unit, Refusal::CannotMove { pages: used, left }));
        }
        info!(
            "drain: span {span}: {} object(s), {} page(s), {} tree(s) moved",
            report.objects_moved, report.pages_moved, report.trees_rebuilt
        );
        Ok(report)
    }

    /// Each span's pages in use and its pages that can be used.
    #[must_use]
    pub fn span_fill(&self) -> Vec<(u16, u64, u64)> {
        let a = self.asp().allocator();
        self.asp()
            .layout()
            .spans()
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let span = u16::try_from(i).expect("at most MAX_UNITS");
                (span, a.span_used(span), s.pages)
            })
            .collect()
    }

    /// One step of balancing by capacity (`STRASPBAL TYPE(*CAPACITY)`):
    /// the span fuller than the ASP's average by more than
    /// [`BALANCE_TOLERANCE`] percent is kept from new allocations while
    /// up to `objects` objects with pages on it move off, onto the spans
    /// with the most room, as a drain moves them. `done` once no span is
    /// that much fuller than the average, there is one span only, or a
    /// whole pass over the objects moved nothing more.
    pub fn balance_step(&mut self, cursor: &mut BalanceCursor, objects: usize) -> Result<BalanceStep> {
        let mut step = BalanceStep::default();
        let Some(span) = self.fullest_span() else {
            step.done = true;
            return Ok(step);
        };
        if cursor.span != Some(span) {
            *cursor = BalanceCursor { span: Some(span), ..BalanceCursor::default() };
        }
        let entries = {
            let mut t = self.begin()?;
            let e = objdir_entries(&mut t)?;
            t.abort();
            e
        };
        let unit = self.asp().layout().span(span).primary;
        self.asp_mut().exclude_unit(unit, true)?;
        let result = self.balance_objects(span, &entries, cursor, objects, &mut step);
        let _ = self.asp_mut().exclude_unit(unit, false);
        result?;
        if cursor.next >= entries.len() {
            if !cursor.moved_this_pass {
                step.done = true;
            }
            cursor.next = 0;
            cursor.moved_this_pass = false;
        }
        Ok(step)
    }

    fn balance_objects(
        &mut self,
        span: u16,
        entries: &[(ObjectId, PageNo)],
        cursor: &mut BalanceCursor,
        objects: usize,
        step: &mut BalanceStep,
    ) -> Result<()> {
        let mut report = DrainReport::default();
        while cursor.next < entries.len() && step.objects_moved < objects {
            let (id, _) = entries[cursor.next];
            cursor.next += 1;
            let mut txn = self.begin()?;
            if move_object(&mut txn, span, id, &mut report)? {
                txn.commit()?;
                step.objects_moved += 1;
                cursor.moved_this_pass = true;
                if self.fullest_span() != Some(span) {
                    break;
                }
            } else {
                txn.abort();
            }
        }
        step.pages_moved = report.pages_moved;
        Ok(())
    }

    /// The span fuller than the average by more than the tolerance; the
    /// fullest when several are.
    #[allow(clippy::cast_precision_loss)] // page counts far below 2^52
    fn fullest_span(&self) -> Option<u16> {
        let fill = self.span_fill();
        if fill.len() < 2 {
            return None;
        }
        let used: u64 = fill.iter().map(|f| f.1).sum();
        let pages: u64 = fill.iter().map(|f| f.2).sum();
        let avg = used as f64 * 100.0 / pages.max(1) as f64;
        fill.iter()
            .map(|&(s, u, p)| (s, u as f64 * 100.0 / p.max(1) as f64))
            .filter(|(_, pct)| pct - avg > BALANCE_TOLERANCE)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(s, _)| s)
    }

    /// What still lies on a span: page counts by type and owner, for the
    /// log when a drain cannot finish.
    fn leftovers(&mut self, span: u16) -> String {
        use std::collections::BTreeMap;
        let pages = self.asp().layout().span(span).pages;
        let used: Vec<PageNo> = (crate::asp::layout::RESERVED_HEAD..pages - 1)
            .map(|lba| self.asp().layout().logical(span, lba))
            .filter(|&p| !self.asp().allocator().is_free(p))
            .collect();
        let tree: Vec<PageNo> = self.asp().allocator().tree_pages().to_vec();
        let log: Vec<PageNo> = self.asp().allocator().log_pages().to_vec();
        let mut by: BTreeMap<String, u64> = BTreeMap::new();
        for p in used.iter().take(4096) {
            let what = if tree.contains(p) {
                "allocator tree".to_string()
            } else if log.contains(p) {
                "allocator log".to_string()
            } else {
                match self.asp_mut().io().read_run(*p, 1) {
                    Ok(raw) if raw.iter().all(|&b| b == 0) => "zero page".to_string(),
                    Ok(raw) => {
                        let t = crate::page::trailer::Trailer::read(&raw);
                        format!("type {:#06x} owner {}", t.page_type, t.owner)
                    }
                    Err(e) => format!("unreadable: {e}"),
                }
            };
            *by.entry(what).or_default() += 1;
        }
        let mut parts: Vec<String> = by.into_iter().map(|(k, n)| format!("{n} x {k}")).collect();
        parts.truncate(12);
        format!("{} page(s) left: {}", used.len(), parts.join("; "))
    }
}

fn on_span(txn: &Txn<'_>, span: u16, pno: PageNo) -> bool {
    txn.layout().locate(pno).map(|(s, _)| s) == Some(span)
}

fn any_on_span<K, V>(tree: &BTree<K, V>, txn: &mut Txn<'_>, span: u16) -> Result<bool>
where
    K: crate::btree::FixedKey,
    V: crate::btree::Value,
{
    Ok(tree.pages(txn)?.into_iter().any(|p| on_span(txn, span, p)))
}

/// Copy one page to a fresh page off the span, keeping its type and owner.
fn copy_page(txn: &mut Txn<'_>, from: PageNo) -> Result<PageNo> {
    let to = txn.alloc()?;
    copy_into(txn, from, to)?;
    Ok(to)
}

/// Copy a page onto an allocated one. A page that was never written
/// (all zero, as a grown extent is before its first write) is left
/// unwritten at the destination too.
fn copy_into(txn: &mut Txn<'_>, from: PageNo, to: PageNo) -> Result<()> {
    // Through the transaction, so this transaction's own writes to the
    // page are what gets copied.
    let mut page = match txn.read(from) {
        Ok(p) => p,
        Err(e @ StoreError::Checksum { .. }) => {
            let raw = txn.store().asp_mut().io().read_run(from, 1)?;
            if raw.iter().all(|&b| b == 0) {
                return Ok(());
            }
            return Err(e);
        }
        Err(e) => return Err(e),
    };
    let t = page.trailer();
    let kind = PageType::from_u16(t.page_type).unwrap_or(PageType::ObjectData);
    txn.write(to, &mut page, kind, t.owner)
}

/// Move an object's pages off the span. Returns whether anything moved.
fn move_object(
    txn: &mut Txn<'_>,
    span: u16,
    id: ObjectId,
    report: &mut DrainReport,
) -> Result<bool> {
    let (page, mut h) = ops::read_header(txn, id)?;
    let mut moved = false;
    // Payload extents.
    let extents = ops::all_extents(txn, &h)?;
    let mut new_extents = Vec::with_capacity(extents.len());
    let mut extents_changed = false;
    for run in extents {
        if !on_span(txn, span, run.start) {
            new_extents.push(run);
            continue;
        }
        let mut from = run.start;
        let mut left = run.len;
        while left > 0 {
            let dst = txn.alloc_run(left)?;
            for i in 0..dst.len {
                copy_into(txn, from + i, dst.start + i)?;
            }
            new_extents.push(dst);
            from += dst.len;
            left -= dst.len;
        }
        txn.free_run(run.start, run.len)?;
        report.pages_moved += run.len;
        extents_changed = true;
        moved = true;
    }
    if extents_changed || h.extent_overflow.is_some_and(|p| on_span(txn, span, p)) {
        // save_extents rewrites the overflow page copy-on-write.
        ops::save_extents(txn, page, &mut h, &new_extents)?;
        moved = true;
    }
    // Private authority overflow tree.
    let privs: BTree<ProfileKey, u64> = BTree::open(h.private_overflow, id);
    if h.private_overflow.is_some() && any_on_span(&privs, txn, span)? {
        let items: Vec<(ProfileKey, u64)> = privs.entries(txn)?.collect::<Result<_>>()?;
        let new = BTree::bulk_load(txn, id, items)?;
        let mut old = privs;
        old.free_all(txn)?;
        h.private_overflow = new.root();
        ops::write_header(txn, page, &h)?;
        report.trees_rebuilt += 1;
        moved = true;
    }
    let rebuilt = move_slot_trees(txn, span, page, &mut h)?;
    report.trees_rebuilt += rebuilt;
    moved |= rebuilt > 0;
    // Trees the object owns.
    if h.id.type_code == types::LIB {
        let mut lib = Library::load(txn, id)?;
        let dir = lib.dir();
        if any_on_span(&dir, txn, span)? {
            let items: Vec<(DirKey, ObjectId)> = dir.entries(txn)?.collect::<Result<_>>()?;
            let new = LibDir::bulk_load(txn, id, items)?;
            let mut old = dir;
            old.free_all(txn)?;
            lib.attr.dir_root = new.root();
            lib.save(txn)?;
            report.trees_rebuilt += 1;
            moved = true;
        }
    } else if h.id.type_code == types::USRIDX {
        let mut attr = usridx::describe(txn, id)?;
        let tree: BTree<usridx::IdxKey, Vec<u8>> = BTree::open(attr.root, id);
        let mut items: Vec<(usridx::IdxKey, Vec<u8>)> =
            tree.entries(txn)?.collect::<Result<_>>()?;
        // A long entry whose overflow pages touch the span is written
        // again elsewhere; its value changes, so the tree is rebuilt.
        let mut values_moved = false;
        for (_, raw) in &mut items {
            let pages = usridx::overflow_pages(txn, raw)?;
            if pages.iter().any(|p| on_span(txn, span, *p)) {
                let data = usridx::decode_value(txn, raw)?;
                for p in pages {
                    txn.free_run(p, 1)?;
                    report.pages_moved += 1;
                }
                *raw = usridx::encode_value(txn, id, &data)?;
                values_moved = true;
            }
        }
        if values_moved || any_on_span(&tree, txn, span)? {
            let new = BTree::bulk_load(txn, id, items)?;
            let mut old = tree;
            old.free_all(txn)?;
            attr.root = new.root();
            usridx::write_attr(txn, page, &attr)?;
            report.trees_rebuilt += 1;
            moved = true;
        }
    }
    // The header page last: everything above wrote through it.
    if on_span(txn, span, page) {
        let to = copy_page(txn, page)?;
        txn.free_run(page, 1)?;
        objdir_update(txn, id, to)?;
        report.pages_moved += 1;
        moved = true;
    }
    Ok(moved)
}

/// Rebuild off the span every tree in the header's root slots (profile
/// indexes, authorization lists, member paths) with a page on it, from
/// its raw entries, whatever its key type, as a restore loads them.
/// Returns how many were rebuilt.
fn move_slot_trees(
    txn: &mut Txn<'_>,
    span: u16,
    page: PageNo,
    h: &mut crate::obj::header::ObjectHeader,
) -> Result<usize> {
    let mut rebuilt = 0;
    for slot in 0..h.tree_roots.len() {
        let Some(root) = h.tree_roots[slot] else {
            continue;
        };
        if !btree::walk_pages(txn, root)?.into_iter().any(|p| on_span(txn, span, p)) {
            continue;
        }
        let (key_len, items) = btree::dump(txn, root)?;
        let new = btree::load(txn, h.id, key_len, items)?;
        btree::free_tree(txn, root)?;
        ops::set_tree_root(txn, page, slot, new)?;
        h.tree_roots[slot] = new;
        rebuilt += 1;
    }
    Ok(rebuilt)
}
