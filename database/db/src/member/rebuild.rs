// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A member rebuilt beside itself (`docs/DB_FORMAT.md` §4): a new
//! `*MBR` the old one names in `MbrAttr.rebuild` takes the records in
//! parts, then one short step puts it in the old one's place. Until
//! then the old member is as it was, so a rebuild cut short loses
//! nothing; what it made is deleted the next time.

use store::obj::header::text_of;
use store::obj::ops::{self, CreateParams};
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

use super::{Core, MemberParams, create};
use crate::error::Result;
use crate::file::{FileDesc, MemberEntry};
use crate::format::FormatDef;

/// Make the member that will replace `entry` of `file`, holding
/// `format`'s records, and name it in the old member.
pub fn begin(txn: &mut Txn<'_>, file: &FileDesc, entry: &MemberEntry, format: &FormatDef) -> Result<ObjectId> {
    let (_, old_h) = ops::read_header(txn, entry.object)?;
    let params = CreateParams {
        library: file.header.library,
        name: entry.name,
        type_code: store::obj::types::MBR,
        attribute: String::new(),
        text: text_of(&old_h.text),
        owner: text_of(&file.header.owner),
        system: text_of(&file.header.system),
        now: old_h.created,
        by_system: true,
    };
    let mp = MemberParams {
        format_index: 0,
        max_records: file.attr.size.max_records(),
        increment: file.attr.size.increment,
        reuse_deleted: file.attr.reusedlt,
    };
    let new = create(txn, file.id, format, mp, &params)?;
    // The new member goes on where the old one was.
    let mut old = Core::load(txn, entry.object)?;
    let mut core = Core::load(txn, new)?;
    if old.attr.identity_next != 0 {
        core.attr.identity_next = old.attr.identity_next;
    }
    core.attr.last_save = old.attr.last_save;
    core.attr.last_restore = old.attr.last_restore;
    core.save(txn)?;
    let (page, mut h) = ops::read_header(txn, new)?;
    h.changed = old_h.changed;
    ops::write_header(txn, page, &h)?;
    old.attr.rebuild = new;
    old.save(txn)?;
    Ok(new)
}

/// Delete what an unfinished rebuild of `member` made; true when there
/// was one.
pub fn abandon(txn: &mut Txn<'_>, job: JobHandle, member: ObjectId) -> Result<bool> {
    let mut core = Core::load(txn, member)?;
    if core.attr.rebuild == ObjectId::NONE {
        return Ok(false);
    }
    let new = core.attr.rebuild;
    core.attr.rebuild = ObjectId::NONE;
    core.save(txn)?;
    if ops::read_header(txn, new).is_ok() {
        ops::delete(txn, job, new)?;
    }
    Ok(true)
}

/// Put the rebuilt member in the old one's place: the old member is
/// deleted; the caller names the new one in the member table.
pub fn finish(txn: &mut Txn<'_>, job: JobHandle, member: ObjectId) -> Result<ObjectId> {
    let mut core = Core::load(txn, member)?;
    let new = core.attr.rebuild;
    core.attr.rebuild = ObjectId::NONE;
    core.save(txn)?;
    ops::delete(txn, job, member)?;
    Ok(new)
}
