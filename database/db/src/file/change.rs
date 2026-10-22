// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Changing a file after creation: attributes, record formats, members.

use qsname::Name;
use store::obj::ObjectId;
use store::obj::ops;
use store::obj::space::Space;
use store::txn::Txn;

use super::{FileAttr, FileDesc, describe, save_attr, write_members};
use crate::error::{DbError, Result};
use crate::format::{FormatDef, sections};

/// Change the attribute area through `f`; the layout fields stay.
pub fn change_attr(
    txn: &mut Txn<'_>,
    file: ObjectId,
    f: &mut dyn FnMut(&mut FileAttr),
) -> Result<FileDesc> {
    let mut desc = describe(txn, file)?;
    let keep = desc.attr.clone();
    f(&mut desc.attr);
    let a = &mut desc.attr;
    a.kind = keep.kind;
    a.record_len = keep.record_len;
    a.format_count = keep.format_count;
    a.member_count = keep.member_count;
    a.def_len = keep.def_len;
    a.formats_off = keep.formats_off;
    a.formats_len = keep.formats_len;
    a.members_off = keep.members_off;
    a.member_cap = keep.member_cap;
    a.dependent_count = keep.dependent_count;
    a.dependents_off = keep.dependents_off;
    a.dependent_cap = keep.dependent_cap;
    save_attr(txn, &desc)?;
    Ok(desc)
}

/// Replace the format sections. The new blob goes at the end of the
/// definition space; the old one is left behind.
pub fn replace_formats(txn: &mut Txn<'_>, file: ObjectId, formats: &[FormatDef]) -> Result<()> {
    let mut desc = describe(txn, file)?;
    if formats.len() != desc.formats.len() {
        return Err(DbError::Definition {
            what: format!(
                "{} formats replace {}; the count cannot change",
                formats.len(),
                desc.formats.len()
            ),
        });
    }
    let blob = sections::encode(formats);
    let off = desc.attr.def_len;
    let len = u64::try_from(blob.len()).expect("fits");
    let space = Space::ensure(txn, file, off + len, 0)?;
    space.write(txn, off, &blob)?;
    desc.attr.formats_off = off;
    desc.attr.formats_len = len;
    desc.attr.def_len = off + len;
    desc.attr.record_len = formats
        .iter()
        .map(|f| f.format.record_len)
        .max()
        .unwrap_or(0);
    save_attr(txn, &desc)
}

/// `CHGPFM`/`CHGLFM`: the member's text and, for a source member, its
/// `SRCTYPE` (`Some(None)` removes it).
pub fn chgpfm(
    txn: &mut Txn<'_>,
    file: ObjectId,
    member: Name,
    text: Option<&str>,
    srctype: Option<Option<Name>>,
    now: u64,
) -> Result<()> {
    crate::auth::check(txn, "file.member", file)?;
    let mut desc = describe(txn, file)?;
    let at = desc
        .members
        .iter()
        .position(|m| m.name == member)
        .ok_or_else(|| DbError::MemberNotFound {
            file: desc.name().to_string(),
            member: member.to_string(),
        })?;
    if let Some(t) = text {
        ops::change_text(txn, desc.members[at].object, t, now)?;
    }
    if let Some(st) = srctype {
        desc.members[at].srctype = st;
        write_members(txn, &mut desc)?;
    }
    Ok(())
}
