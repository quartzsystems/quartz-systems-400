// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An object's payload as parts (`SAVE_FORMAT.md` §3): read out of the
//! store on save, written back on restore.

use std::collections::BTreeMap;

use savfmt::object::{decode_refs, encode_refs};
use savfmt::{Part, RefEntry, TreeImage};
use store::btree;
use store::obj::header::{ObjectHeader, TREE_ROOT_SLOTS};
use store::obj::space::Space;
use store::obj::{ObjectId, ops, types, usridx};
use store::txn::Txn;

use crate::Result;
use crate::ids;

/// An object read out of the store.
#[derive(Debug, Clone)]
pub struct Exported {
    /// Its header.
    pub header: ObjectHeader,
    /// Its parts.
    pub parts: BTreeMap<Part, Vec<u8>>,
}

impl Exported {
    /// Raw bytes of every part.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.parts.values().map(|p| p.len() as u64).sum()
    }
}

/// Zero the page numbers and counts a type keeps in its attribute area.
fn scrub_attr(type_code: u16, attr: &mut [u8]) {
    match type_code {
        types::LIB => {
            attr[0..8].fill(0);
            attr[24..32].fill(0);
        }
        types::USRIDX => attr[8..24].fill(0),
        _ => {}
    }
}

/// Trees a type keeps as indexes of other objects by ID (a profile's
/// owned, authorized and group objects; the objects an authorization
/// list secures): rebuilt where they are restored, never saved.
fn index_slot(type_code: u16, slot: usize) -> bool {
    match type_code {
        types::USRPRF => slot < 3,
        types::AUTL => slot == 0,
        _ => false,
    }
}

/// Read object `id` out as parts. `data`: false for `SAVFDTA(*NO)` and
/// the like (attributes only).
pub fn export(txn: &mut Txn<'_>, id: ObjectId, data: bool) -> Result<Exported> {
    let (page, header) = ops::read_header(txn, id)?;
    let tc = id.type_code;
    let mut parts = BTreeMap::new();
    let mut refs = Vec::new();
    let mut attr = ops::read_attr(txn, page)?;
    scrub_attr(tc, &mut attr);
    refs.extend(ids::find(txn, ids::ATTR, &attr)?);
    ids::zero(&mut attr, &refs, ids::ATTR);
    parts.insert(Part::ATTR, attr);
    if tc == types::USRIDX {
        let mut b = Vec::new();
        for e in usridx::first(txn, id, usize::MAX)? {
            b.extend_from_slice(&u16::try_from(e.len()).expect("entry").to_be_bytes());
            b.extend_from_slice(&e);
        }
        if !b.is_empty() {
            parts.insert(Part::typed(0), b);
        }
    } else if data && tc != types::LIB {
        let (space, _) = Space::load(txn, id)?;
        if space.bytes() > 0 {
            let len = usize::try_from(space.bytes()).expect("space fits memory");
            let mut bytes = space.read(txn, 0, len)?;
            if ids::space_has_ids(tc) {
                refs.extend(ids::find(txn, ids::SPACE, &bytes)?);
                ids::zero(&mut bytes, &refs, ids::SPACE);
            }
            parts.insert(Part::SPACE, bytes);
        }
        for (slot, root) in header.tree_roots.iter().enumerate() {
            if let Some(root) = root.filter(|_| !index_slot(tc, slot)) {
                let (key_len, entries) = btree::dump(txn, root)?;
                let t = TreeImage {
                    key_len: u16::try_from(key_len).expect("key"),
                    entries,
                };
                parts.insert(Part::tree(slot), t.encode());
            }
        }
    }
    // The object's SQL information goes with it: its name, definition
    // text, view or index attributes (format 2).
    if !header.sql.is_none()
        && let Some(info) = db::sqlinf::load(txn, id)?
    {
        parts.insert(Part::SQLINF, sqlinf_part(&info));
    }
    // A schema's SQL name, from QSYS's index.
    if tc == types::LIB
        && let Some((name, _)) = db::sqlinf::names::list(txn, header.library, db::sqlinf::names::Space::Schema)?.into_iter().find(|(_, t)| t.id == id)
    {
        let mut info = db::sqlinf::SqlInfo::new(db::sqlinf::Kind::Schema);
        info.set(db::sqlinf::tag::NAME, name.into_bytes());
        parts.insert(Part::SQLINF, sqlinf_part(&info));
    }
    if !refs.is_empty() {
        parts.insert(Part::REFS, encode_refs(&refs));
    }
    Ok(Exported { header, parts })
}

