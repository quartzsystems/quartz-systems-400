// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Encoding attribute values (`GETATTR`, `READDIR`, `VERIFY`): those
//! asked that are supported, in bit order; the rest are left out. What
//! needs another call (space, limits, the creation time) is asked of
//! [`Extra`] only when wanted.

use super::{Bits, WRITE_ONLY, supported};
use crate::registry::Registry;
use crate::server::vfs::{Attr, FileType, FsStat, PathConf, Time};
use crate::types::nfs4::{
    self as t, FH4_PERSISTENT, attrlist4, fattr4, fsid4, nfs_ftype4, nfsstat4, nfstime4,
    specdata4,
};
use crate::xdr::{Encoder, Xdr};

/// An object as the attributes see it.
#[derive(Debug, Clone)]
pub struct View<'a> {
    /// Its attributes.
    pub attr: &'a Attr,
    /// Its handle's bytes.
    pub fh: &'a [u8],
    /// `fsid`.
    pub fsid: (u64, u64),
    /// `mounted_on_fileid`.
    pub mounted_on: u64,
}

/// What some attributes need beyond [`Attr`].
pub trait Extra {
    /// Space and objects.
    ///
    /// # Errors
    ///
    /// The status to answer.
    fn fsstat(&mut self) -> Result<FsStat, nfsstat4>;
    /// Names and links.
    ///
    /// # Errors
    ///
    /// The status to answer.
    fn pathconf(&mut self) -> Result<PathConf, nfsstat4>;
    /// When it was made.
    ///
    /// # Errors
    ///
    /// The status to answer.
    fn created(&mut self) -> Result<Time, nfsstat4>;
}

/// What every object's attributes share.
#[derive(Debug, Clone, Copy)]
pub struct Env<'a> {
    /// The compound's minor version.
    pub minor: u32,
    /// The lease, in seconds.
    pub lease: u32,
    /// The largest `READ` and `WRITE`.
    pub max_io: u64,
    /// Owners and groups.
    pub reg: &'a Registry,
}

/// `nfs_ftype4`.
#[must_use]
pub fn ftype(f: FileType) -> nfs_ftype4 {
    match f {
        FileType::Reg => nfs_ftype4::NF4REG,
        FileType::Dir => nfs_ftype4::NF4DIR,
        FileType::Blk => nfs_ftype4::NF4BLK,
        FileType::Chr => nfs_ftype4::NF4CHR,
        FileType::Lnk => nfs_ftype4::NF4LNK,
        FileType::Sock => nfs_ftype4::NF4SOCK,
        FileType::Fifo => nfs_ftype4::NF4FIFO,
    }
}

/// `nfstime4`.
#[must_use]
pub fn time4(t: Time) -> nfstime4 {
    nfstime4 {
        seconds: t.secs,
        nseconds: t.nsecs,
    }
}

/// Whether `want` asks for a write-only attribute (`NFS4ERR_INVAL`).
#[must_use]
pub fn asks_write_only(want: &Bits) -> bool {
    WRITE_ONLY.iter().any(|n| want.has(*n))
}

fn put<T: Xdr>(e: &mut Encoder, v: &T) -> Result<(), nfsstat4> {
    v.encode(e).map_err(|_| nfsstat4::NFS4ERR_SERVERFAULT)
}

