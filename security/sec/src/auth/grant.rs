// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Changing an object's authority: private authorities, `*PUBLIC`,
//! the owner, the primary group, the authorization list. Each keeps the
//! object header and the profiles' indexes in step and bumps the
//! object's authority generation.

use store::obj::header::{Authority as A, ObjectHeader, text};
use store::obj::{ObjectId, auth};
use store::txn::Txn;

use crate::auth::autl;
use crate::auth::bits::Auth;
use crate::profile::index::{self, Kind};
use crate::profile::objects;
use crate::rec::text_of;
use crate::{Result, SecError};

/// Bytes of a page, for the owner's storage tally.
pub const PAGE_KB: i64 = 4;

fn key(name: &str) -> Result<[u8; 10]> {
    Ok(text(name)?)
}

/// The header of `id`.
pub fn header(txn: &mut Txn<'_>, id: ObjectId) -> Result<ObjectHeader> {
    Ok(auth::load(txn, id)?.1)
}

/// Add `kb` (negative: take away) to `owner`'s storage tally; growth
/// past `MAXSTG` is refused.
pub fn charge(txn: &mut Txn<'_>, qsys: ObjectId, owner: &str, kb: i64) -> Result<()> {
    if kb == 0 || owner.is_empty() {
        return Ok(());
    }
    let Some(pid) = objects::find(txn, qsys, owner)? else {
        return Ok(());
    };
    let mut p = objects::read_id(txn, pid)?;
    crate::profile::limits::storage(&p, kb)?;
    p.stg_used = p.stg_used.saturating_add_signed(kb);
    objects::write(txn, pid, &p)
}

/// Set (`Some`) or remove (`None`) `profile`'s private authority.
pub fn set_private(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    id: ObjectId,
    profile: &str,
    bits: Option<Auth>,
) -> Result<()> {
    if !objects::exists(txn, qsys, profile) {
        return Err(objects::not_found(profile));
    }
    let (page, mut h) = auth::load(txn, id)?;
    auth::private_set(txn, page, &mut h, key(profile)?, bits.map(Auth::store))?;
    match bits {
        Some(b) => index::put(txn, qsys, profile, Kind::Private, id, u64::from(b.0))?,
        None => index::remove(txn, qsys, profile, Kind::Private, id)?,
    }
    Ok(())
}

/// `profile`'s private authority to `h`, if any.
pub fn private(txn: &mut Txn<'_>, h: &ObjectHeader, profile: &str) -> Result<Option<Auth>> {
    Ok(auth::private_get(txn, h, key(profile)?)?.map(Auth::of))
}

/// Set `*PUBLIC`; `None` takes it from the authorization list.
pub fn set_public(txn: &mut Txn<'_>, id: ObjectId, bits: Option<Auth>) -> Result<()> {
    let (page, mut h) = auth::load(txn, id)?;
    h.public_authority = if let Some(b) = bits {
        b.store()
    } else {
        if text_of(&h.autl).is_empty() {
            return Err(SecError::msg(
                "CPF2254",
                format!("{} is not secured by an authorization list.", h.name),
            ));
        }
        A(h.public_authority.0 | A::FROM_AUTL)
    };
    auth::save_authority(txn, page, &mut h).map_err(Into::into)
}

/// Reduce or restore the owner's own authority.
pub fn set_owner_authority(txn: &mut Txn<'_>, id: ObjectId, bits: Auth) -> Result<()> {
    let (page, mut h) = auth::load(txn, id)?;
    h.owner_authority = bits.store();
    auth::save_authority(txn, page, &mut h).map_err(Into::into)
}