/// Set the stream flags of an exported object's references.
pub(crate) fn mark_refs(e: &mut Exported, pos: &std::collections::HashMap<[u8; 16], u32>, me: u32) {
    if let Some(b) = e.parts.get_mut(&Part::REFS)
        && let Ok(mut refs) = decode_refs(b)
    {
        ids::mark(&mut refs, pos, me);
        *b = encode_refs(&refs);
    }
}

/// The references a saved object lists.
pub fn refs(parts: &BTreeMap<Part, Vec<u8>>) -> Result<Vec<RefEntry>> {
    Ok(parts
        .get(&Part::REFS)
        .map(|b| decode_refs(b))
        .transpose()?
        .unwrap_or_default())
}

/// Write the saved parts into object `id`, replacing what it had. The
/// caller has patched the IDs in `parts` already.
pub fn import(txn: &mut Txn<'_>, id: ObjectId, parts: &BTreeMap<Part, Vec<u8>>) -> Result<()> {
    let (page, h) = ops::read_header(txn, id)?;
    let tc = id.type_code;
    let empty = Vec::new();
    let attr = parts.get(&Part::ATTR).unwrap_or(&empty);
    match tc {
        // A library keeps its own directory: only its kind, create
        // authority and ASP come back.
        types::LIB => {
            if attr.len() >= 24 {
                ops::write_attr(txn, page, 8, &attr[8..24])?;
            }
            return Ok(());
        }
        types::USRIDX => {
            usridx::drop_entries(txn, id)?;
            if !attr.is_empty() {
                ops::write_attr(txn, page, 0, attr)?;
            }
            let b = parts.get(&Part::typed(0)).unwrap_or(&empty);
            let mut at = 0;
            while at + 2 <= b.len() {
                let n = usize::from(u16::from_be_bytes([b[at], b[at + 1]]));
                usridx::insert(txn, id, &b[at + 2..at + 2 + n])?;
                at += 2 + n;
            }
            return Ok(());
        }
        _ => {}
    }
    // Trees first: freeing them does not touch the attribute area. A
    // type's indexes of other objects stay: they are this system's.
    for (slot, root) in h.tree_roots.iter().enumerate().take(TREE_ROOT_SLOTS) {
        if let Some(root) = root.as_ref().filter(|_| !index_slot(tc, slot)) {
            btree::free_tree(txn, *root)?;
            ops::set_tree_root(txn, page, slot, None)?;
        }
    }
    Space::truncate(txn, id, 0)?;
    if let Some(space) = parts.get(&Part::SPACE) {
        let s = Space::ensure(txn, id, space.len() as u64, 0)?;
        s.write(txn, 0, space)?;
    }
    for slot in 0..TREE_ROOT_SLOTS {
        if let Some(b) = parts.get(&Part::tree(slot)) {
            let t = TreeImage::decode(b)?;
            let root = btree::load(txn, id, usize::from(t.key_len), t.entries)?;
            ops::set_tree_root(txn, page, slot, root)?;
        }
    }
    if !attr.is_empty() {
        ops::write_attr(txn, page, 0, attr)?;
    }
    Ok(())
}

/// Put `new` at a reference's place in the parts.
pub fn patch(parts: &mut BTreeMap<Part, Vec<u8>>, r: &RefEntry, new: ObjectId) {
    let part = if r.region == ids::ATTR {
        Part::ATTR
    } else {
        Part::SPACE
    };
    if let Some(b) = parts.get_mut(&part) {
        let at = usize::try_from(r.offset).unwrap_or(usize::MAX);
        if at + 16 <= b.len() {
            b[at..at + 16].copy_from_slice(&new.to_bytes());
        }
    }
}

