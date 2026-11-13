// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client's half of `fattr4`: the bitmap and the values of the
//! attributes it asks for and sets (RFC 7530 section 5), in bit order.
//! A value of an attribute not in this table cannot be skipped, so a
//! reply with one is refused; the client asks only for these. (The
//! server's full table is `v4/attr`, NFS-6's; this one is the client's
//! own subset, decoded from untrusted bytes with every length checked.)

use ifs::remote::{RAttr, RKind, SetAttr, SetTime};

use crate::types::nfs4::{
    FATTR4_CASE_INSENSITIVE, FATTR4_CASE_PRESERVING, FATTR4_CHANGE, FATTR4_CHOWN_RESTRICTED,
    FATTR4_FILEHANDLE, FATTR4_FILEID, FATTR4_FILES_AVAIL, FATTR4_FILES_FREE, FATTR4_FILES_TOTAL,
    FATTR4_LEASE_TIME, FATTR4_MAXLINK, FATTR4_MAXNAME, FATTR4_MAXREAD, FATTR4_MAXWRITE,
    FATTR4_MODE, FATTR4_NO_TRUNC, FATTR4_NUMLINKS, FATTR4_OWNER, FATTR4_OWNER_GROUP, FATTR4_RAWDEV,
    FATTR4_RDATTR_ERROR, FATTR4_SIZE, FATTR4_SPACE_AVAIL, FATTR4_SPACE_FREE, FATTR4_SPACE_TOTAL,
    FATTR4_SPACE_USED, FATTR4_SUPPORTED_ATTRS, FATTR4_TIME_ACCESS, FATTR4_TIME_ACCESS_SET,
    FATTR4_TIME_METADATA, FATTR4_TIME_MODIFY, FATTR4_TIME_MODIFY_SET, FATTR4_TYPE, NFS4_FHSIZE,
    NFS4_OPAQUE_LIMIT, attrlist4, bitmap4, fattr4,
};
use crate::xdr::{Decoder, Encoder};

/// What an object's attributes are asked with.
pub const OBJECT: &[u32] = &[
    FATTR4_TYPE,
    FATTR4_CHANGE,
    FATTR4_SIZE,
    FATTR4_FILEID,
    FATTR4_MODE,
    FATTR4_NUMLINKS,
    FATTR4_OWNER,
    FATTR4_OWNER_GROUP,
    FATTR4_RAWDEV,
    FATTR4_SPACE_USED,
    FATTR4_TIME_ACCESS,
    FATTR4_TIME_METADATA,
    FATTR4_TIME_MODIFY,
];

/// A directory entry's: the object's, its handle and its error.
pub const ENTRY: &[u32] = &[
    FATTR4_TYPE,
    FATTR4_CHANGE,
    FATTR4_SIZE,
    FATTR4_RDATTR_ERROR,
    FATTR4_FILEHANDLE,
    FATTR4_FILEID,
    FATTR4_MODE,
    FATTR4_NUMLINKS,
    FATTR4_OWNER,
    FATTR4_OWNER_GROUP,
    FATTR4_RAWDEV,
    FATTR4_SPACE_USED,
    FATTR4_TIME_ACCESS,
    FATTR4_TIME_METADATA,
    FATTR4_TIME_MODIFY,
];

/// Capacity.
pub const CAPACITY: &[u32] = &[
    FATTR4_FILES_AVAIL,
    FATTR4_FILES_FREE,
    FATTR4_FILES_TOTAL,
    FATTR4_SPACE_AVAIL,
    FATTR4_SPACE_FREE,
    FATTR4_SPACE_TOTAL,
];

/// Limits, and what the mount settles.
pub const LIMITS: &[u32] = &[
    FATTR4_LEASE_TIME,
    FATTR4_CASE_INSENSITIVE,
    FATTR4_CASE_PRESERVING,
    FATTR4_CHOWN_RESTRICTED,
    FATTR4_MAXLINK,
    FATTR4_MAXNAME,
    FATTR4_MAXREAD,
    FATTR4_MAXWRITE,
    FATTR4_NO_TRUNC,
];

