// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A `*DIRDB` object: creating and opening it, its attribute area, tree
//! roots and chunked records, and entries by ID.

use qsname::Name;
use store::btree::BTree;
use store::obj::header::{ATTR_OFF, OFF_TREE_ROOTS};
use store::obj::library::Library;
use store::obj::{CreateParams, ObjectId, ops, types};
use store::page::pager::{PageNo, Pager};
use store::txn::Txn;

use super::error::{DirError, Result};
use super::format::{ATTR_LEN, CHUNK, ChunkKey, DirdbAttr, IndexKind, MAX_RECORD, SLOT_ENTRIES};
use super::normalize::Normalizer;
use super::record::{self, Entry, IndexDef};

/// What creating a store needs besides its library and name.
#[derive(Debug, Clone)]
pub struct CreateOpts {
    /// Text description.
    pub text: String,
    /// Owner profile.
    pub owner: String,
    /// The system's name.
    pub system: String,
    /// Unix seconds.
    pub now: u64,
}

/// One instance's entry store. Every call runs inside the caller's
/// transaction; nothing is cached across calls, so a handle stays valid
/// across transactions, aborted ones included. A call that fails may
/// have done part of its work: the caller aborts the transaction, as
/// the service does for every failed operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirDb {
    id: ObjectId,
    page: PageNo,
}

pub(crate) type Chunks = BTree<ChunkKey, Vec<u8>>;

/// The always-on indexes (docs/DIRSRV.md §5.2).
pub(crate) fn shipped_indexes() -> Vec<IndexDef> {
    let pe = IndexKind::Presence.bit() | IndexKind::Equality.bit();
    let pes = pe | IndexKind::Substring.bit();
    [
        ("objectclass", pe),
        ("cn", pes),
        ("uid", pes),
        ("sn", pes),
        ("mail", pes),
        ("entryuuid", pe),
    ]
    .iter()
    .zip(1u16..)
    .map(|((attr, kinds), number)| IndexDef {
        number,
        attr: (*attr).into(),
        kinds: *kinds,
        always: true,
    })
    .collect()
}

impl DirDb {
    /// Create `lib/name` (`QUSRDIRDB/QUSRDIR`), empty but for the
    /// always-on index definitions.
    ///
    /// # Errors
    ///
    /// The object store's: the name taken, the library missing.
    pub fn create(txn: &mut Txn<'_>, lib: ObjectId, name: Name, opts: &CreateOpts) -> Result<Self> {
        let id = ops::create(
            txn,
            &CreateParams {
                library: lib,
                name,
                type_code: types::DIRDB,
                attribute: String::new(),
                text: opts.text.clone(),
                owner: opts.owner.clone(),
                system: opts.system.clone(),
                now: opts.now,
                by_system: true,
            },
        )?;
        let (page, _) = ops::read_header(txn, id)?;
        let db = Self { id, page };
        let defs = shipped_indexes();
        let mut attr = DirdbAttr::new();
        attr.index_count = u16::try_from(defs.len()).expect("six");
        db.save_attr(txn, &attr)?;
        db.write_record(txn, SLOT_ENTRIES, 0, &record::encode_defs(&defs)?)?;
        Ok(db)
    }

    /// The store `lib/name`, if there is one.
    ///
    /// # Errors
    ///
    /// The object store's, or a store of a version this code does not
    /// read.
    pub fn open(txn: &mut Txn<'_>, lib: ObjectId, name: Name) -> Result<Option<Self>> {
        let Some(id) = Library::load(txn, lib)?.lookup(txn, types::DIRDB, name)? else {
            return Ok(None);
        };
        Self::open_id(txn, id).map(Some)
    }

    /// The store with object ID `id`.
    ///
    /// # Errors
    ///
    /// No such object, not a `*DIRDB`, or a version this code does not
    /// read.
    pub fn open_id(txn: &mut Txn<'_>, id: ObjectId) -> Result<Self> {
        if id.type_code != types::DIRDB {
            return Err(DirError::Corrupt(format!("{id} is not a *DIRDB")));
        }
        let (page, _) = ops::read_header(txn, id)?;
        let db = Self { id, page };
        db.attr(txn)?;
        Ok(db)
    }