/// Patch a reference in an object already written to the store.
pub fn patch_stored(txn: &mut Txn<'_>, id: ObjectId, r: &RefEntry, new: ObjectId) -> Result<()> {
    let at = usize::try_from(r.offset).unwrap_or(usize::MAX);
    if r.region == ids::ATTR {
        let (page, _) = ops::read_header(txn, id)?;
        if at + 16 <= store::obj::header::ATTR_LEN {
            ops::write_attr(txn, page, at, &new.to_bytes())?;
        }
    } else {
        let (s, _) = Space::load(txn, id)?;
        if (at + 16) as u64 <= s.bytes() {
            s.write(txn, at as u64, &new.to_bytes())?;
        }
    }
    Ok(())
}

/// The `sqlinf` part: the `SqlInfAttr`, then the entries.
fn sqlinf_part(info: &db::sqlinf::SqlInfo) -> Vec<u8> {
    use db::sqlinf::entry;
    let body = entry::encode(&info.entries);
    let count = u32::try_from(info.entries.len()).expect("entries fit");
    let mut b = entry::encode_attr(info.kind, count, body.len() as u64).to_vec();
    b.extend_from_slice(&body);
    b
}

/// Give a restored object the SQL information it was saved with, or
/// none when it had none, and its SQL name its place in the library's
/// name index again.
pub fn import_sqlinf(txn: &mut Txn<'_>, job: store::obj::JobHandle, id: ObjectId, parts: &BTreeMap<Part, Vec<u8>>, now: u64) -> Result<()> {
    use db::sqlinf::{self, entry, names};
    names::forget(txn, id)?;
    let Some(b) = parts.get(&Part::SQLINF) else {
        sqlinf::clear(txn, job, id)?;
        return Ok(());
    };
    let head = b.get(..entry::ATTR_LEN).ok_or_else(|| db::DbError::Corrupt { what: "saved SQL information".into() })?;
    let (kind, count, len) = entry::decode_attr(head)?;
    let body = b.get(entry::ATTR_LEN..entry::ATTR_LEN + usize::try_from(len).unwrap_or(usize::MAX)).ok_or_else(|| db::DbError::Corrupt { what: "saved SQL information".into() })?;
    let mut info = sqlinf::SqlInfo { kind, entries: entry::decode(body, count)? };
    let (_, h) = ops::read_header(txn, id)?;
    // Routines are found by their specific names; an SQL routine's
    // program is the object itself, in the library it is restored to.
    let lib_name = ops::read_header(txn, h.library)?.1.name.to_string();
    for mut r in sqlinf::routine::Routine::all(&info)? {
        if (r.def.body == 1 || r.def.routine_type == 2) && r.def.program_library != lib_name {
            r.def.program_library.clone_from(&lib_name);
            sqlinf::routine::Routine::replace(&mut info, &r.def.specific.clone(), Some(&r))?;
        }
        names::put(txn, h.library, names::Space::Specific, &r.def.specific, names::Target { id, system: h.name })?;
    }
    sqlinf::save(txn, id, &info, now)?;
    if let Some(name) = info.name() {
        names::put(txn, h.library, kind.space(), &name, names::Target { id, system: h.name })?;
    }
    Ok(())
}

/// A restored library's schema name back in QSYS's index.
pub fn import_schema_name(txn: &mut Txn<'_>, qsys: ObjectId, lib: ObjectId, parts: &BTreeMap<Part, Vec<u8>>) -> Result<()> {
    use db::sqlinf::{entry, names};
    let Some(b) = parts.get(&Part::SQLINF) else {
        return Ok(());
    };
    let Some(head) = b.get(..entry::ATTR_LEN) else {
        return Ok(());
    };
    let (_, count, len) = entry::decode_attr(head)?;
    let body = b.get(entry::ATTR_LEN..entry::ATTR_LEN + usize::try_from(len).unwrap_or(usize::MAX)).unwrap_or(&[]);
    let info = db::sqlinf::SqlInfo { kind: db::sqlinf::Kind::Schema, entries: entry::decode(body, count)? };
    if let Some(name) = info.name() {
        let (_, h) = ops::read_header(txn, lib)?;
        names::put(txn, qsys, names::Space::Schema, &name, names::Target { id: lib, system: h.name })?;
    }
    Ok(())
}
