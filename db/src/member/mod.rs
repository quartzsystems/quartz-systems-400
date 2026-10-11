// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Members: `*MBR` objects owned by a file, their record slots and the
//! free-slot tree. The operations here are the raw slot operations; the
//! open data path adds positioning, locks and journaling on top.

pub mod attr;
pub mod slots;

pub use attr::MbrAttr;
pub use slots::Slot;

use store::PageNo;
use store::btree::{self, BTree};
use store::obj::header::TREE_ROOT_SLOTS;
use store::obj::ops::{self, CreateParams};
use store::obj::space::Space;
use store::obj::{JobHandle, ObjectId, types};
use store::txn::Txn;

use crate::error::{DbError, Result};
use crate::format::FormatDef;

/// Bytes a space grows by when `SIZE` gives no increment.
const DEFAULT_GROWTH_BYTES: u64 = 16 * 4064;

/// Free-slot tree: RRNs of deleted slots, no values.
type FreeTree = BTree<u64, ()>;

/// A member loaded for one transaction's work.
#[derive(Clone, Debug)]
pub struct Member {
    /// The `*MBR` object.
    pub id: ObjectId,
    /// Its header page.
    pub page: PageNo,
    /// The attributes.
    pub attr: MbrAttr,
    /// Header tree roots: slot 0 the access path, slot 1 the free slots.
    pub roots: [Option<PageNo>; TREE_ROOT_SLOTS],
    /// The member's name, for messages.
    pub name: qsname::Name,
}

/// What a new member takes from its file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemberParams {
    /// Index of the format the member holds.
    pub format_index: u16,
    /// Most records, 0 for no limit.
    pub max_records: u64,
    /// `SIZE` increment in records, 0 for the default growth.
    pub increment: u32,
    /// `REUSEDLT(*YES)`.
    pub reuse_deleted: bool,
}

/// Create a member of `file` for `format`, owned by the file.
pub fn create(
    txn: &mut Txn<'_>,
    file: ObjectId,
    format: &FormatDef,
    mp: MemberParams,
    params: &CreateParams,
) -> Result<ObjectId> {
    let params = CreateParams {
        type_code: types::MBR,
        ..params.clone()
    };
    let id = ops::create_owned(txn, &params, file)?;
    let slot_len = format.slot_len();
    let increment_slots = if mp.increment > 0 {
        u64::from(mp.increment)
    } else {
        (DEFAULT_GROWTH_BYTES / u64::from(slot_len)).max(1)
    };
    let attr = MbrAttr {
        path_invalid: false,
        reuse_deleted: mp.reuse_deleted,
        slot_len,
        record_count: 0,
        deleted_count: 0,
        slots: 0,
        max_records: mp.max_records,
        increment_slots,
        path_entries: 0,
        key_len: format.key_len,
        format: mp.format_index,
        null_map_len: format.null_map_len,
        file,
        last_save: 0,
        last_restore: 0,
    };
    let (page, _) = ops::read_header(txn, id)?;
    ops::write_attr(txn, page, 0, &attr.encode())?;
    Ok(id)
}

/// Delete a member object with everything it holds.
pub fn destroy(txn: &mut Txn<'_>, job: JobHandle, id: ObjectId) -> Result<()> {
    ops::delete(txn, job, id)?;
    Ok(())
}

impl Member {
    /// Load a member.
    pub fn load(txn: &mut Txn<'_>, id: ObjectId) -> Result<Self> {
        let (page, h) = ops::read_header(txn, id)?;
        if h.id.type_code != types::MBR {
            return Err(DbError::Corrupt {
                what: format!("{} is not a member", h.name),
            });
        }
        let a = ops::read_attr(txn, page)?;
        Ok(Self {
            id,
            page,
            attr: MbrAttr::decode(&a[..attr::ATTR_LEN])?,
            roots: h.tree_roots,
            name: h.name,
        })
    }

    fn save(&self, txn: &mut Txn<'_>) -> Result<()> {
        ops::write_attr(txn, self.page, 0, &self.attr.encode())?;
        Ok(())
    }

    fn set_root(&mut self, txn: &mut Txn<'_>, slot: usize, root: Option<PageNo>) -> Result<()> {
        if self.roots[slot] != root {
            ops::set_tree_root(txn, self.page, slot, root)?;
            self.roots[slot] = root;
        }
        Ok(())
    }

    /// Bytes of null map per record.
    #[must_use]
    pub fn null_map_len(&self) -> usize {
        usize::from(self.attr.null_map_len)
    }

    /// Bytes of record data per slot.
    #[must_use]
    pub fn record_len(&self) -> usize {
        usize::try_from(self.attr.slot_len).expect("fits") - 1 - self.null_map_len()
    }

    fn check(&self, record: &[u8], null_map: &[u8]) -> Result<()> {
        if record.len() != self.record_len() {
            return Err(DbError::RecordLength {
                want: self.record_len(),
                got: record.len(),
            });
        }
        if null_map.len() != self.null_map_len() {
            return Err(DbError::RecordLength {
                want: self.null_map_len(),
                got: null_map.len(),
            });
        }
        Ok(())
    }

    fn slot_bytes(&self, status: u8, record: &[u8], null_map: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(usize::try_from(self.attr.slot_len).expect("fits"));
        out.push(status);
        out.extend_from_slice(record);
        out.extend_from_slice(null_map);
        out
    }

