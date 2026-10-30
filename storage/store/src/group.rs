// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Group commit: transactions committed while a group is open merge
//! into one WAL record, made durable together when the group is flushed
//! (`docs/STORE_FORMAT.md`, "WAL"). Their in-place writes wait for that
//! record; until then reads find the pages here.

use std::collections::BTreeMap;

use crate::page::Page;
use crate::page::pager::PageNo;
use crate::wal::record::WalRecord;

/// Most WAL pages one group's record takes (1 MiB).
pub(crate) const MAX_PAGES: u64 = 256;

/// The transactions of an open group not yet durable.
#[derive(Default)]
pub(crate) struct Group {
    /// Their merged record; `None` while the group is empty.
    pub(crate) record: Option<WalRecord>,
    /// Their in-place pages, written once the record is durable.
    pub(crate) held: BTreeMap<PageNo, Page>,
}

impl Group {
    /// Whether `r` can join without the merged record outgrowing
    /// `room` bytes or a count field of the record header.
    pub(crate) fn fits(&self, r: &WalRecord, room: usize) -> bool {
        let Some(g) = &self.record else {
            return r.encoded_len() <= room;
        };
        let max = usize::from(u16::MAX);
        g.encoded_len() + r.encoded_len() <= room
            && g.roots.len() + r.roots.len() <= max
            && g.allocs.len() + r.allocs.len() <= max
            && g.frees.len() + r.frees.len() <= max
            && g.images.len() + r.images.len() <= max
            && g.patches.len() + r.patches.len() <= max
    }

    /// Add `r` after the records already merged. Replay applies every
    /// allocation, then every free, root, image and patch, each in
    /// commit order. That equals replaying the records one by one: a
    /// page freed since the checkpoint is not allocated again before
    /// it, and a page is imaged once per checkpoint, before any patch.
    pub(crate) fn merge(&mut self, r: &WalRecord) {
        let Some(g) = self.record.as_mut() else {
            self.record = Some(r.clone());
            return;
        };
        g.txn = r.txn;
        g.next_serial = r.next_serial;
        g.roots.extend(r.roots.iter().copied());
        g.allocs.extend(r.allocs.iter().copied());
        g.frees.extend(r.frees.iter().copied());
        g.images.extend(r.images.iter().cloned());
        g.patches.extend(r.patches.iter().cloned());
    }
}