/// The values of `want` for `v`.
///
/// # Errors
///
/// `NFS4ERR_INVAL` for a write-only attribute, and what [`Extra`] answers.
pub fn encode(
    want: &Bits,
    v: &View<'_>,
    x: &mut dyn Extra,
    env: &Env<'_>,
) -> Result<fattr4, nfsstat4> {
    if asks_write_only(want) {
        return Err(nfsstat4::NFS4ERR_INVAL);
    }
    let sup = supported(env.minor);
    let mut got = Bits::default();
    let mut e = Encoder::new();
    let a = v.attr;
    let mut fs: Option<FsStat> = None;
    let mut pc: Option<PathConf> = None;
    for n in want.and(&sup).iter() {
        match n {
            t::FATTR4_SUPPORTED_ATTRS => put(&mut e, &sup.to_bitmap())?,
            t::FATTR4_TYPE => put(&mut e, &ftype(a.ftype))?,
            t::FATTR4_FH_EXPIRE_TYPE => e.put_u32(FH4_PERSISTENT),
            t::FATTR4_CHANGE => e.put_u64(a.change),
            t::FATTR4_SIZE => e.put_u64(a.size),
            t::FATTR4_LINK_SUPPORT
            | t::FATTR4_SYMLINK_SUPPORT
            | t::FATTR4_UNIQUE_HANDLES
            | t::FATTR4_CANSETTIME
            | t::FATTR4_CHOWN_RESTRICTED
            | t::FATTR4_HOMOGENEOUS
            | t::FATTR4_NO_TRUNC => e.put_bool(true),
            t::FATTR4_NAMED_ATTR => e.put_bool(false),
            t::FATTR4_FSID => put(
                &mut e,
                &fsid4 {
                    major: v.fsid.0,
                    minor: v.fsid.1,
                },
            )?,
            t::FATTR4_LEASE_TIME => e.put_u32(env.lease),
            t::FATTR4_RDATTR_ERROR => put(&mut e, &nfsstat4::NFS4_OK)?,
            t::FATTR4_ACLSUPPORT => e.put_u32(0),
            t::FATTR4_CASE_INSENSITIVE | t::FATTR4_CASE_PRESERVING | t::FATTR4_MAXLINK => {
                let p = match pc {
                    Some(p) => p,
                    None => *pc.insert(x.pathconf()?),
                };
                match n {
                    t::FATTR4_CASE_INSENSITIVE => e.put_bool(p.case_insensitive),
                    t::FATTR4_CASE_PRESERVING => e.put_bool(p.case_preserving),
                    _ => e.put_u32(p.linkmax),
                }
            }
            t::FATTR4_FILEHANDLE => e
                .put_opaque(v.fh, t::NFS4_FHSIZE)
                .map_err(|_| nfsstat4::NFS4ERR_SERVERFAULT)?,
            t::FATTR4_FILEID => e.put_u64(a.fileid),
            t::FATTR4_FILES_AVAIL
            | t::FATTR4_FILES_FREE
            | t::FATTR4_FILES_TOTAL
            | t::FATTR4_SPACE_AVAIL
            | t::FATTR4_SPACE_FREE
            | t::FATTR4_SPACE_TOTAL => {
                let s = match fs {
                    Some(s) => s,
                    None => *fs.insert(x.fsstat()?),
                };
                e.put_u64(match n {
                    t::FATTR4_FILES_AVAIL => s.afiles,
                    t::FATTR4_FILES_FREE => s.ffiles,
                    t::FATTR4_FILES_TOTAL => s.tfiles,
                    t::FATTR4_SPACE_AVAIL => s.abytes,
                    t::FATTR4_SPACE_FREE => s.fbytes,
                    _ => s.tbytes,
                });
            }
            t::FATTR4_MAXFILESIZE => e.put_u64(i64::MAX.cast_unsigned()),
            t::FATTR4_MAXNAME => e.put_u32(255),
            t::FATTR4_MAXREAD | t::FATTR4_MAXWRITE => e.put_u64(env.max_io),
            t::FATTR4_MODE => e.put_u32(a.mode & 0o7777),
            t::FATTR4_NUMLINKS => e.put_u32(a.nlink),
            t::FATTR4_OWNER => {
                let s = env.reg.owner(a.uid, Some(a.owner.as_str()));
                e.put_opaque(s.as_bytes(), u32::MAX)
                    .map_err(|_| nfsstat4::NFS4ERR_SERVERFAULT)?;
            }
            t::FATTR4_OWNER_GROUP => {
                let s = env.reg.group(a.gid, a.group.as_deref());
                e.put_opaque(s.as_bytes(), u32::MAX)
                    .map_err(|_| nfsstat4::NFS4ERR_SERVERFAULT)?;
            }
            t::FATTR4_RAWDEV => put(
                &mut e,
                &specdata4 {
                    specdata1: a.rdev.0,
                    specdata2: a.rdev.1,
                },
            )?,
            t::FATTR4_SPACE_USED => e.put_u64(a.used),
            t::FATTR4_TIME_ACCESS => put(&mut e, &time4(a.atime))?,
            t::FATTR4_TIME_CREATE => put(&mut e, &time4(x.created()?))?,
            t::FATTR4_TIME_DELTA => put(
                &mut e,
                &nfstime4 {
                    seconds: 0,
                    nseconds: 1,
                },
            )?,
            t::FATTR4_TIME_METADATA => put(&mut e, &time4(a.ctime))?,
            t::FATTR4_TIME_MODIFY => put(&mut e, &time4(a.mtime))?,
            t::FATTR4_MOUNTED_ON_FILEID => e.put_u64(v.mounted_on),
            t::FATTR4_SUPPATTR_EXCLCREAT => put(&mut e, &Bits::of(super::EXCLCREAT).to_bitmap())?,
            _ => continue,
        }
        got.set(n);
    }
    Ok(fattr4 {
        attrmask: got.to_bitmap(),
        attr_vals: attrlist4(e.into_bytes()),
    })
}

/// Just `rdattr_error` (a `READDIR` entry whose attributes failed).
#[must_use]
pub fn rdattr_error(s: nfsstat4) -> fattr4 {
    let mut e = Encoder::new();
    let _ = s.encode(&mut e);
    fattr4 {
        attrmask: Bits::of(&[t::FATTR4_RDATTR_ERROR]).to_bitmap(),
        attr_vals: attrlist4(e.into_bytes()),
    }
}