    /// The object's ID.
    #[must_use]
    pub fn id(&self) -> ObjectId {
        self.id
    }

    /// The attribute area: counters, the change log's bounds, the
    /// suffixes.
    ///
    /// # Errors
    ///
    /// The object store's, or a damaged area.
    pub fn attr(&self, txn: &mut Txn<'_>) -> Result<DirdbAttr> {
        let p = txn.read(self.page)?;
        DirdbAttr::decode(&p.body()[ATTR_OFF..ATTR_OFF + ATTR_LEN])
            .ok_or_else(|| DirError::Corrupt("attribute area version".into()))
    }

    pub(crate) fn save_attr(&self, txn: &mut Txn<'_>, attr: &DirdbAttr) -> Result<()> {
        ops::write_attr(txn, self.page, 0, &attr.encode())?;
        Ok(())
    }

    pub(crate) fn root(&self, txn: &mut Txn<'_>, slot: usize) -> Result<Option<PageNo>> {
        let p = txn.read(self.page)?;
        let at = OFF_TREE_ROOTS + slot * 8;
        let r = u64::from_le_bytes(p.body()[at..at + 8].try_into().expect("8 bytes"));
        Ok((r != 0).then_some(r))
    }

    pub(crate) fn set_root(
        &self,
        txn: &mut Txn<'_>,
        slot: usize,
        root: Option<PageNo>,
    ) -> Result<()> {
        if self.root(txn, slot)? != root {
            ops::set_tree_root(txn, self.page, slot, root)?;
        }
        Ok(())
    }

    pub(crate) fn tree<K: store::btree::FixedKey, V: store::btree::Value>(
        &self,
        txn: &mut Txn<'_>,
        slot: usize,
    ) -> Result<BTree<K, V>> {
        Ok(BTree::open(self.root(txn, slot)?, self.id))
    }

    /// A chunked record's bytes, `None` when it is absent.
    pub(crate) fn read_record(
        &self,
        txn: &mut Txn<'_>,
        slot: usize,
        id: u64,
    ) -> Result<Option<Vec<u8>>> {
        let t: Chunks = self.tree(txn, slot)?;
        let mut out = Vec::new();
        let mut n = 0u16;
        for r in t.range(txn, ChunkKey::first(id)..=ChunkKey::last(id))? {
            let (k, v) = r?;
            if k.chunk != n {
                return Err(DirError::Corrupt(format!("record {id}: chunk {n} missing")));
            }
            out.extend_from_slice(&v);
            n += 1;
        }
        Ok((n > 0).then_some(out))
    }

    /// Write a chunked record over whatever was there.
    pub(crate) fn write_record(
        &self,
        txn: &mut Txn<'_>,
        slot: usize,
        id: u64,
        bytes: &[u8],
    ) -> Result<()> {
        if bytes.len() > MAX_RECORD {
            return Err(DirError::Limit(format!("record of {} bytes", bytes.len())));
        }
        let mut t: Chunks = self.tree(txn, slot)?;
        let mut n = 0u16;
        for c in bytes.chunks(CHUNK) {
            t.insert(txn, ChunkKey { id, chunk: n }, &c.to_vec())?;
            n += 1;
        }
        if bytes.is_empty() {
            t.insert(txn, ChunkKey::first(id), &Vec::new())?;
            n = 1;
        }
        let stale: Vec<ChunkKey> = t
            .range(txn, ChunkKey { id, chunk: n }..=ChunkKey::last(id))?
            .map(|r| r.map(|(k, _)| k))
            .collect::<store::Result<_>>()?;
        for k in stale {
            t.delete(txn, &k)?;
        }
        self.set_root(txn, slot, t.root())
    }

