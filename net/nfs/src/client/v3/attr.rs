// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Between NFS version 3's attributes and the IFS's remote ones.

use ifs::remote::{RAttr, RKind, SetAttr, SetTime};

use crate::types::nfs3::{fattr3, ftype3, nfstime3, sattr3, set_atime, set_mtime, time_how};

/// Nanoseconds since the epoch of a wire time.
#[must_use]
pub fn nanos(t: &nfstime3) -> i64 {
    i64::from(t.seconds) * 1_000_000_000 + i64::from(t.nseconds.min(999_999_999))
}

/// A wire time of nanoseconds since the epoch (clamped to 32-bit seconds).
#[must_use]
pub fn time3(ns: i64) -> nfstime3 {
    let secs = ns.div_euclid(1_000_000_000);
    nfstime3 {
        seconds: u32::try_from(secs.clamp(0, i64::from(u32::MAX))).unwrap_or(0),
        nseconds: u32::try_from(ns.rem_euclid(1_000_000_000)).unwrap_or(0),
    }
}

/// What a kind is on the wire.
#[must_use]
pub fn kind(t: ftype3) -> RKind {
    match t {
        ftype3::NF3REG => RKind::File,
        ftype3::NF3DIR => RKind::Dir,
        ftype3::NF3BLK => RKind::Blk,
        ftype3::NF3CHR => RKind::Chr,
        ftype3::NF3LNK => RKind::Symlink,
        ftype3::NF3SOCK => RKind::Socket,
        ftype3::NF3FIFO => RKind::Fifo,
    }
}

/// Version 3 has no change attribute: the change time, the
/// modification time and the size folded together (FNV-1a).
#[must_use]
pub fn change_of(ctime: i64, mtime: i64, size: u64) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in ctime
        .to_be_bytes()
        .into_iter()
        .chain(mtime.to_be_bytes())
        .chain(size.to_be_bytes())
    {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// `fattr3` as [`RAttr`].
#[must_use]
pub fn rattr(a: &fattr3) -> RAttr {
    let (atime, mtime, ctime) = (nanos(&a.atime), nanos(&a.mtime), nanos(&a.ctime));
    RAttr {
        kind: kind(a.r#type),
        mode: a.mode & 0o7777,
        nlink: a.nlink,
        uid: a.uid,
        gid: a.gid,
        size: a.size,
        used: a.used,
        fileid: a.fileid,
        rdev: (u64::from(a.rdev.specdata1) << 32) | u64::from(a.rdev.specdata2),
        atime,
        mtime,
        ctime,
        change: change_of(ctime, mtime, a.size),
    }
}

/// [`SetAttr`] as `sattr3`.
#[must_use]
pub fn sattr(s: &SetAttr) -> sattr3 {
    let at = |t: Option<SetTime>| match t {
        None => (None, time_how::DONT_CHANGE),
        Some(SetTime::Server) => (None, time_how::SET_TO_SERVER_TIME),
        Some(SetTime::At(ns)) => (Some(time3(ns)), time_how::SET_TO_CLIENT_TIME),
    };
    let atime = match at(s.atime) {
        (Some(t), _) => set_atime::SET_TO_CLIENT_TIME(t),
        (None, h) => set_atime::default(h),
    };
    let mtime = match at(s.mtime) {
        (Some(t), _) => set_mtime::SET_TO_CLIENT_TIME(t),
        (None, h) => set_mtime::default(h),
    };
    sattr3 {
        mode: s.mode.map(|m| m & 0o7777),
        uid: s.uid,
        gid: s.gid,
        size: s.size,
        atime,
        mtime,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_round_trip() {
        let ns = 1_700_000_000_123_456_789;
        assert_eq!(nanos(&time3(ns)), ns);
        assert_eq!(time3(-5).seconds, 0);
    }

    #[test]
    fn change_moves() {
        assert_ne!(change_of(1, 2, 3), change_of(1, 2, 4));
        assert_ne!(change_of(1, 2, 3), change_of(2, 2, 3));
    }
}