/// The change time alone (a `setattr` guard).
pub const CTIME: &[u32] = &[FATTR4_TIME_METADATA];

/// The most bitmap words read.
const MAX_WORDS: u32 = 8;

/// A bitmap of attribute numbers.
#[must_use]
pub fn bitmap(list: &[u32]) -> bitmap4 {
    let mut w: Vec<u32> = Vec::new();
    for &n in list {
        let i = (n / 32) as usize;
        if w.len() <= i {
            w.resize(i + 1, 0);
        }
        w[i] |= 1 << (n % 32);
    }
    bitmap4(w)
}

/// The attribute numbers of a bitmap, in order.
#[must_use]
pub fn numbers(b: &bitmap4) -> Vec<u32> {
    let mut v = Vec::new();
    for (i, w) in b.0.iter().enumerate().take(MAX_WORDS as usize) {
        for bit in 0..32 {
            if w & (1 << bit) != 0 {
                v.push(u32::try_from(i).unwrap_or(0) * 32 + bit);
            }
        }
    }
    v
}

/// A time as seconds and nanoseconds, nanoseconds since the epoch.
fn ns(secs: i64, nsecs: u32) -> i64 {
    secs.saturating_mul(1_000_000_000)
        .saturating_add(i64::from(nsecs.min(999_999_999)))
}

/// The attributes the client knows, as decoded.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Attrs {
    /// `type` (`nfs_ftype4`'s value).
    pub ftype: Option<u32>,
    /// `change`.
    pub change: Option<u64>,
    /// `size`.
    pub size: Option<u64>,
    /// `lease_time`, seconds.
    pub lease_time: Option<u32>,
    /// `rdattr_error`.
    pub rdattr_error: Option<u32>,
    /// `case_insensitive`.
    pub case_insensitive: Option<bool>,
    /// `case_preserving`.
    pub case_preserving: Option<bool>,
    /// `chown_restricted`.
    pub chown_restricted: Option<bool>,
    /// `filehandle`.
    pub filehandle: Option<Vec<u8>>,
    /// `fileid`.
    pub fileid: Option<u64>,
    /// `files_avail`, `files_free`, `files_total`.
    pub files: [Option<u64>; 3],
    /// `maxlink`.
    pub maxlink: Option<u32>,
    /// `maxname`.
    pub maxname: Option<u32>,
    /// `maxread`.
    pub maxread: Option<u64>,
    /// `maxwrite`.
    pub maxwrite: Option<u64>,
    /// `mode`.
    pub mode: Option<u32>,
    /// `no_trunc`.
    pub no_trunc: Option<bool>,
    /// `numlinks`.
    pub numlinks: Option<u32>,
    /// `owner`.
    pub owner: Option<Vec<u8>>,
    /// `owner_group`.
    pub group: Option<Vec<u8>>,
    /// `rawdev`.
    pub rawdev: Option<(u32, u32)>,
    /// `space_avail`, `space_free`, `space_total`.
    pub space: [Option<u64>; 3],
    /// `space_used`.
    pub space_used: Option<u64>,
    /// `time_access`, ns.
    pub atime: Option<i64>,
    /// `time_metadata`, ns.
    pub ctime: Option<i64>,
    /// `time_modify`, ns.
    pub mtime: Option<i64>,
}

/// Why attributes cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadAttrs;

fn put_time(e: &mut Encoder, ns: i64) {
    e.put_i64(ns.div_euclid(1_000_000_000));
    e.put_u32(u32::try_from(ns.rem_euclid(1_000_000_000)).unwrap_or(0));
}