/// Change the owner (`CHGOBJOWN`). The new owner loses any private
/// authority it had (the owner's authority replaces it); the old owner
/// keeps `*ALL` privately unless `revoke`.
pub fn set_owner(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    id: ObjectId,
    owner: &str,
    revoke: bool,
) -> Result<()> {
    if !objects::exists(txn, qsys, owner) {
        return Err(objects::not_found(owner));
    }
    let (page, mut h) = auth::load(txn, id)?;
    let old = text_of(&h.owner);
    if old == owner {
        return Ok(());
    }
    if text_of(&h.primary_group) == owner {
        return Err(SecError::msg(
            "CPF22A6",
            format!(
                "{owner} is the primary group of {} and cannot own it.",
                h.name
            ),
        ));
    }
    let size = i64::try_from(h.size_pages).unwrap_or(0) * PAGE_KB;
    charge(txn, qsys, owner, size)?;
    charge(txn, qsys, &old, -size)?;
    h.owner = key(owner)?;
    h.owner_authority = A::ALL;
    auth::save_authority(txn, page, &mut h)?;
    index::remove(txn, qsys, &old, Kind::Owned, id)?;
    index::put(txn, qsys, owner, Kind::Owned, id, 0)?;
    if private(txn, &h, owner)?.is_some() {
        set_private(txn, qsys, id, owner, None)?;
    }
    if !revoke && objects::exists(txn, qsys, &old) {
        let all = if id.type_code == store::obj::types::AUTL {
            Auth::ALL_AUTL
        } else {
            Auth::ALL
        };
        set_private(txn, qsys, id, &old, Some(all))?;
    }
    Ok(())
}

/// Change the primary group (`CHGOBJPGP`); `None` is `*NONE`. A private
/// authority the new group had becomes its group authority; the old
/// group keeps its authority privately unless `revoke`.
pub fn set_group(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    id: ObjectId,
    group: Option<&str>,
    revoke: bool,
) -> Result<()> {
    let (_, h) = auth::load(txn, id)?;
    let old = text_of(&h.primary_group);
    let old_bits = Auth::of(h.group_authority);
    if group == Some(text_of(&h.owner).as_str()) {
        return Err(SecError::msg(
            "CPF22A6",
            format!("The owner of {} cannot be its primary group.", h.name),
        ));
    }
    if let Some(g) = group
        && !objects::exists(txn, qsys, g)
    {
        return Err(objects::not_found(g));
    }
    let new_bits = match group {
        Some(g) => {
            let had = private(txn, &h, g)?;
            if had.is_some() {
                set_private(txn, qsys, id, g, None)?;
            }
            had.unwrap_or(Auth::EXCLUDE)
        }
        None => Auth::EXCLUDE,
    };
    let (page, mut h) = auth::load(txn, id)?;
    h.primary_group = match group {
        Some(g) => key(g)?,
        None => store::obj::header::blank(),
    };
    h.group_authority = new_bits.store();
    auth::save_authority(txn, page, &mut h)?;
    if !old.is_empty() {
        index::remove(txn, qsys, &old, Kind::Group, id)?;
        if !revoke {
            set_private(txn, qsys, id, &old, Some(old_bits))?;
        }
    }
    if let Some(g) = group {
        index::put(txn, qsys, g, Kind::Group, id, u64::from(new_bits.0))?;
    }
    Ok(())
}

/// Set the primary group's authority.
pub fn set_group_authority(
    txn: &mut Txn<'_>,
    qsys: ObjectId,
    id: ObjectId,
    bits: Auth,
) -> Result<()> {
    let (page, mut h) = auth::load(txn, id)?;
    let g = text_of(&h.primary_group);
    if g.is_empty() {
        return Err(SecError::msg(
            "CPF22A7",
            format!("{} has no primary group.", h.name),
        ));
    }
    h.group_authority = bits.store();
    auth::save_authority(txn, page, &mut h)?;
    index::put(txn, qsys, &g, Kind::Group, id, u64::from(bits.0))
}

/// Secure `id` by authorization list `list`, or by none.
pub fn set_autl(txn: &mut Txn<'_>, qsys: ObjectId, id: ObjectId, list: Option<&str>) -> Result<()> {
    if id.type_code == store::obj::types::AUTL {
        return Err(SecError::msg(
            "CPF2256",
            "An authorization list cannot be secured by one.",
        ));
    }
    let (page, mut h) = auth::load(txn, id)?;
    let old = text_of(&h.autl);
    if let Some(l) = list {
        autl::find(txn, qsys, l)?;
    }
    if !old.is_empty() {
        autl::unsecure(txn, qsys, &old, id)?;
    }
    h.autl = match list {
        Some(l) => key(l)?,
        None => store::obj::header::blank(),
    };
    if list.is_none() {
        h.public_authority = A(h.public_authority.0 & !A::FROM_AUTL);
    }
    auth::save_authority(txn, page, &mut h)?;
    if let Some(l) = list {
        autl::secure(txn, qsys, l, id)?;
    }
    Ok(())
}
