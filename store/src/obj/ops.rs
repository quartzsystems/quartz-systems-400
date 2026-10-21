// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operations every registered type gets: create, delete, rename,
//! move, change text, describe, list, resolve; and the storage helpers
//! the payload modules build on.

use qsname::{LibSpec, Name};
use tracing::debug;

use crate::alloc::Run;
use crate::obj::ObjectId;
use crate::obj::dir::{self, DirKey};
use crate::obj::header::{self, ATTR_OFF, ObjectHeader, text};
use crate::obj::library::{LibAttr, LibType, Library};
use crate::obj::lock::{JobHandle, LockState};
use crate::obj::types::{self, type_by_code};
use crate::page::pager::{PageNo, Pager};
use crate::page::{Page, PageType};
use crate::txn::Txn;
use crate::{Result, StoreError};

/// What `create` needs.
#[derive(Debug, Clone)]
pub struct CreateParams {
    /// Containing library.
    pub library: ObjectId,
    /// Name.
    pub name: Name,
    /// Type code.
    pub type_code: u16,
    /// Extended attribute, or empty.
    pub attribute: String,
    /// Text description.
    pub text: String,
    /// Owner and creator profile name.
    pub owner: String,
    /// Created-on system name.
    pub system: String,
    /// Unix seconds.
    pub now: u64,
    /// By the system itself: allows non-user-creatable types and `Q` names.
    pub by_system: bool,
}

/// A resolved object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Found {
    /// The object.
    pub id: ObjectId,
    /// The library it was found in.
    pub library: ObjectId,
    /// Its header page.
    pub page: PageNo,
}

/// What `*LIBL` and `*CURLIB` mean for a caller. The storage service
/// builds it from the requesting job's library list.
#[derive(Debug, Clone, Default)]
pub struct NameContext {
    /// Current library, if any.
    pub curlib: Option<ObjectId>,
    /// Library list, in search order.
    pub libl: Vec<ObjectId>,
}

/// The name QSYS.
pub fn qsys_name() -> Name {
    Name::parse("QSYS").expect("valid")
}

fn not_found(name: Name, library: &str) -> StoreError {
    StoreError::ObjectNotFound {
        name: name.to_string(),
        library: library.to_string(),
    }
}

/// Read an object's header and its page.
pub fn read_header(txn: &mut Txn<'_>, id: ObjectId) -> Result<(PageNo, ObjectHeader)> {
    let page = dir::header_page(txn, id)?;
    let p = txn.read(page)?;
    p.verify_type(page, PageType::ObjectHeader)?;
    let h = ObjectHeader::decode(p.body(), page)?;
    if h.id != id {
        return Err(StoreError::Corrupt {
            page,
            what: format!("header belongs to {} not {id}", h.id),
        });
    }
    Ok((page, h))
}

/// Write the fixed part of a header in place.
pub fn write_header(txn: &mut Txn<'_>, page: PageNo, h: &ObjectHeader) -> Result<()> {
    let mut body = vec![0u8; header::FIXED_LEN];
    h.encode(&mut body);
    txn.patch(page, 0, &body)
}

/// Name of a library by ID, for messages.
fn library_name(txn: &mut Txn<'_>, id: ObjectId) -> String {
    read_header(txn, id).map_or_else(|_| id.to_string(), |(_, h)| h.name.to_string())
}

/// Create the root library QSYS: the first object, serial 1, in itself.
pub fn bootstrap(txn: &mut Txn<'_>, now: u64, system: &str) -> Result<ObjectId> {
    bootstrap_root(txn, now, system, qsys_name())
}