/// Decode `f`.
///
/// # Errors
///
/// An attribute this client does not know, a value that does not
/// decode, or bytes left over.
pub fn decode(f: &fattr4) -> Result<Attrs, BadAttrs> {
    let mut a = Attrs::default();
    let mut d = Decoder::new(&f.attr_vals.0);
    fn b<T>(r: crate::xdr::Result<T>) -> Result<T, BadAttrs> {
        r.map_err(|_| BadAttrs)
    }
    for n in numbers(&f.attrmask) {
        match n {
            FATTR4_SUPPORTED_ATTRS => {
                let k = b(d.get_u32())?;
                if k > MAX_WORDS {
                    return Err(BadAttrs);
                }
                for _ in 0..k {
                    b(d.get_u32())?;
                }
            }
            FATTR4_TYPE => a.ftype = Some(b(d.get_u32())?),
            FATTR4_CHANGE => a.change = Some(b(d.get_u64())?),
            FATTR4_SIZE => a.size = Some(b(d.get_u64())?),
            FATTR4_LEASE_TIME => a.lease_time = Some(b(d.get_u32())?),
            FATTR4_RDATTR_ERROR => a.rdattr_error = Some(b(d.get_u32())?),
            FATTR4_CASE_INSENSITIVE => a.case_insensitive = Some(b(d.get_bool())?),
            FATTR4_CASE_PRESERVING => a.case_preserving = Some(b(d.get_bool())?),
            FATTR4_CHOWN_RESTRICTED => a.chown_restricted = Some(b(d.get_bool())?),
            FATTR4_FILEHANDLE => a.filehandle = Some(b(d.get_opaque(NFS4_FHSIZE))?),
            FATTR4_FILEID => a.fileid = Some(b(d.get_u64())?),
            FATTR4_FILES_AVAIL => a.files[0] = Some(b(d.get_u64())?),
            FATTR4_FILES_FREE => a.files[1] = Some(b(d.get_u64())?),
            FATTR4_FILES_TOTAL => a.files[2] = Some(b(d.get_u64())?),
            FATTR4_MAXLINK => a.maxlink = Some(b(d.get_u32())?),
            FATTR4_MAXNAME => a.maxname = Some(b(d.get_u32())?),
            FATTR4_MAXREAD => a.maxread = Some(b(d.get_u64())?),
            FATTR4_MAXWRITE => a.maxwrite = Some(b(d.get_u64())?),
            FATTR4_MODE => a.mode = Some(b(d.get_u32())?),
            FATTR4_NO_TRUNC => a.no_trunc = Some(b(d.get_bool())?),
            FATTR4_NUMLINKS => a.numlinks = Some(b(d.get_u32())?),
            FATTR4_OWNER => a.owner = Some(b(d.get_opaque(NFS4_OPAQUE_LIMIT))?),
            FATTR4_OWNER_GROUP => a.group = Some(b(d.get_opaque(NFS4_OPAQUE_LIMIT))?),
            FATTR4_RAWDEV => a.rawdev = Some((b(d.get_u32())?, b(d.get_u32())?)),
            FATTR4_SPACE_AVAIL => a.space[0] = Some(b(d.get_u64())?),
            FATTR4_SPACE_FREE => a.space[1] = Some(b(d.get_u64())?),
            FATTR4_SPACE_TOTAL => a.space[2] = Some(b(d.get_u64())?),
            FATTR4_SPACE_USED => a.space_used = Some(b(d.get_u64())?),
            FATTR4_TIME_ACCESS => a.atime = Some(ns(b(d.get_i64())?, b(d.get_u32())?)),
            FATTR4_TIME_METADATA => a.ctime = Some(ns(b(d.get_i64())?, b(d.get_u32())?)),
            FATTR4_TIME_MODIFY => a.mtime = Some(ns(b(d.get_i64())?, b(d.get_u32())?)),
            _ => return Err(BadAttrs),
        }
    }
    b(d.finish())?;
    Ok(a)
}

