// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Members: `*MBR` objects owned by a file, their record slots, their
//! access paths and the logical members that depend on them. The
//! operations here are the raw record operations; the open data path
//! adds positioning, locks and journaling on top.

pub mod attr;
pub mod slots;

pub use attr::MbrAttr;
pub use slots::Slot;

use std::ops::Bound;

use qsname::{MemberSpec, Name};
use store::PageNo;
use store::btree::{self, BTree};
use store::obj::header::TREE_ROOT_SLOTS;
use store::obj::ops::{self, CreateParams};
use store::obj::space::Space;
use store::obj::{JobHandle, ObjectId, types};
use store::txn::Txn;

use crate::error::{DbError, Result};
use crate::file::{self, FileDesc, Maint, MemberEntry};
use crate::format::FormatDef;
use crate::lf;
use crate::path::{self, MULTI_FORMAT, Path, PathKey};

/// Bytes a space grows by when `SIZE` gives no increment.
const DEFAULT_GROWTH_BYTES: u64 = 16 * 4064;

/// Free-slot tree: RRNs of deleted slots, no values.
type FreeTree = BTree<u64, ()>;

/// Progress of a rebuild: records done, records in all.
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64);

/// The stored part of a member: header page, attributes, tree roots.
#[derive(Clone, Debug)]
pub struct Core {
    /// The `*MBR` object.
    pub id: ObjectId,
    /// Its header page.
    pub page: PageNo,
    /// The attributes.
    pub attr: MbrAttr,
    /// Header tree roots: slot 0 the access path, slot 1 the free slots.
    pub roots: [Option<PageNo>; TREE_ROOT_SLOTS],
    /// The member's name.
    pub name: Name,
}

impl Core {
    /// Load a member's stored part.
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

    /// The based-on members of a logical member, in format order.
    pub fn base_members(&self, txn: &mut Txn<'_>) -> Result<Vec<ObjectId>> {
        let a = ops::read_attr(txn, self.page)?;
        Ok((0..usize::from(self.attr.base_count))
            .map(|i| {
                ObjectId::from_bytes(&a[attr::ATTR_LEN + i * 16..attr::ATTR_LEN + i * 16 + 16])
            })
            .collect())
    }

    /// The access path, when the member has one.
    #[must_use]
    pub fn path(&self) -> Option<Path> {
        (self.attr.key_len > 0).then(|| Path {
            owner: self.id,
            key_len: usize::from(self.attr.key_len),
            multi: self.attr.format == MULTI_FORMAT,
            root: self.roots[attr::PATH_ROOT],
        })
    }

