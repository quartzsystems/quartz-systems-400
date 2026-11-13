// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Between the [`crate::server::Vfs`]'s types and NFS version 3's.

use crate::server::vfs::{Attr, FileType, Node, SetAttr, SetTime, Time, Wcc, WccAttr};
use crate::server::{FileHandle, VfsError};
use crate::types::nfs3::{
    fattr3, ftype3, nfs_fh3, nfsstat3, nfstime3, post_op_attr, post_op_fh3, pre_op_attr, sattr3,
    set_atime, set_mtime, specdata3, time_how, wcc_attr, wcc_data,
};

/// The status of an error.
#[must_use]
pub fn status(e: VfsError) -> nfsstat3 {
    use nfsstat3 as S;
    match e {
        VfsError::Perm => S::NFS3ERR_PERM,
        VfsError::NoEnt => S::NFS3ERR_NOENT,
        VfsError::Nxio => S::NFS3ERR_NXIO,
        VfsError::Acces | VfsError::Busy | VfsError::Again => S::NFS3ERR_ACCES,
        VfsError::Exist => S::NFS3ERR_EXIST,
        VfsError::XDev => S::NFS3ERR_XDEV,
        VfsError::NotDir => S::NFS3ERR_NOTDIR,
        VfsError::IsDir => S::NFS3ERR_ISDIR,
        VfsError::Inval => S::NFS3ERR_INVAL,
        VfsError::FBig => S::NFS3ERR_FBIG,
        VfsError::NoSpc => S::NFS3ERR_NOSPC,
        VfsError::RoFs => S::NFS3ERR_ROFS,
        VfsError::MLink => S::NFS3ERR_MLINK,
        VfsError::NameTooLong => S::NFS3ERR_NAMETOOLONG,
        VfsError::NotEmpty => S::NFS3ERR_NOTEMPTY,
        VfsError::Stale => S::NFS3ERR_STALE,
        VfsError::BadHandle => S::NFS3ERR_BADHANDLE,
        VfsError::NotSync => S::NFS3ERR_NOT_SYNC,
        VfsError::BadCookie => S::NFS3ERR_BAD_COOKIE,
        VfsError::NotSupp => S::NFS3ERR_NOTSUPP,
        VfsError::TooSmall => S::NFS3ERR_TOOSMALL,
        VfsError::ServerFault => S::NFS3ERR_SERVERFAULT,
        VfsError::BadType => S::NFS3ERR_BADTYPE,
        VfsError::Io | VfsError::Deadlock | VfsError::NoLocks | VfsError::Loop => S::NFS3ERR_IO,
    }
}

/// A time on the wire: seconds as 32 bits.
#[must_use]
pub fn time3(t: Time) -> nfstime3 {
    nfstime3 {
        seconds: u32::try_from(t.secs.clamp(0, i64::from(u32::MAX))).unwrap_or(0),
        nseconds: t.nsecs,
    }
}

/// A wire time.
#[must_use]
pub fn time_of(t: &nfstime3) -> Time {
    Time {
        secs: i64::from(t.seconds),
        nsecs: t.nseconds.min(999_999_999),
    }
}

/// `fattr3`.
#[must_use]
pub fn fattr(a: &Attr) -> fattr3 {
    fattr3 {
        r#type: match a.ftype {
            FileType::Reg => ftype3::NF3REG,
            FileType::Dir => ftype3::NF3DIR,
            FileType::Blk => ftype3::NF3BLK,
            FileType::Chr => ftype3::NF3CHR,
            FileType::Lnk => ftype3::NF3LNK,
            FileType::Sock => ftype3::NF3SOCK,
            FileType::Fifo => ftype3::NF3FIFO,
        },
        mode: a.mode,
        nlink: a.nlink,
        uid: a.uid,
        gid: a.gid,
        size: a.size,
        used: a.used,
        rdev: specdata3 {
            specdata1: a.rdev.0,
            specdata2: a.rdev.1,
        },
        fsid: a.fsid,
        fileid: a.fileid,
        atime: time3(a.atime),
        mtime: time3(a.mtime),
        ctime: time3(a.ctime),
    }
}

/// `post_op_attr`.
#[must_use]
pub fn post(a: Option<&Attr>) -> post_op_attr {
    a.map(fattr)
}

fn pre(w: Option<&WccAttr>) -> pre_op_attr {
    w.map(|w| wcc_attr {
        size: w.size,
        mtime: time3(w.mtime),
        ctime: time3(w.ctime),
    })
}

/// `wcc_data`.
#[must_use]
pub fn wcc(w: &Wcc) -> wcc_data {
    wcc_data {
        before: pre(w.before.as_ref()),
        after: post(w.after.as_ref()),
    }
}

/// A `wcc_data` with nothing known.
#[must_use]
pub fn no_wcc() -> wcc_data {
    wcc_data {
        before: None,
        after: None,
    }
}

/// `nfs_fh3`.
#[must_use]
pub fn fh3(h: &FileHandle) -> nfs_fh3 {
    nfs_fh3 {
        data: h.to_bytes().to_vec(),
    }
}

/// `post_op_fh3` and `post_op_attr` of a node.
#[must_use]
pub fn node(n: Option<&Node>) -> (post_op_fh3, post_op_attr) {
    (n.map(|n| fh3(&n.fh)), n.map(|n| fattr(&n.attr)))
}

/// `sattr3` as [`SetAttr`].
#[must_use]
pub fn sattr(s: &sattr3) -> SetAttr {
    let when = |c: Option<&nfstime3>, h: time_how| match (c, h) {
        (Some(t), _) => SetTime::Client(time_of(t)),
        (None, time_how::SET_TO_SERVER_TIME) => SetTime::Server,
        (None, _) => SetTime::Keep,
    };
    let atime = match &s.atime {
        set_atime::SET_TO_CLIENT_TIME(t) => when(Some(t), time_how::SET_TO_CLIENT_TIME),
        set_atime::default(h) => when(None, *h),
    };
    let mtime = match &s.mtime {
        set_mtime::SET_TO_CLIENT_TIME(t) => when(Some(t), time_how::SET_TO_CLIENT_TIME),
        set_mtime::default(h) => when(None, *h),
    };
    SetAttr {
        mode: s.mode,
        uid: s.uid,
        gid: s.gid,
        size: s.size,
        atime,
        mtime,
    }
}
