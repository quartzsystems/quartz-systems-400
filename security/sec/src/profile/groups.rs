// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Group profiles: `GRPPRF` and `SUPGRPPRF` checked against the
//! profiles that exist, membership found from the profiles, and the
//! `UID`/`GID` numbers a profile and a group get.

use std::collections::HashSet;

use store::obj::ObjectId;
use store::txn::Txn;

use crate::profile::objects;
use crate::profile::payload::{MAX_SUPGRP, UsrPrf};
use crate::{Result, SecError};

/// The lowest generated `UID` or `GID`.
pub const FIRST_ID: u32 = 100;

/// Check `p`'s group profiles: each exists and is not `p`; supplemental
/// groups need a group profile, are unique and at most fifteen.
pub fn validate(txn: &mut Txn<'_>, qsys: ObjectId, p: &UsrPrf) -> Result<()> {
    let has_group = p.grpprf != "*NONE" && !p.grpprf.is_empty();
    if !has_group && !p.supgrpprf.is_empty() {
        return Err(SecError::msg(
            "CPF22A1",
            "Supplemental groups need a group profile (GRPPRF).",
        ));
    }
    if p.supgrpprf.len() > MAX_SUPGRP {
        return Err(SecError::msg(
            "CPF22A2",
            format!("At most {MAX_SUPGRP} supplemental groups are allowed."),
        ));
    }
    let mut seen = HashSet::new();
    for g in p.groups() {
        if g == p.name {
            return Err(SecError::msg(
                "CPF22A3",
                format!("{g} cannot be its own group profile."),
            ));
        }
        if !seen.insert(g.clone()) {
            return Err(SecError::msg(
                "CPF22A4",
                format!("Group profile {g} is named more than once."),
            ));
        }
        if !objects::exists(txn, qsys, &g) {
            return Err(objects::not_found(&g));
        }
    }
    Ok(())
}

/// Every profile whose group profiles include `group`.
pub fn members(txn: &mut Txn<'_>, qsys: ObjectId, group: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for name in objects::list(txn, qsys)? {
        let (_, p) = objects::read(txn, qsys, &name)?;
        if p.groups().iter().any(|g| g == group) {
            out.push(name);
        }
    }
    Ok(out)
}

/// Whether `name` is the group profile of any profile.
pub fn is_group(txn: &mut Txn<'_>, qsys: ObjectId, name: &str) -> Result<bool> {
    Ok(!members(txn, qsys, name)?.is_empty())
}

/// The next free `UID` (`which` false) or `GID` (true).
pub fn next_id(txn: &mut Txn<'_>, qsys: ObjectId, gid: bool) -> Result<u32> {
    let mut top = FIRST_ID - 1;
    for name in objects::list(txn, qsys)? {
        let (_, p) = objects::read(txn, qsys, &name)?;
        top = top.max(if gid { p.gid } else { p.uid });
    }
    Ok(top + 1)
}

/// Give each of `p`'s groups a `GID` if it has none, as IBM does when a
/// profile becomes a group.
pub fn ensure_gids(txn: &mut Txn<'_>, qsys: ObjectId, p: &UsrPrf) -> Result<()> {
    for g in p.groups() {
        let (id, mut gp) = objects::read(txn, qsys, &g)?;
        if gp.gid == 0 {
            gp.gid = next_id(txn, qsys, true)?;
            objects::write(txn, id, &gp)?;
        }
    }
    Ok(())
}