    fn store_path(&mut self, txn: &mut Txn<'_>, p: &Path, entries: u64) -> Result<()> {
        self.set_root(txn, attr::PATH_ROOT, p.root)?;
        self.attr.path_entries = entries;
        self.save(txn)
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

    /// The slot at `rrn`: `None` past the end or never written.
    pub fn read_slot(&self, txn: &mut Txn<'_>, rrn: u64) -> Result<Option<Slot>> {
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

    /// Mark the path invalid until a rebuild.
    pub fn invalidate(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        if !self.attr.path_invalid {
            self.attr.path_invalid = true;
            self.save(txn)?;
        }
        Ok(())
    }
}

/// A logical member whose path a physical member maintains.
#[derive(Clone, Debug)]
pub struct Dependent {
    /// The logical member.
    pub core: Core,
    /// Its file.
    pub lf: FileDesc,
    /// Formats of the logical file based on this physical member.
    pub formats: Vec<usize>,
}

/// A based-on member of a logical member, with the format it holds.
#[derive(Clone, Debug)]
pub struct Base {
    /// The physical member.
    pub member: ObjectId,
    /// Its record format.
    pub format: FormatDef,
}

/// A member opened with its file: the physical records and access path,
/// or the logical view over its based-on members.
#[derive(Clone, Debug)]
pub struct Member {
    /// The stored part.
    pub core: Core,
    /// The owning file.
    pub file: FileDesc,
    /// Logical member: based-on member per format.
    pub bases: Vec<Base>,
    /// Physical member: logical members depending on it.
    pub dependents: Vec<Dependent>,
    /// Who changes it, for the journal.
    pub jrn: crate::journal::JrnCtx,
}

/// What a new physical member takes from its file.
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

fn new_attr(format: &FormatDef, file: ObjectId, mp: MemberParams) -> MbrAttr {
    let slot_len = format.slot_len();
    let increment_slots = if mp.increment > 0 {
        u64::from(mp.increment)
    } else {
        (DEFAULT_GROWTH_BYTES / u64::from(slot_len)).max(1)
    };
    MbrAttr {
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
        base_count: 0,
        file,
        last_save: 0,
        last_restore: 0,
    }
}

fn create_object(
    txn: &mut Txn<'_>,
    file: ObjectId,
    params: &CreateParams,
) -> Result<(ObjectId, PageNo)> {
    let params = CreateParams {
        type_code: types::MBR,
        ..params.clone()
    };
    let id = ops::create_owned(txn, &params, file)?;
    let (page, _) = ops::read_header(txn, id)?;
    Ok((id, page))
}

/// Create a physical member of `file` for `format`, owned by the file.
pub fn create(
    txn: &mut Txn<'_>,
    file: ObjectId,
    format: &FormatDef,
    mp: MemberParams,
    params: &CreateParams,
) -> Result<ObjectId> {
    let (id, page) = create_object(txn, file, params)?;
    let attr = new_attr(format, file, mp);
    ops::write_attr(txn, page, 0, &attr.encode())?;
    Ok(id)
}

/// Create a logical member over `bases`, one per format of the file.
pub fn create_logical(
    txn: &mut Txn<'_>,
    file: ObjectId,
    formats: &[FormatDef],
    bases: &[ObjectId],
    params: &CreateParams,
) -> Result<ObjectId> {
    let (id, page) = create_object(txn, file, params)?;
    let first = formats.first().ok_or_else(|| DbError::Corrupt {
        what: "logical file without formats".into(),
    })?;
    let mut attr = new_attr(
        first,
        file,
        MemberParams {
            format_index: if formats.len() > 1 { MULTI_FORMAT } else { 0 },
            max_records: 0,
            increment: 0,
            reuse_deleted: false,
        },
    );
    attr.key_len = formats.iter().map(|f| f.key_len).max().unwrap_or(0);
    attr.base_count = u16::try_from(bases.len()).expect("few");
    ops::write_attr(txn, page, 0, &attr.encode())?;
    let mut list = Vec::with_capacity(bases.len() * 16);
    for b in bases {
        list.extend_from_slice(&b.to_bytes());
    }
    ops::write_attr(txn, page, attr::ATTR_LEN, &list)?;
    Ok(id)
}

/// Delete a member object with everything it holds.
pub fn destroy(txn: &mut Txn<'_>, job: JobHandle, id: ObjectId) -> Result<()> {
    ops::delete(txn, job, id)?;
    Ok(())
}

/// The format of a based-on file a logical format names.
fn base_format(txn: &mut Txn<'_>, file: ObjectId, name: Name) -> Result<FormatDef> {
    let desc = file::describe(txn, file)?;
    desc.formats
        .iter()
        .find(|f| f.format.name == name)
        .or_else(|| desc.formats.first())
        .cloned()
        .ok_or_else(|| DbError::Corrupt {
            what: format!("based-on file {} has no format {name}", desc.name()),
        })
}

impl Member {
    /// Open a member of `file`. A `MAINT(*DLY)` path found invalid is
    /// rebuilt here.
    pub fn open(txn: &mut Txn<'_>, file: &FileDesc, entry: &MemberEntry) -> Result<Self> {
        let core = Core::load(txn, entry.object)?;
        let mut m = Self {
            core,
            file: file.clone(),
            bases: Vec::new(),
            dependents: Vec::new(),
            jrn: crate::journal::JrnCtx::default(),
        };
        if file.attr.kind == file::FileKind::Logical {
            let members = m.core.base_members(txn)?;
            for (fi, member) in members.into_iter().enumerate() {
                let log = m.logical(fi)?;
                let (base_file, name) =
                    log.based_on
                        .first()
                        .copied()
                        .ok_or_else(|| DbError::Corrupt {
                            what: format!("format {fi} of {} has no based-on file", file.name()),
                        })?;
                let format = base_format(txn, base_file, name)?;
                m.bases.push(Base { member, format });
            }
        } else {
            m.load_dependents(txn)?;
        }
        if m.core.attr.path_invalid && m.file.attr.maint == Maint::Dly {
            m.rebuild_path(txn, &mut |_, _| {})?;
        }
        Ok(m)
    }

    /// Open the member a `MBR` parameter names.
    pub fn open_by(txn: &mut Txn<'_>, file: ObjectId, spec: MemberSpec) -> Result<Self> {
        let desc = file::describe(txn, file)?;
        let entry = *desc.member(spec).ok_or_else(|| DbError::MemberNotFound {
            file: desc.name().to_string(),
            member: spec.to_string(),
        })?;
        Self::open(txn, &desc, &entry)
    }

    fn load_dependents(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        let mine: Vec<_> = self
            .file
            .dependents
            .iter()
            .filter(|d| d.base_member == self.core.id)
            .copied()
            .collect();
        for d in mine {
            if self.dependents.iter().any(|x| x.core.id == d.lf_member) {
                continue;
            }
            let lf = file::describe(txn, d.lf_file)?;
            let core = Core::load(txn, d.lf_member)?;
            let formats: Vec<usize> = core
                .base_members(txn)?
                .iter()
                .enumerate()
                .filter(|(_, b)| **b == self.core.id)
                .map(|(fi, _)| fi)
                .collect();
            self.dependents.push(Dependent { core, lf, formats });
        }
        Ok(())
    }

    /// The member's object.
    #[must_use]
    pub fn id(&self) -> ObjectId {
        self.core.id
    }

    /// The attributes.
    #[must_use]
    pub fn attr(&self) -> &MbrAttr {
        &self.core.attr
    }

    /// The member's name.
    #[must_use]
    pub fn name(&self) -> Name {
        self.core.name
    }

    /// Whether this is a logical member.
    #[must_use]
    pub fn is_logical(&self) -> bool {
        self.file.attr.kind == file::FileKind::Logical
    }

    /// Whether the path has more than one format.
    #[must_use]
    pub fn is_multi_format(&self) -> bool {
        self.core.attr.format == MULTI_FORMAT
    }

    /// The format of a single-format member.
    #[must_use]
    pub fn format(&self) -> &FormatDef {
        let i = usize::from(self.core.attr.format);
        &self.file.formats[if self.is_multi_format() { 0 } else { i }]
    }

    /// Format `index` of the file.
    #[must_use]
    pub fn format_at(&self, index: usize) -> &FormatDef {
        &self.file.formats[index]
    }

    fn logical(&self, index: usize) -> Result<&lf::LogicalDef> {
        self.file.formats[index]
            .logical
            .as_ref()
            .ok_or_else(|| DbError::Corrupt {
                what: format!("format {index} of {} is not logical", self.file.name()),
            })
    }

    fn physical_only(&self) -> Result<()> {
        if self.is_logical() {
            return Err(DbError::Definition {
                what: format!(
                    "records of logical file {} are written through its based-on file",
                    self.file.name()
                ),
            });
        }
        Ok(())
    }

    fn check(&self, record: &[u8], null_map: &[u8]) -> Result<()> {
        if record.len() != self.core.record_len() {
            return Err(DbError::RecordLength {
                want: self.core.record_len(),
                got: record.len(),
            });
        }
        if null_map.len() != self.core.null_map_len() {
            return Err(DbError::RecordLength {
                want: self.core.null_map_len(),
                got: null_map.len(),
            });
        }
        Ok(())
    }

    fn slot_bytes(&self, status: u8, record: &[u8], null_map: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(usize::try_from(self.core.attr.slot_len).expect("fits"));
        out.push(status);
        out.extend_from_slice(record);
        out.extend_from_slice(null_map);
        out
    }

    /// The slot at `rrn` of a physical member.
    pub fn read(&self, txn: &mut Txn<'_>, rrn: u64) -> Result<Option<Slot>> {
        self.physical_only()?;
        self.core.read_slot(txn, rrn)
    }

    /// Active records from `from` on, at most `max`, in RRN order.
    pub fn scan(&self, txn: &mut Txn<'_>, from: u64, max: usize) -> Result<Vec<Slot>> {
        self.physical_only()?;
        let mut out = Vec::new();
        let mut rrn = from.max(1);
        while rrn <= self.core.attr.slots && out.len() < max {
            if let Some(s) = self.core.read_slot(txn, rrn)?
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
        if self.core.attr.reuse_deleted
            && let Some(root) = self.core.roots[attr::FREE_ROOT]
        {
            let mut tree: FreeTree = BTree::open(Some(root), self.core.id);
            let first = tree.entries(txn)?.next().transpose()?;
            if let Some((rrn, ())) = first {
                tree.delete(txn, &rrn)?;
                self.core.set_root(txn, attr::FREE_ROOT, tree.root())?;
                return Ok((rrn, true));
            }
        }
        let rrn = self.core.attr.slots + 1;
        if self.core.attr.max_records > 0 && rrn > self.core.attr.max_records {
            return Err(DbError::MemberFull {
                member: self.core.name.to_string(),
                records: self.core.attr.slots,
            });
        }
        Ok((rrn, false))
    }

    /// Write a new record; returns its RRN.
    pub fn write_new(&mut self, txn: &mut Txn<'_>, record: &[u8], null_map: &[u8]) -> Result<u64> {
        self.physical_only()?;
        self.check(record, null_map)?;
        let (rrn, reused) = self.next_rrn(txn)?;
        self.link(txn, rrn, record, null_map)?;
        let slot_len = u64::from(self.core.attr.slot_len);
        let need = rrn * slot_len;
        let (mut space, _) = Space::load(txn, self.core.id)?;
        if space.bytes() < need {
            let grow_to =
                ((self.core.attr.slots + self.core.attr.increment_slots) * slot_len).max(need);
            space = Space::ensure(txn, self.core.id, grow_to, 0)?;
        }
        space.write(
            txn,
            slots::offset(rrn, self.core.attr.slot_len),
            &self.slot_bytes(slots::STATUS_ACTIVE, record, null_map),
        )?;
        if reused {
            self.core.attr.deleted_count -= 1;
        } else {
            self.core.attr.slots = rrn;
        }
        self.core.attr.record_count += 1;
        self.core.save(txn)?;
        crate::journal::record(txn, self, "PT", rrn, record, null_map)?;
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
        self.physical_only()?;
        self.check(record, null_map)?;
        let old = self.active(txn, rrn)?;
        self.unlink(txn, rrn, &old.record, &old.null_map)?;
        self.link(txn, rrn, record, null_map)?;
        let (space, _) = Space::load(txn, self.core.id)?;
        space.write(
            txn,
            slots::offset(rrn, self.core.attr.slot_len),
            &self.slot_bytes(slots::STATUS_ACTIVE, record, null_map),
        )?;
        if self.file.attr.jrn_both_images {
            crate::journal::record(txn, self, "UB", rrn, &old.record, &old.null_map)?;
        }
        crate::journal::record(txn, self, "UP", rrn, record, null_map)
    }

    /// Delete the record at `rrn`, which must be active.
    pub fn delete(&mut self, txn: &mut Txn<'_>, rrn: u64) -> Result<()> {
        self.physical_only()?;
        let old = self.active(txn, rrn)?;
        self.unlink(txn, rrn, &old.record, &old.null_map)?;
        let (space, _) = Space::load(txn, self.core.id)?;
        space.write(
            txn,
            slots::offset(rrn, self.core.attr.slot_len),
            &[slots::STATUS_DELETED],
        )?;
        self.core.attr.record_count -= 1;
        self.core.attr.deleted_count += 1;
        if self.core.attr.reuse_deleted {
            let mut tree: FreeTree = BTree::open(self.core.roots[attr::FREE_ROOT], self.core.id);
            tree.insert(txn, rrn, &())?;
            self.core.set_root(txn, attr::FREE_ROOT, tree.root())?;
        }
        self.core.save(txn)?;
        crate::journal::record(txn, self, "DL", rrn, &old.record, &old.null_map)
    }

    fn active(&self, txn: &mut Txn<'_>, rrn: u64) -> Result<Slot> {
        match self.core.read_slot(txn, rrn)? {
            None => Err(DbError::RecordNotFound { rrn }),
            Some(s) if s.deleted => Err(DbError::RecordDeleted { rrn }),
            Some(s) => Ok(s),
        }
    }

    /// `CLRPFM`: every record gone, the space and trees released, the
    /// dependent paths emptied.
    pub fn clear(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        self.physical_only()?;
        self.refresh_dependents(txn)?;
        Space::truncate(txn, self.core.id, 0)?;
        for slot in [attr::PATH_ROOT, attr::FREE_ROOT] {
            if let Some(root) = self.core.roots[slot] {
                btree::free_tree(txn, root)?;
                self.core.set_root(txn, slot, None)?;
            }
        }
        self.core.attr.record_count = 0;
        self.core.attr.deleted_count = 0;
        self.core.attr.slots = 0;
        self.core.attr.path_entries = 0;
        self.core.save(txn)?;
        for di in 0..self.dependents.len() {
            let mut d = self.dependents[di].clone();
            if let Some(mut p) = d.core.path() {
                if d.formats.len() == d.lf.formats.len() {
                    p.free(txn)?;
                    d.core.store_path(txn, &p, 0)?;
                } else {
                    // Only this member's formats leave the shared path.
                    let mut removed = 0u64;
                    for k in p.all(txn)? {
                        if d.formats.contains(&usize::from(k.format)) && p.remove(txn, &k)? {
                            removed += 1;
                        }
                    }
                    let left = d.core.attr.path_entries.saturating_sub(removed);
                    d.core.store_path(txn, &p, left)?;
                }
            }
            self.dependents[di] = d;
        }
        crate::journal::member_event(txn, self, "MC")
    }

    /// `REUSEDLT`: build the free-slot tree from the deleted slots, or drop it.
    pub fn set_reuse_deleted(&mut self, txn: &mut Txn<'_>, yes: bool) -> Result<()> {
        self.physical_only()?;
        if let Some(root) = self.core.roots[attr::FREE_ROOT] {
            btree::free_tree(txn, root)?;
            self.core.set_root(txn, attr::FREE_ROOT, None)?;
        }
        if yes {
            let mut tree: FreeTree = BTree::open(None, self.core.id);
            for rrn in 1..=self.core.attr.slots {
                if self.core.read_slot(txn, rrn)?.is_some_and(|s| s.deleted) {
                    tree.insert(txn, rrn, &())?;
                }
            }
            self.core.set_root(txn, attr::FREE_ROOT, tree.root())?;
        }
        self.core.attr.reuse_deleted = yes;
        self.core.save(txn)
    }

    /// `SIZE`: the record limit and growth of this member.
    pub fn set_limits(
        &mut self,
        txn: &mut Txn<'_>,
        max_records: u64,
        increment: u32,
    ) -> Result<()> {
        self.core.attr.max_records = max_records;
        if increment > 0 {
            self.core.attr.increment_slots = u64::from(increment);
        }
        self.core.save(txn)
    }

    /// Reload the stored part: another handle may have written records
    /// in this transaction.
    pub fn refresh(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        self.core = Core::load(txn, self.core.id)?;
        Ok(())
    }

    /// Reload what the dependents hold on disk: another handle may have
    /// rebuilt or invalidated a path in this transaction.
    fn refresh_dependents(&mut self, txn: &mut Txn<'_>) -> Result<()> {
        for d in &mut self.dependents {
            d.core = Core::load(txn, d.core.id)?;
            let a = ops::read_attr(txn, d.lf.page)?;
            d.lf.attr = file::FileAttr::decode(&a[..file::attr::ATTR_LEN])?;
        }
        Ok(())
    }

    /// Every path entry a physical record makes, with the path it goes
    /// in: `None` for the member's own, `Some(i)` for dependent `i`.
    fn entries_of(
        &self,
        rrn: u64,
        record: &[u8],
        null_map: &[u8],
    ) -> Result<Vec<(Option<usize>, PathKey, bool)>> {
        let mut out = Vec::new();
        let own = self.format();
        if own.is_keyed() {
            let unique = own.unique.is_some()
                && !(own.unique == Some(dds::NullKeys::Exclude)
                    && path::key_has_null(own, null_map));
            out.push((
                None,
                PathKey {
                    key: path::record_key(own, record, null_map)?,
                    format: 0,
                    rrn,
                },
                unique,
            ));
        }
        for (di, d) in self.dependents.iter().enumerate() {
            for &fi in &d.formats {
                let lf_fmt = &d.lf.formats[fi];
                let log = lf_fmt.logical.as_ref().ok_or_else(|| DbError::Corrupt {
                    what: format!("format {fi} of {} is not logical", d.lf.name()),
                })?;
                let (rec, nulls) = lf::derive(lf_fmt, log, own, record, null_map)?;
                if !d.lf.attr.dynslt && !lf::selected(lf_fmt, log, &rec, &nulls)? {
                    continue;
                }
                let unique = lf_fmt.unique.is_some()
                    && !(lf_fmt.unique == Some(dds::NullKeys::Exclude)
                        && path::key_has_null(lf_fmt, &nulls));
                out.push((
                    Some(di),
                    PathKey {
                        key: path::record_key(lf_fmt, &rec, &nulls)?,
                        format: u16::try_from(fi).expect("few"),
                        rrn,
                    },
                    unique,
                ));
            }
        }
        Ok(out)
    }

    /// Whether a path is maintained now (else it is marked invalid).
    fn target(&mut self, txn: &mut Txn<'_>, which: Option<usize>) -> Result<Option<(Path, Maint)>> {
        let (core, maint) = match which {
            None => (&mut self.core, self.file.attr.maint),
            Some(di) => {
                let d = &mut self.dependents[di];
                (&mut d.core, d.lf.attr.maint)
            }
        };
        let Some(p) = core.path() else {
            return Ok(None);
        };
        if maint != Maint::Immed {
            core.invalidate(txn)?;
            return Ok(None);
        }
        Ok(Some((p, maint)))
    }

    fn store_target(
        &mut self,
        txn: &mut Txn<'_>,
        which: Option<usize>,
        p: &Path,
        delta: i64,
    ) -> Result<()> {
        let core = match which {
            None => &mut self.core,
            Some(di) => &mut self.dependents[di].core,
        };
        let entries = core.attr.path_entries.saturating_add_signed(delta);
        core.store_path(txn, p, entries)
    }

    /// Add a record to every path, after the unique checks.
    fn link(&mut self, txn: &mut Txn<'_>, rrn: u64, record: &[u8], null_map: &[u8]) -> Result<()> {
        self.refresh_dependents(txn)?;
        let entries = self.entries_of(rrn, record, null_map)?;
        let mut planned = Vec::new();
        for (which, key, unique) in entries {
            let Some((p, _)) = self.target(txn, which)? else {
                continue;
            };
            if unique && p.has_key(txn, &key.key)? {
                let (file, member) = match which {
                    None => (self.file.name(), self.core.name),
                    Some(di) => (self.dependents[di].lf.name(), self.dependents[di].core.name),
                };
                return Err(DbError::DuplicateKey {
                    file: file.to_string(),
                    member: member.to_string(),
                    key: hex(&key.key),
                });
            }
            planned.push((which, p, key));
        }
        for (which, mut p, key) in planned {
            let fresh = p.insert(txn, &key)?;
            self.store_target(txn, which, &p, i64::from(fresh))?;
        }
        Ok(())
    }

    /// Remove a record from every path.
    fn unlink(
        &mut self,
        txn: &mut Txn<'_>,
        rrn: u64,
        record: &[u8],
        null_map: &[u8],
    ) -> Result<()> {
        self.refresh_dependents(txn)?;
        for (which, key, _) in self.entries_of(rrn, record, null_map)? {
            let Some((mut p, _)) = self.target(txn, which)? else {
                continue;
            };
            let had = p.remove(txn, &key)?;
            self.store_target(txn, which, &p, -i64::from(had))?;
        }
        Ok(())
    }

    /// The member's access path.
    #[must_use]
    pub fn path(&self) -> Option<Path> {
        self.core.path()
    }

    /// The encoded key of a record of this member's format.
    pub fn key_of(&self, record: &[u8], null_map: &[u8]) -> Result<Vec<u8>> {
        path::record_key(self.format(), record, null_map)
    }

    /// Path entries within the bounds, ascending.
    pub fn keys(
        &self,
        txn: &mut Txn<'_>,
        lo: Bound<&[u8]>,
        hi: Bound<&[u8]>,
        max: usize,
    ) -> Result<Vec<PathKey>> {
        let Some(p) = self.usable_path()? else {
            return Ok(Vec::new());
        };
        p.scan(txn, lo, hi, max)
    }

    /// Path entries at or below `hi`, descending.
    pub fn keys_back(
        &self,
        txn: &mut Txn<'_>,
        hi: Bound<&[u8]>,
        max: usize,
    ) -> Result<Vec<PathKey>> {
        let Some(p) = self.usable_path()? else {
            return Ok(Vec::new());
        };
        p.scan_back(txn, hi, max)
    }

    fn usable_path(&self) -> Result<Option<Path>> {
        if self.core.attr.path_invalid {
            return Err(DbError::PathInvalid {
                file: self.file.name().to_string(),
                member: self.core.name.to_string(),
            });
        }
        Ok(self.core.path())
    }

    /// The record a path entry names: the physical slot, or the logical
    /// record derived from it (`None` when `DYNSLT` omits it).
    pub fn read_entry(&self, txn: &mut Txn<'_>, entry: &PathKey) -> Result<Option<Slot>> {
        if !self.is_logical() {
            return Ok(self.core.read_slot(txn, entry.rrn)?.filter(|s| !s.deleted));
        }
        let fi = usize::from(entry.format);
        let base = self.bases.get(fi).ok_or_else(|| DbError::Corrupt {
            what: format!("format {fi} has no based-on member"),
        })?;
        let core = Core::load(txn, base.member)?;
        let Some(slot) = core.read_slot(txn, entry.rrn)? else {
            return Ok(None);
        };
        if slot.deleted {
            return Ok(None);
        }
        let lf_fmt = &self.file.formats[fi];
        let log = self.logical(fi)?;
        let (record, null_map) =
            lf::derive(lf_fmt, log, &base.format, &slot.record, &slot.null_map)?;
        // Path entries passed static select/omit when they were built; an
        // entry without a key is an arrival-sequence read and is checked here.
        let check = self.file.attr.dynslt || entry.key.is_empty();
        if check && !lf::selected(lf_fmt, log, &record, &null_map)? {
            return Ok(None);
        }
        Ok(Some(Slot {
            rrn: entry.rrn,
            record,
            null_map,
            deleted: false,
        }))
    }

    /// Rebuild the path when it is invalid; true when it was.
    pub fn ensure_path(&mut self, txn: &mut Txn<'_>, progress: Progress<'_>) -> Result<bool> {
        if !self.core.attr.path_invalid {
            return Ok(false);
        }
        self.rebuild_path(txn, progress)?;
        Ok(true)
    }

    /// Rebuild the path from the records, by bulk load.
    pub fn rebuild_path(&mut self, txn: &mut Txn<'_>, progress: Progress<'_>) -> Result<()> {
        let Some(mut p) = self.core.path() else {
            self.core.attr.path_invalid = false;
            return self.core.save(txn);
        };
        let mut entries = Vec::new();
        if self.is_logical() {
            let total: u64 = {
                let mut t = 0;
                for b in &self.bases {
                    t += Core::load(txn, b.member)?.attr.slots;
                }
                t
            };
            let mut done = 0;
            for (fi, base) in self.bases.iter().enumerate() {
                let lf_fmt = &self.file.formats[fi];
                let log = self.logical(fi)?;
                let core = Core::load(txn, base.member)?;
                for rrn in 1..=core.attr.slots {
                    done += 1;
                    progress(done, total);
                    let Some(slot) = core.read_slot(txn, rrn)? else {
                        continue;
                    };
                    if slot.deleted {
                        continue;
                    }
                    let (rec, nulls) =
                        lf::derive(lf_fmt, log, &base.format, &slot.record, &slot.null_map)?;
                    if !self.file.attr.dynslt && !lf::selected(lf_fmt, log, &rec, &nulls)? {
                        continue;
                    }
                    entries.push(PathKey {
                        key: path::record_key(lf_fmt, &rec, &nulls)?,
                        format: u16::try_from(fi).expect("few"),
                        rrn,
                    });
                }
            }
        } else {
            let fmt = self.format().clone();
            let total = self.core.attr.slots;
            for rrn in 1..=total {
                progress(rrn, total);
                let Some(slot) = self.core.read_slot(txn, rrn)? else {
                    continue;
                };
                if slot.deleted {
                    continue;
                }
                entries.push(PathKey {
                    key: path::record_key(&fmt, &slot.record, &slot.null_map)?,
                    format: 0,
                    rrn,
                });
            }
        }
        let n = u64::try_from(entries.len()).expect("fits");
        p.load(txn, entries)?;
        self.core.attr.path_invalid = false;
        self.core.store_path(txn, &p, n)
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02X}");
        s
    })
}
