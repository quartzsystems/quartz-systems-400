// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NFSv4` attributes (RFC 7530 section 5, RFC 8881 section 5): the
//! `bitmap4` sets ([`Bits`]) and the table of what this server supports.
//! `fattr4`'s values are opaque in XDR: [`get`] encodes them in bit
//! order, [`set`] decodes those a client sends.
//!
//! Supported: every REQUIRED attribute and the RECOMMENDED ones the IFS
//! answers truthfully. Not supported (asked in `GETATTR`: left out; in
//! `SETATTR`, `CREATE`, `OPEN`, `VERIFY`: `NFS4ERR_ATTRNOTSUPP`): `acl`,
//! `dacl`, `sacl`, `archive`, `hidden`, `system`, `mimetype`, quotas,
//! `time_backup`, `fs_locations` and `fs_locations_info` (no
//! referrals), the layout attributes (no pNFS), retention, the
//! notification delays, `change_policy`, `fs_status`, `mode_set_masked`
//! and `fs_charset_cap`.

pub mod get;
pub mod set;

use crate::types::nfs4::{self as t, bitmap4};

/// A set of attribute numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bits(pub Vec<u32>);

impl Bits {
    /// The set of `numbers`.
    #[must_use]
    pub fn of(numbers: &[u32]) -> Self {
        let mut b = Self::default();
        for n in numbers {
            b.set(*n);
        }
        b
    }

    /// Whether `n` is in it.
    #[must_use]
    pub fn has(&self, n: u32) -> bool {
        let w = (n / 32) as usize;
        self.0.get(w).is_some_and(|x| x & (1 << (n % 32)) != 0)
    }

    /// Puts `n` in.
    pub fn set(&mut self, n: u32) {
        let w = (n / 32) as usize;
        if self.0.len() <= w {
            self.0.resize(w + 1, 0);
        }
        self.0[w] |= 1 << (n % 32);
    }

    /// The numbers, lowest first.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.0.iter().enumerate().flat_map(|(w, x)| {
            (0..32u32)
                .filter(move |b| x & (1 << b) != 0)
                .map(move |b| u32::try_from(w).unwrap_or(0) * 32 + b)
        })
    }

    /// Whether it is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|x| *x == 0)
    }

    /// Those of `self` also in `other`.
    #[must_use]
    pub fn and(&self, other: &Self) -> Self {
        Self(
            self.0
                .iter()
                .zip(other.0.iter().chain(std::iter::repeat(&0)))
                .map(|(a, b)| a & b)
                .collect(),
        )
    }

    /// Those of `self` not in `other`.
    #[must_use]
    pub fn minus(&self, other: &Self) -> Self {
        Self(
            self.0
                .iter()
                .zip(other.0.iter().chain(std::iter::repeat(&0)))
                .map(|(a, b)| a & !b)
                .collect(),
        )
    }

    /// As a `bitmap4`, without trailing empty words.
    #[must_use]
    pub fn to_bitmap(&self) -> bitmap4 {
        let mut v = self.0.clone();
        while v.last() == Some(&0) {
            v.pop();
        }
        bitmap4(v)
    }
}

impl From<&bitmap4> for Bits {
    fn from(b: &bitmap4) -> Self {
        Self(b.0.clone())
    }
}

/// The attributes this server answers, 4.0's.
pub const SUPPORTED: &[u32] = &[
    t::FATTR4_SUPPORTED_ATTRS,
    t::FATTR4_TYPE,
    t::FATTR4_FH_EXPIRE_TYPE,
    t::FATTR4_CHANGE,
    t::FATTR4_SIZE,
    t::FATTR4_LINK_SUPPORT,
    t::FATTR4_SYMLINK_SUPPORT,
    t::FATTR4_NAMED_ATTR,
    t::FATTR4_FSID,
    t::FATTR4_UNIQUE_HANDLES,
    t::FATTR4_LEASE_TIME,
    t::FATTR4_RDATTR_ERROR,
    t::FATTR4_ACLSUPPORT,
    t::FATTR4_CANSETTIME,
    t::FATTR4_CASE_INSENSITIVE,
    t::FATTR4_CASE_PRESERVING,
    t::FATTR4_CHOWN_RESTRICTED,
    t::FATTR4_FILEHANDLE,
    t::FATTR4_FILEID,
    t::FATTR4_FILES_AVAIL,
    t::FATTR4_FILES_FREE,
    t::FATTR4_FILES_TOTAL,
    t::FATTR4_HOMOGENEOUS,
    t::FATTR4_MAXFILESIZE,
    t::FATTR4_MAXLINK,
    t::FATTR4_MAXNAME,
    t::FATTR4_MAXREAD,
    t::FATTR4_MAXWRITE,
    t::FATTR4_MODE,
    t::FATTR4_NO_TRUNC,
    t::FATTR4_NUMLINKS,
    t::FATTR4_OWNER,
    t::FATTR4_OWNER_GROUP,
    t::FATTR4_RAWDEV,
    t::FATTR4_SPACE_AVAIL,
    t::FATTR4_SPACE_FREE,
    t::FATTR4_SPACE_TOTAL,
    t::FATTR4_SPACE_USED,
    t::FATTR4_TIME_ACCESS,
    t::FATTR4_TIME_ACCESS_SET,
    t::FATTR4_TIME_CREATE,
    t::FATTR4_TIME_DELTA,
    t::FATTR4_TIME_METADATA,
    t::FATTR4_TIME_MODIFY,
    t::FATTR4_TIME_MODIFY_SET,
    t::FATTR4_MOUNTED_ON_FILEID,
];

/// What a client may set.
pub const WRITABLE: &[u32] = &[
    t::FATTR4_SIZE,
    t::FATTR4_MODE,
    t::FATTR4_OWNER,
    t::FATTR4_OWNER_GROUP,
    t::FATTR4_TIME_ACCESS_SET,
    t::FATTR4_TIME_MODIFY_SET,
];

/// What a client may only set (`GETATTR` of them is `NFS4ERR_INVAL`).
pub const WRITE_ONLY: &[u32] = &[t::FATTR4_TIME_ACCESS_SET, t::FATTR4_TIME_MODIFY_SET];

/// What `EXCLUSIVE4_1` takes beside the verifier (`suppattr_exclcreat`):
/// the times hold the verifier.
pub const EXCLCREAT: &[u32] = &[
    t::FATTR4_SIZE,
    t::FATTR4_MODE,
    t::FATTR4_OWNER,
    t::FATTR4_OWNER_GROUP,
];

/// The supported set at minor version `minor`.
#[must_use]
pub fn supported(minor: u32) -> Bits {
    let mut b = Bits::of(SUPPORTED);
    if minor >= 1 {
        b.set(t::FATTR4_SUPPATTR_EXCLCREAT);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_sets() {
        let b = Bits::of(&[1, 33, 75]);
        assert!(b.has(33) && b.has(75) && !b.has(2));
        assert_eq!(b.iter().collect::<Vec<_>>(), vec![1, 33, 75]);
        assert_eq!(b.to_bitmap().0.len(), 3);
        let c = Bits::of(&[1, 2]);
        assert_eq!(b.and(&c), Bits(vec![2, 0, 0]));
        assert_eq!(b.minus(&c).iter().collect::<Vec<_>>(), vec![33, 75]);
        assert_eq!(Bits(vec![2, 0, 0]).to_bitmap().0, vec![2]);
        assert!(supported(1).has(75) && !supported(0).has(75));
        assert!(!supported(1).has(t::FATTR4_ACL));
    }
}