    /// The slot at `rrn`: `None` past the end or never written.
    pub fn read(&self, txn: &mut Txn<'_>, rrn: u64) -> Result<Option<Slot>> {
        if rrn == 0 || rrn > self.attr.slots {
            return Ok(None);
        }
        let (space, _) = Space::load(txn, self.id)?;
        let bytes = space.read(
            txn,
            slots::offset(rrn, self.attr.slot_len),
            usize::try_from(self.attr.slot_len).expect("fits"),
        )?;
        let record_len = self.record_len();
        Ok(match bytes[0] {
            slots::STATUS_EMPTY => None,
            status => Some(Slot {
                rrn,
                record: bytes[1..=record_len].to_vec(),
                null_map: bytes[1 + record_len..].to_vec(),
                deleted: status == slots::STATUS_DELETED,
            }),
        })
    }

    /// Active records from `from` on, at most `max`, in RRN order.
    pub fn scan(&self, txn: &mut Txn<'_>, from: u64, max: usize) -> Result<Vec<Slot>> {
        let mut out = Vec::new();
        let mut rrn = from.max(1);
        while rrn <= self.attr.slots && out.len() < max {
            if let Some(s) = self.read(txn, rrn)?
                && !s.deleted
            {
                out.push(s);
            }
            rrn += 1;
        }
        Ok(out)
    }

    /// The RRN a new record takes: a reused slot or the next one.
    fn next_rrn(&mut self, txn: &mut Txn<'_>) -> Result<(u64, bool)> {
        if self.attr.reuse_deleted
            && let Some(root) = self.roots[attr::FREE_ROOT]
        {
            let mut tree: FreeTree = BTree::open(Some(root), self.id);
            let first = tree.entries(txn)?.next().transpose()?;
            if let Some((rrn, ())) = first {
                tree.delete(txn, &rrn)?;
                self.set_root(txn, attr::FREE_ROOT, tree.root())?;
                return Ok((rrn, true));
            }
        }
        let rrn = self.attr.slots + 1;
        if self.attr.max_records > 0 && rrn > self.attr.max_records {
            return Err(DbError::MemberFull {
                member: self.name.to_string(),
                records: self.attr.slots,
            });
        }
        Ok((rrn, false))
    }

    /// Write a new record; returns its RRN.
    pub fn write_new(&mut self, txn: &mut Txn<'_>, record: &[u8], null_map: &[u8]) -> Result<u64> {
        self.check(record, null_map)?;
        let (rrn, reused) = self.next_rrn(txn)?;
        let slot_len = u64::from(self.attr.slot_len);
        let need = rrn * slot_len;
        let (mut space, _) = Space::load(txn, self.id)?;
        if space.bytes() < need {
            let grow_to = ((self.attr.slots + self.attr.increment_slots) * slot_len).max(need);
            space = Space::ensure(txn, self.id, grow_to, 0)?;
        }
        space.write(
            txn,
            slots::offset(rrn, self.attr.slot_len),
            &self.slot_bytes(slots::STATUS_ACTIVE, record, null_map),
        )?;
        if reused {
            self.attr.deleted_count -= 1;
        } else {
            self.attr.slots = rrn;
        }
        self.attr.record_count += 1;
        self.save(txn)?;
        Ok(rrn)
    }

    /// Replace the record at `rrn`, which must be active.
    pub fn update(
        &mut self,
        txn: &mut Txn<'_>,
        rrn: u64,
        record: &[u8],
        null_map: &[u8],
    ) -> Result<()> {
        self.check(record, null_map)?;
        self.active(txn, rrn)?;
        let (space, _) = Space::load(txn, self.id)?;
        space
            .write(
                txn,
                slots::offset(rrn, self.attr.slot_len),
                &self.slot_bytes(slots::STATUS_ACTIVE, record, null_map),
            )
            .map_err(DbError::from)
    }

    /// Delete the record at `rrn`, which must be active.
    pub fn delete(&mut self, txn: &mut Txn<'_>, rrn: u64) -> Result<()> {
        self.active(txn, rrn)?;
        let (space, _) = Space::load(txn, self.id)?;
        space.write(
            txn,
            slots::offset(rrn, self.attr.slot_len),
            &[slots::STATUS_DELETED],
        )?;
        self.attr.record_count -= 1;
        self.attr.deleted_count += 1;
        if self.attr.reuse_deleted {
            let mut tree: FreeTree = BTree::open(self.roots[attr::FREE_ROOT], self.id);
            tree.insert(txn, rrn, &())?;
            self.set_root(txn, attr::FREE_ROOT, tree.root())?;
        }
        self.save(txn)
    }

    fn active(&self, txn: &mut Txn<'_>, rrn: u64) -> Result<()> {
        match self.read(txn, rrn)? {
            None => Err(DbError::RecordNotFound { rrn }),
            Some(s) if s.deleted => Err(DbError::RecordDeleted { rrn }),
            Some(_) => Ok(()),
        }
    }

    /// `CLRPFM`: every record gone, the space and trees released.
    pub fn clear(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        Space::truncate(txn, self.id, 0)?;
        for slot in [attr::PATH_ROOT, attr::FREE_ROOT] {
            if let Some(root) = self.roots[slot] {
                btree::free_tree(txn, root)?;
                self.set_root(txn, slot, None)?;
            }
        }
        self.attr.record_count = 0;
        self.attr.deleted_count = 0;
        self.attr.slots = 0;
        self.attr.path_entries = 0;
        self.save(txn)
    }
}