    /// Remove a chunked record; whether it was there.
    pub(crate) fn delete_record(&self, txn: &mut Txn<'_>, slot: usize, id: u64) -> Result<bool> {
        let mut t: Chunks = self.tree(txn, slot)?;
        let keys: Vec<ChunkKey> = t
            .range(txn, ChunkKey::first(id)..=ChunkKey::last(id))?
            .map(|r| r.map(|(k, _)| k))
            .collect::<store::Result<_>>()?;
        for k in &keys {
            t.delete(txn, k)?;
        }
        self.set_root(txn, slot, t.root())?;
        Ok(!keys.is_empty())
    }

    /// Whether entry `id` exists.
    ///
    /// # Errors
    ///
    /// The object store's.
    pub fn exists(&self, txn: &mut Txn<'_>, id: u64) -> Result<bool> {
        if id == 0 {
            return Ok(false);
        }
        let t: Chunks = self.tree(txn, SLOT_ENTRIES)?;
        Ok(t.get(txn, &ChunkKey::first(id))?.is_some())
    }

    /// Entry `id`, `None` when there is none.
    ///
    /// # Errors
    ///
    /// The object store's, or a damaged record.
    pub fn get(&self, txn: &mut Txn<'_>, id: u64) -> Result<Option<Entry>> {
        if id == 0 {
            return Ok(None);
        }
        self.read_record(txn, SLOT_ENTRIES, id)?
            .map(|b| Entry::decode(&b))
            .transpose()
    }

    pub(crate) fn must_get(&self, txn: &mut Txn<'_>, id: u64) -> Result<Entry> {
        self.get(txn, id)?.ok_or(DirError::NoSuchEntry(id))
    }

    /// Replace entry `id`'s attributes with `entry`'s (a modify); its
    /// name and place stay, whatever `entry` says of them. The indexes
    /// follow.
    ///
    /// # Errors
    ///
    /// No such entry, a value the normalizer refuses, a limit.
    pub fn put(
        &self,
        txn: &mut Txn<'_>,
        norm: &dyn Normalizer,
        id: u64,
        entry: &Entry,
    ) -> Result<()> {
        let old = self.must_get(txn, id)?;
        let new = Entry {
            parent: old.parent,
            suffix: old.suffix,
            rdn: old.rdn.clone(),
            rdn_key: old.rdn_key.clone(),
            attrs: entry.attrs.clone(),
        };
        self.write_record(txn, SLOT_ENTRIES, id, &new.encode()?)?;
        self.reindex(txn, norm, id, Some(&old), Some(&new))
    }

    /// Store a new entry record under the next ID.
    pub(crate) fn insert_new(
        &self,
        txn: &mut Txn<'_>,
        norm: &dyn Normalizer,
        entry: &Entry,
    ) -> Result<u64> {
        let bytes = entry.encode()?;
        let mut attr = self.attr(txn)?;
        let id = attr.next_id;
        attr.next_id += 1;
        attr.entry_count += 1;
        self.save_attr(txn, &attr)?;
        self.write_record(txn, SLOT_ENTRIES, id, &bytes)?;
        self.reindex(txn, norm, id, None, Some(entry))?;
        Ok(id)
    }

    /// Drop an entry record and its index keys.
    pub(crate) fn remove_record(
        &self,
        txn: &mut Txn<'_>,
        norm: &dyn Normalizer,
        id: u64,
        old: &Entry,
    ) -> Result<()> {
        self.reindex(txn, norm, id, Some(old), None)?;
        self.delete_record(txn, SLOT_ENTRIES, id)?;
        let mut attr = self.attr(txn)?;
        attr.entry_count = attr.entry_count.saturating_sub(1);
        self.save_attr(txn, &attr)
    }

    /// Entry IDs from `from` on, at most `max`, ascending: every entry
    /// of the store, entry 0 left out.
    ///
    /// # Errors
    ///
    /// The object store's.
    pub fn ids(&self, txn: &mut Txn<'_>, from: u64, max: usize) -> Result<Vec<u64>> {
        let t: Chunks = self.tree(txn, SLOT_ENTRIES)?;
        let mut out = Vec::new();
        for r in t.range(txn, ChunkKey::first(from.max(1))..)? {
            let (k, _) = r?;
            if k.chunk == 0 {
                if out.len() == max {
                    break;
                }
                out.push(k.id);
            }
        }
        Ok(out)
    }
}