/// Create a root library called `name` (QSYS, or QTEMP in the
/// temporary ASP): the first object, serial 1, in itself.
pub fn bootstrap_root(txn: &mut Txn<'_>, now: u64, system: &str, name: Name) -> Result<ObjectId> {
    if txn.objdir_root().is_some() {
        return Err(StoreError::Corrupt {
            page: 0,
            what: "bootstrap on a store that has objects".into(),
        });
    }
    let asp = txn.store().asp_number();
    let serial = txn.take_serial();
    let id = ObjectId {
        asp,
        type_code: types::LIB,
        serial,
        generation: 0,
    };
    let page = txn.alloc()?;
    let mut h = ObjectHeader::new(id, id, name, now);
    h.text = text("System library")?;
    h.owner = text("QSYS")?;
    h.creator = text("QSYS")?;
    h.system = text(system)?;
    h.domain = header::Domain::System;
    h.user_state = false;
    h.public_authority = header::Authority::USE;
    let mut p = Page::zeroed();
    h.encode(p.body_mut());
    LibAttr::new(LibType::Prod, asp)?.encode(p.body_mut());
    txn.write(page, &mut p, PageType::ObjectHeader, id)?;
    dir::objdir_insert(txn, id, page)?;
    let mut lib = Library::load(txn, id)?;
    lib.add(txn, types::LIB, name, id)?;
    debug!("objects: root library {name} is {id} at page {page}");
    Ok(id)
}

/// The library called `name`.
pub fn find_library(txn: &mut Txn<'_>, qsys: ObjectId, name: Name) -> Result<Found> {
    let lib = Library::load(txn, qsys)?;
    let id = lib
        .lookup(txn, types::LIB, name)?
        .ok_or_else(|| StoreError::LibraryNotFound {
            name: name.to_string(),
        })?;
    let page = dir::header_page(txn, id)?;
    Ok(Found {
        id,
        library: qsys,
        page,
    })
}

/// Create an object of any registered type with an empty payload.
pub fn create(txn: &mut Txn<'_>, params: &CreateParams) -> Result<ObjectId> {
    let info = type_by_code(params.type_code).ok_or_else(|| StoreError::Corrupt {
        page: 0,
        what: format!("unknown type code {:#06x}", params.type_code),
    })?;
    if !params.by_system {
        if !info.user_creatable {
            return Err(StoreError::NotAuthorized {
                name: params.name.to_string(),
                library: library_name(txn, params.library),
            });
        }
        if info.q_reserved && params.name.as_bytes()[0] == 0xD8 {
            return Err(StoreError::InvalidName {
                name: params.name.to_string(),
            });
        }
    }
    let mut lib = Library::load(txn, params.library)?;
    let lib_header = read_header(txn, params.library)?.1;
    // ASP 0's root holds each job's QTEMP library.
    if info.qsys_only && lib_header.name != qsys_name() && txn.store().asp_number() != 0 {
        return Err(StoreError::InvalidName {
            name: format!("{}/{} {}", lib_header.name, params.name, info.name),
        });
    }
    if lib.lookup(txn, params.type_code, params.name)?.is_some() {
        return Err(StoreError::AlreadyExists {
            name: params.name.to_string(),
            library: lib_header.name.to_string(),
        });
    }
    let id = ObjectId {
        asp: txn.store().asp_number(),
        type_code: params.type_code,
        serial: txn.take_serial(),
        generation: 0,
    };
    let page = txn.alloc()?;
    let mut h = ObjectHeader::new(id, params.library, params.name, params.now);
    h.attribute = text(&params.attribute)?;
    h.text = text(&params.text)?;
    h.owner = text(&params.owner)?;
    h.creator = text(&params.owner)?;
    h.system = text(&params.system)?;
    let mut p = Page::zeroed();
    h.encode(p.body_mut());
    if params.type_code == types::LIB {
        LibAttr::new(LibType::Prod, id.asp)?.encode(p.body_mut());
    }
    txn.write(page, &mut p, PageType::ObjectHeader, id)?;
    dir::objdir_insert(txn, id, page)?;
    lib.add(txn, params.type_code, params.name, id)?;
    Ok(id)
}

