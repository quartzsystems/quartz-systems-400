// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An object's private authorities: sixteen inline slots in the header,
//! then a B+tree keyed by profile name whose root is the header's
//! `private_overflow`. Every change bumps the object's authority
//! generation so cached decisions go stale. See `docs/STORE_FORMAT.md`.

use crate::Result;
use crate::btree::{BTree, FixedKey};
use crate::obj::ObjectId;
use crate::obj::header::{Authority, ObjectHeader, PRIVATE_SLOTS, PrivateAuth};
use crate::obj::ops::{read_header, write_header};
use crate::page::pager::PageNo;
use crate::txn::Txn;

/// A profile name, CP037, blank padded: the overflow tree's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProfileKey(pub [u8; 10]);

impl FixedKey for ProfileKey {
    const LEN: usize = 10;
    fn write(&self, out: &mut [u8]) {
        out[..10].copy_from_slice(&self.0);
    }
    fn read(bytes: &[u8]) -> Self {
        Self(bytes[..10].try_into().expect("10 bytes"))
    }
}

type Overflow = BTree<ProfileKey, u64>;

fn overflow(h: &ObjectHeader) -> Overflow {
    BTree::open(h.private_overflow, h.id)
}

/// The private authority `profile` has, if any.
pub fn private_get(
    txn: &mut Txn<'_>,
    h: &ObjectHeader,
    profile: [u8; 10],
) -> Result<Option<Authority>> {
    if let Some(p) = h.private_auths.iter().find(|p| p.profile == profile) {
        return Ok(Some(p.bits));
    }
    if h.private_overflow.is_none() {
        return Ok(None);
    }
    let bits = overflow(h).get(txn, &ProfileKey(profile))?;
    Ok(bits.map(|b| Authority(u32::try_from(b).unwrap_or(0))))
}

/// Every private authority, inline first, then the overflow in key order.
pub fn private_list(txn: &mut Txn<'_>, h: &ObjectHeader) -> Result<Vec<PrivateAuth>> {
    let mut v = h.private_auths.clone();
    if h.private_overflow.is_some() {
        for e in overflow(h).entries(txn)? {
            let (k, b) = e?;
            v.push(PrivateAuth {
                profile: k.0,
                bits: Authority(u32::try_from(b).unwrap_or(0)),
            });
        }
    }
    Ok(v)
}

/// Set (`Some`) or remove (`None`) `profile`'s private authority on the
/// object whose header is at `page`; the header is rewritten with a new
/// authority generation. The previous value comes back.
pub fn private_set(
    txn: &mut Txn<'_>,
    page: PageNo,
    h: &mut ObjectHeader,
    profile: [u8; 10],
    bits: Option<Authority>,
) -> Result<Option<Authority>> {
    let key = ProfileKey(profile);
    let mut tree = overflow(h);
    let inline = h.private_auths.iter().position(|p| p.profile == profile);
    let old = match inline {
        Some(i) => Some(h.private_auths[i].bits),
        None if h.private_overflow.is_some() => tree
            .get(txn, &key)?
            .map(|b| Authority(u32::try_from(b).unwrap_or(0))),
        None => None,
    };
    match (bits, inline) {
        (Some(b), Some(i)) => h.private_auths[i].bits = b,
        (Some(b), None) if old.is_some() => {
            tree.insert(txn, key, &u64::from(b.0))?;
        }
        (Some(b), None) if h.private_auths.len() < PRIVATE_SLOTS => {
            h.private_auths.push(PrivateAuth { profile, bits: b });
        }
        (Some(b), None) => {
            tree.insert(txn, key, &u64::from(b.0))?;
        }
        (None, Some(i)) => {
            h.private_auths.remove(i);
            // Pull one entry up from the overflow to keep the slots full.
            if let Some(first) = tree.entries(txn)?.next() {
                let (k, b) = first?;
                tree.delete(txn, &k)?;
                h.private_auths.push(PrivateAuth {
                    profile: k.0,
                    bits: Authority(u32::try_from(b).unwrap_or(0)),
                });
            }
        }
        (None, None) => {
            if old.is_some() {
                tree.delete(txn, &key)?;
            }
        }
    }
    h.private_overflow = tree.root();
    h.auth_gen += 1;
    write_header(txn, page, h)?;
    Ok(old)
}

/// Rewrite a header after an authority change (public, owner, group,
/// authorization list) with a new authority generation.
pub fn save_authority(txn: &mut Txn<'_>, page: PageNo, h: &mut ObjectHeader) -> Result<()> {
    h.auth_gen += 1;
    write_header(txn, page, h)
}

/// The header of `id` and its page, for an authority change.
pub fn load(txn: &mut Txn<'_>, id: ObjectId) -> Result<(PageNo, ObjectHeader)> {
    read_header(txn, id)
}

/// Every page of the overflow tree.
pub fn overflow_pages(txn: &mut Txn<'_>, h: &ObjectHeader) -> Result<Vec<PageNo>> {
    match h.private_overflow {
        Some(root) => crate::btree::walk_pages(txn, root),
        None => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::obj::header::text;

    #[test]
    fn key_round_trip() {
        let k = ProfileKey(text("BOB").unwrap());
        let mut b = [0u8; 10];
        k.write(&mut b);
        assert_eq!(ProfileKey::read(&b), k);
    }
}