/// Encode `a`'s values that are set, as the server writes them (tests).
#[must_use]
pub fn encode(a: &Attrs) -> fattr4 {
    let mut bits = Vec::new();
    let mut e = Encoder::new();
    macro_rules! put {
        ($n:expr, $v:expr, $f:expr) => {
            if let Some(v) = &$v {
                bits.push($n);
                $f(&mut e, v);
            }
        };
    }
    let u32_ = |e: &mut Encoder, v: &u32| e.put_u32(*v);
    let u64_ = |e: &mut Encoder, v: &u64| e.put_u64(*v);
    let bool_ = |e: &mut Encoder, v: &bool| e.put_bool(*v);
    let op = |e: &mut Encoder, v: &Vec<u8>| {
        let _ = e.put_opaque(v, u32::MAX);
    };
    let tm = |e: &mut Encoder, v: &i64| put_time(e, *v);
    put!(FATTR4_TYPE, a.ftype, u32_);
    put!(FATTR4_CHANGE, a.change, u64_);
    put!(FATTR4_SIZE, a.size, u64_);
    put!(FATTR4_LEASE_TIME, a.lease_time, u32_);
    put!(FATTR4_RDATTR_ERROR, a.rdattr_error, u32_);
    put!(FATTR4_CASE_INSENSITIVE, a.case_insensitive, bool_);
    put!(FATTR4_CASE_PRESERVING, a.case_preserving, bool_);
    put!(FATTR4_CHOWN_RESTRICTED, a.chown_restricted, bool_);
    put!(FATTR4_FILEHANDLE, a.filehandle, op);
    put!(FATTR4_FILEID, a.fileid, u64_);
    put!(FATTR4_FILES_AVAIL, a.files[0], u64_);
    put!(FATTR4_FILES_FREE, a.files[1], u64_);
    put!(FATTR4_FILES_TOTAL, a.files[2], u64_);
    put!(FATTR4_MAXLINK, a.maxlink, u32_);
    put!(FATTR4_MAXNAME, a.maxname, u32_);
    put!(FATTR4_MAXREAD, a.maxread, u64_);
    put!(FATTR4_MAXWRITE, a.maxwrite, u64_);
    put!(FATTR4_MODE, a.mode, u32_);
    put!(FATTR4_NO_TRUNC, a.no_trunc, bool_);
    put!(FATTR4_NUMLINKS, a.numlinks, u32_);
    put!(FATTR4_OWNER, a.owner, op);
    put!(FATTR4_OWNER_GROUP, a.group, op);
    if let Some((x, y)) = a.rawdev {
        bits.push(FATTR4_RAWDEV);
        e.put_u32(x);
        e.put_u32(y);
    }
    put!(FATTR4_SPACE_AVAIL, a.space[0], u64_);
    put!(FATTR4_SPACE_FREE, a.space[1], u64_);
    put!(FATTR4_SPACE_TOTAL, a.space[2], u64_);
    put!(FATTR4_SPACE_USED, a.space_used, u64_);
    put!(FATTR4_TIME_ACCESS, a.atime, tm);
    put!(FATTR4_TIME_METADATA, a.ctime, tm);
    put!(FATTR4_TIME_MODIFY, a.mtime, tm);
    fattr4 {
        attrmask: bitmap(&bits),
        attr_vals: attrlist4(e.into_bytes()),
    }
}

/// An owner or group string: a number as text; anything else is the
/// user without an identity (no `user@domain` mapping on the client;
/// docs/NFS.md 7.7).
#[must_use]
pub fn id_of(s: &[u8]) -> u32 {
    std::str::from_utf8(s)
        .ok()
        .and_then(|t| t.parse::<u32>().ok())
        .unwrap_or(ifs::remote::Cred::NOBODY)
}

