// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Object IDs inside payloads (`SAVE_FORMAT.md` `RefEntry`): found on
//! save, patched on restore.

use savfmt::RefEntry;
use store::obj::header::text_of;
use store::obj::{ObjectId, ops, type_by_code, types};
use store::txn::Txn;

/// Region codes.
pub const ATTR: u16 = 1;
/// The byte space.
pub const SPACE: u16 = 2;

/// Whether the byte space of a type holds IDs (definitions, not data).
#[must_use]
pub fn space_has_ids(type_code: u16) -> bool {
    matches!(type_code, types::FILE | crate::order::JRN)
}

/// Every object ID in `bytes`, with what it names. `self_id` (the
/// object's own ID) is not a reference.
pub fn find(
    txn: &mut Txn<'_>,
    region: u16,
    bytes: &[u8],
    self_id: ObjectId,
) -> store::Result<Vec<RefEntry>> {
    let asp = txn.store().asp_number().to_be_bytes();
    let next = txn.store().next_serial();
    let mut out = Vec::new();
    let mut o = 0;
    while o + 16 <= bytes.len() {
        let w = &bytes[o..o + 16];
        if w[..2] != asp {
            o += 1;
            continue;
        }
        let id = ObjectId::from_bytes(w);
        let plausible =
            id.serial > 0 && id.serial < next && type_by_code(id.type_code).is_some() && id != self_id;
        if plausible && let Some(r) = describe(txn, id)? {
            out.push(RefEntry {
                region,
                offset: o as u64,
                ..r
            });
            o += 16;
            continue;
        }
        o += 1;
    }
    Ok(out)
}

/// The names of object `id`, when it exists.
pub(crate) fn describe(txn: &mut Txn<'_>, id: ObjectId) -> store::Result<Option<RefEntry>> {
    let Ok((_, h)) = ops::read_header(txn, id) else {
        return Ok(None);
    };
    if h.id != id {
        return Ok(None);
    }
    let blank = [0x40u8; 10];
    let (library, name, member) = if ops::is_library(h.library) || h.library == h.id {
        let lib = ops::read_header(txn, h.library)?.1;
        (*lib.name.as_bytes(), *h.name.as_bytes(), blank)
    } else {
        // An owned object (a member): its file's names.
        let file = ops::read_header(txn, h.library)?.1;
        let lib = ops::read_header(txn, file.library)?.1;
        (*lib.name.as_bytes(), *file.name.as_bytes(), *h.name.as_bytes())
    };
    Ok(Some(RefEntry {
        original_id: id.to_bytes(),
        type_code: id.type_code,
        region: 0,
        library,
        name,
        member,
        offset: 0,
        flags: 0,
    }))
}

/// The object on this system that a reference names, when there is one:
/// in `first_lib` (a renamed restore's library) before the saved one.
pub fn resolve(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    r: &RefEntry,
    first_lib: Option<&str>,
) -> Option<ObjectId> {
    if let Some(l) = first_lib.and_then(|l| qsname::Name::parse(l).ok())
        && let Some(id) = resolve_in(txn, qsys, r, l)
    {
        return Some(id);
    }
    resolve_in(txn, qsys, r, qsname::Name::from_bytes(r.library))
}

/// Mark which references point into the stream, and which backwards.
pub(crate) fn mark(refs: &mut [RefEntry], pos: &std::collections::HashMap<[u8; 16], u32>, me: u32) {
    for r in refs {
        r.flags = 0;
        if let Some(p) = pos.get(&r.original_id) {
            r.flags |= RefEntry::IN_STREAM;
            if *p < me {
                r.flags |= RefEntry::EARLIER;
            }
        }
    }
}

fn resolve_in(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    r: &RefEntry,
    lib: qsname::Name,
) -> Option<ObjectId> {
    let name = qsname::Name::from_bytes(r.name);
    let lib = ops::find_library(txn, qsys, lib).ok()?.id;
    let l = store::obj::library::Library::load(txn, lib).ok()?;
    if r.member == [0x40; 10] {
        return l.lookup(txn, r.type_code, name).ok().flatten();
    }
    let file = l.lookup(txn, types::FILE, name).ok().flatten()?;
    let desc = db::file::describe(txn, file).ok()?;
    let want = text_of(&r.member);
    desc.members
        .iter()
        .find(|m| m.name.to_string() == want)
        .map(|m| m.object)
}