/// Create an object owned by another object rather than by a library
/// (a file member): its `library` field names the owner and it has no
/// library directory entry. Only the owner's type reaches it.
pub fn create_owned(txn: &mut Txn<'_>, params: &CreateParams, owner: ObjectId) -> Result<ObjectId> {
    let info = type_by_code(params.type_code).ok_or_else(|| StoreError::Corrupt {
        page: 0,
        what: format!("unknown type code {:#06x}", params.type_code),
    })?;
    let owner_header = read_header(txn, owner)?.1;
    if owner_header.id.type_code == types::LIB || info.user_creatable {
        return Err(StoreError::BadValue {
            what: format!("{} cannot be owned by {}", info.name, owner_header.name),
        });
    }
    let id = ObjectId {
        asp: txn.store().asp_number(),
        type_code: params.type_code,
        serial: txn.take_serial(),
        generation: 0,
    };
    let page = txn.alloc()?;
    let mut h = ObjectHeader::new(id, owner, params.name, params.now);
    h.attribute = text(&params.attribute)?;
    h.text = text(&params.text)?;
    h.owner = text(&params.owner)?;
    h.creator = text(&params.owner)?;
    h.system = text(&params.system)?;
    h.temporary = owner_header.temporary;
    let mut p = Page::zeroed();
    h.encode(p.body_mut());
    txn.write(page, &mut p, PageType::ObjectHeader, id)?;
    dir::objdir_insert(txn, id, page)?;
    Ok(id)
}

/// Whether `id` is a library (an owned object's `library` is not).
#[must_use]
pub fn is_library(id: ObjectId) -> bool {
    id.type_code == types::LIB
}

/// Publish one of an object's tree roots in place (8 bytes patched).
pub fn set_tree_root(
    txn: &mut Txn<'_>,
    page: PageNo,
    slot: usize,
    root: Option<PageNo>,
) -> Result<()> {
    assert!(slot < header::TREE_ROOT_SLOTS, "tree root slot {slot}");
    let at = header::OFF_TREE_ROOTS + slot * 8;
    txn.patch(page, at, &root.unwrap_or(0).to_le_bytes())
}

/// Delete an object: its payload pages, its header and its directory
/// entries. Needs `*EXCL`, which is taken for `job` for the duration.
/// A library must be empty.
pub fn delete(txn: &mut Txn<'_>, job: JobHandle, id: ObjectId) -> Result<()> {
    let (page, h) = read_header(txn, id)?;
    let lib_name = library_name(txn, h.library);
    if let Err(held) = txn.store().locks_mut().lock(job, id, LockState::Exclusive) {
        debug!(
            "objects: delete of {} blocked by job {:?}",
            h.name, held.job
        );
        return Err(StoreError::Locked {
            name: h.name.to_string(),
            library: lib_name,
        });
    }
    let result = delete_locked(txn, id, page, &h);
    txn.store().locks_mut().release_object(id);
    result
}

fn delete_locked(txn: &mut Txn<'_>, id: ObjectId, page: PageNo, h: &ObjectHeader) -> Result<()> {
    if h.id.type_code == types::LIB {
        let mut lib = Library::load(txn, id)?;
        let own = u64::from(id == h.library);
        if lib.attr.object_count > own {
            return Err(StoreError::Locked {
                name: h.name.to_string(),
                library: library_name(txn, h.library),
            });
        }
        if own != 0 {
            lib.remove(txn, types::LIB, h.name)?;
        }
        lib.free_dir(txn)?;
    }
    for run in all_extents(txn, h)? {
        txn.free_run(run.start, run.len)?;
    }
    if let Some(p) = h.extent_overflow {
        txn.free_run(p, 1)?;
    }
    if let Some(p) = h.private_overflow {
        txn.free_run(p, 1)?;
    }
    for root in h.tree_roots.into_iter().flatten() {
        crate::btree::free_tree(txn, root)?;
    }
    if id != h.library && is_library(h.library) {
        let mut lib = Library::load(txn, h.library)?;
        lib.remove(txn, h.id.type_code, h.name)?;
    }
    dir::objdir_remove(txn, id)?;
    txn.free_run(page, 1)?;
    Ok(())
}