/// What `setattr` (and a create) sets, as `fattr4`: owners as numeric
/// strings, times as `settime4`.
#[must_use]
pub fn settable(s: &SetAttr) -> fattr4 {
    let mut bits = Vec::new();
    let mut e = Encoder::new();
    if let Some(v) = s.size {
        bits.push(FATTR4_SIZE);
        e.put_u64(v);
    }
    if let Some(v) = s.mode {
        bits.push(FATTR4_MODE);
        e.put_u32(v & 0o7777);
    }
    if let Some(v) = s.uid {
        bits.push(FATTR4_OWNER);
        let _ = e.put_opaque(v.to_string().as_bytes(), NFS4_OPAQUE_LIMIT);
    }
    if let Some(v) = s.gid {
        bits.push(FATTR4_OWNER_GROUP);
        let _ = e.put_opaque(v.to_string().as_bytes(), NFS4_OPAQUE_LIMIT);
    }
    for (n, t) in [
        (FATTR4_TIME_ACCESS_SET, s.atime),
        (FATTR4_TIME_MODIFY_SET, s.mtime),
    ] {
        match t {
            None => {}
            Some(SetTime::Server) => {
                bits.push(n);
                e.put_u32(0);
            }
            Some(SetTime::At(ns)) => {
                bits.push(n);
                e.put_u32(1);
                put_time(&mut e, ns);
            }
        }
    }
    fattr4 {
        attrmask: bitmap(&bits),
        attr_vals: attrlist4(e.into_bytes()),
    }
}

/// The change time as a guard (`VERIFY`).
#[must_use]
pub fn ctime_is(ns: i64) -> fattr4 {
    let mut e = Encoder::new();
    put_time(&mut e, ns);
    fattr4 {
        attrmask: bitmap(CTIME),
        attr_vals: attrlist4(e.into_bytes()),
    }
}

/// What a kind is.
#[must_use]
pub fn kind(t: u32) -> RKind {
    match t {
        2 => RKind::Dir,
        3 => RKind::Blk,
        4 => RKind::Chr,
        5 => RKind::Symlink,
        6 => RKind::Socket,
        7 => RKind::Fifo,
        _ => RKind::File,
    }
}

/// The object's attributes as the IFS's.
#[must_use]
pub fn rattr(a: &Attrs) -> RAttr {
    RAttr {
        kind: kind(a.ftype.unwrap_or(1)),
        mode: a.mode.unwrap_or(0) & 0o7777,
        nlink: a.numlinks.unwrap_or(1),
        uid: a.owner.as_deref().map_or(ifs::remote::Cred::NOBODY, id_of),
        gid: a.group.as_deref().map_or(ifs::remote::Cred::NOBODY, id_of),
        size: a.size.unwrap_or(0),
        used: a.space_used.unwrap_or(0),
        fileid: a.fileid.unwrap_or(0),
        rdev: a
            .rawdev
            .map_or(0, |(x, y)| (u64::from(x) << 32) | u64::from(y)),
        atime: a.atime.unwrap_or(0),
        mtime: a.mtime.unwrap_or(0),
        ctime: a.ctime.unwrap_or(0),
        change: a.change.unwrap_or_else(|| {
            super::super::v3::attr::change_of(
                a.ctime.unwrap_or(0),
                a.mtime.unwrap_or(0),
                a.size.unwrap_or(0),
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let a = Attrs {
            ftype: Some(1),
            change: Some(9),
            size: Some(100),
            filehandle: Some(vec![1, 2, 3]),
            owner: Some(b"100".to_vec()),
            group: Some(b"staff@example".to_vec()),
            mtime: Some(1_700_000_000_000_000_001),
            rawdev: Some((1, 2)),
            ..Attrs::default()
        };
        let f = encode(&a);
        assert_eq!(decode(&f).unwrap(), a);
        let r = rattr(&a);
        assert_eq!(
            (r.uid, r.gid, r.size),
            (100, ifs::remote::Cred::NOBODY, 100)
        );
        assert_eq!(numbers(&bitmap(&[1, 33, 55])), vec![1, 33, 55]);
    }

    #[test]
    fn unknown_or_short_is_refused() {
        let f = fattr4 {
            attrmask: bitmap(&[FATTR4_SIZE]),
            attr_vals: attrlist4(vec![0, 0, 0]),
        };
        assert_eq!(decode(&f), Err(BadAttrs));
        let f = fattr4 {
            attrmask: bitmap(&[12]),
            attr_vals: attrlist4(vec![0; 4]),
        };
        assert_eq!(decode(&f), Err(BadAttrs));
    }
}