/// Rename within the library.
pub fn rename(txn: &mut Txn<'_>, id: ObjectId, new_name: Name, now: u64) -> Result<()> {
    let (page, mut h) = read_header(txn, id)?;
    let mut lib = Library::load(txn, h.library)?;
    if lib.lookup(txn, id.type_code, new_name)?.is_some() {
        return Err(StoreError::AlreadyExists {
            name: new_name.to_string(),
            library: library_name(txn, h.library),
        });
    }
    lib.remove(txn, id.type_code, h.name)?;
    lib.add(txn, id.type_code, new_name, id)?;
    h.name = new_name;
    h.changed = now;
    write_header(txn, page, &h)
}

/// Move to another library.
pub fn move_to(txn: &mut Txn<'_>, id: ObjectId, to: ObjectId, now: u64) -> Result<()> {
    let (page, mut h) = read_header(txn, id)?;
    if id.type_code == types::LIB {
        return Err(StoreError::NotAuthorized {
            name: h.name.to_string(),
            library: library_name(txn, h.library),
        });
    }
    let mut dest = Library::load(txn, to)?;
    if dest.lookup(txn, id.type_code, h.name)?.is_some() {
        return Err(StoreError::AlreadyExists {
            name: h.name.to_string(),
            library: library_name(txn, to),
        });
    }
    let mut from = Library::load(txn, h.library)?;
    from.remove(txn, id.type_code, h.name)?;
    dest.add(txn, id.type_code, h.name, id)?;
    h.library = to;
    h.changed = now;
    write_header(txn, page, &h)
}

/// Change the text description.
pub fn change_text(txn: &mut Txn<'_>, id: ObjectId, new_text: &str, now: u64) -> Result<()> {
    let (page, mut h) = read_header(txn, id)?;
    h.text = text(new_text)?;
    h.changed = now;
    write_header(txn, page, &h)
}

/// The header, for a description display.
pub fn describe(txn: &mut Txn<'_>, id: ObjectId) -> Result<ObjectHeader> {
    Ok(read_header(txn, id)?.1)
}

/// Entries of a library, in key order (type, then EBCDIC name).
pub fn list(
    txn: &mut Txn<'_>,
    library: ObjectId,
    type_code: Option<u16>,
) -> Result<Vec<(DirKey, ObjectId)>> {
    Library::load(txn, library)?.list(txn, type_code)
}

/// Every library, in EBCDIC name order.
pub fn libraries(txn: &mut Txn<'_>, qsys: ObjectId) -> Result<Vec<(Name, ObjectId)>> {
    Ok(list(txn, qsys, Some(types::LIB))?
        .into_iter()
        .map(|(k, id)| (k.name, id))
        .collect())
}

/// Resolve `lib/name type` in `ctx`. `*LIBL` walks the list in order
/// and returns the first match.
pub fn resolve(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    ctx: &NameContext,
    lib: LibSpec,
    name: Name,
    type_code: u16,
) -> Result<Found> {
    let candidates: Vec<ObjectId> = match lib {
        LibSpec::Name(n) => vec![find_library(txn, qsys, n)?.id],
        LibSpec::Curlib => vec![ctx.curlib.ok_or_else(|| StoreError::LibraryNotFound {
            name: "*CURLIB".into(),
        })?],
        LibSpec::Libl => ctx.libl.clone(),
    };
    for library in candidates {
        let l = Library::load(txn, library)?;
        if let Some(id) = l.lookup(txn, type_code, name)? {
            let page = dir::header_page(txn, id)?;
            return Ok(Found { id, library, page });
        }
    }
    Err(not_found(name, &lib.to_string()))
}

/// Every extent of an object, inline and overflow.
pub fn all_extents(txn: &mut Txn<'_>, h: &ObjectHeader) -> Result<Vec<Run>> {
    let mut v = h.extents.clone();
    if let Some(p) = h.extent_overflow {
        let page = txn.read(p)?;
        page.verify_type(p, PageType::ExtentOverflow)?;
        v.extend(header::decode_overflow(page.body(), p)?);
    }
    Ok(v)
}

/// Grow an object's storage by `pages`, extending its last extent in
/// place when possible. Returns the runs added.
pub fn grow(txn: &mut Txn<'_>, id: ObjectId, pages: u64) -> Result<Vec<Run>> {
    let (page, mut h) = read_header(txn, id)?;
    let mut extents = all_extents(txn, &h)?;
    let mut added = Vec::new();
    let mut need = pages;
    if let Some(last) = extents.last_mut() {
        let got = txn.extend_run(last.start + last.len, need);
        if got > 0 {
            added.push(Run {
                start: last.start + last.len,
                len: got,
            });
            last.len += got;
            need -= got;
        }
    }
    while need > 0 {
        let run = txn.alloc_run(need)?;
        added.push(run);
        extents.push(run);
        need -= run.len;
    }
    h.size_pages += pages;
    save_extents(txn, page, &mut h, &extents)?;
    Ok(added)
}

/// Give back the last `pages` pages of an object's storage.
pub fn shrink(txn: &mut Txn<'_>, id: ObjectId, pages: u64) -> Result<()> {
    let (page, mut h) = read_header(txn, id)?;
    let mut extents = all_extents(txn, &h)?;
    let mut left = pages.min(h.size_pages);
    while left > 0 {
        let last = extents.last_mut().expect("size_pages counts extents");
        let cut = left.min(last.len);
        txn.free_run(last.start + last.len - cut, cut)?;
        last.len -= cut;
        left -= cut;
        if last.len == 0 {
            extents.pop();
        }
    }
    h.size_pages -= pages.min(h.size_pages);
    save_extents(txn, page, &mut h, &extents)
}

/// Store an extent list: inline, then one overflow page.
pub(crate) fn save_extents(
    txn: &mut Txn<'_>,
    page: PageNo,
    h: &mut ObjectHeader,
    extents: &[Run],
) -> Result<()> {
    if extents.len() > header::EXTENT_SLOTS + header::OVERFLOW_SLOTS {
        return Err(StoreError::NoSpace { asp: h.id.asp });
    }
    let (inline, rest) = extents.split_at(extents.len().min(header::EXTENT_SLOTS));
    h.extents = inline.to_vec();
    if rest.is_empty() {
        if let Some(p) = h.extent_overflow.take() {
            txn.free_run(p, 1)?;
        }
    } else {
        // Rewrite the overflow page copy-on-write.
        if let Some(p) = h.extent_overflow.take() {
            txn.free_run(p, 1)?;
        }
        let p = txn.alloc()?;
        let mut op = Page::zeroed();
        header::encode_overflow(op.body_mut(), rest);
        txn.write(p, &mut op, PageType::ExtentOverflow, h.id)?;
        h.extent_overflow = Some(p);
    }
    write_header(txn, page, h)
}

/// Read bytes of the attribute area.
pub fn read_attr(txn: &mut Txn<'_>, page: PageNo) -> Result<Vec<u8>> {
    let p = txn.read(page)?;
    Ok(p.body()[ATTR_OFF..ATTR_OFF + header::ATTR_LEN].to_vec())
}

/// Patch bytes of the attribute area.
pub fn write_attr(txn: &mut Txn<'_>, page: PageNo, offset: usize, bytes: &[u8]) -> Result<()> {
    assert!(offset + bytes.len() <= header::ATTR_LEN);
    txn.patch(page, ATTR_OFF + offset, bytes)
}
