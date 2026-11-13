// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Between the [`crate::server::Vfs`]'s errors and names and `NFSv4`'s.

use crate::server::VfsError;
use crate::server::vfs::{SetTime, Time};
use crate::types::nfs4::{nfsstat4, nfstime4};

/// The status of a file system error.
#[must_use]
pub fn status(e: VfsError) -> nfsstat4 {
    use nfsstat4 as S;
    match e {
        VfsError::Perm => S::NFS4ERR_PERM,
        VfsError::NoEnt => S::NFS4ERR_NOENT,
        VfsError::Io | VfsError::Loop => S::NFS4ERR_IO,
        VfsError::Nxio => S::NFS4ERR_NXIO,
        VfsError::Acces => S::NFS4ERR_ACCESS,
        VfsError::Exist => S::NFS4ERR_EXIST,
        VfsError::XDev => S::NFS4ERR_XDEV,
        VfsError::NotDir => S::NFS4ERR_NOTDIR,
        VfsError::IsDir => S::NFS4ERR_ISDIR,
        VfsError::Inval => S::NFS4ERR_INVAL,
        VfsError::FBig => S::NFS4ERR_FBIG,
        VfsError::NoSpc => S::NFS4ERR_NOSPC,
        VfsError::RoFs => S::NFS4ERR_ROFS,
        VfsError::MLink => S::NFS4ERR_MLINK,
        VfsError::NameTooLong => S::NFS4ERR_NAMETOOLONG,
        VfsError::NotEmpty => S::NFS4ERR_NOTEMPTY,
        VfsError::Stale => S::NFS4ERR_STALE,
        VfsError::BadHandle => S::NFS4ERR_BADHANDLE,
        VfsError::NotSync => S::NFS4ERR_NOT_SAME,
        VfsError::BadCookie => S::NFS4ERR_BAD_COOKIE,
        VfsError::NotSupp => S::NFS4ERR_NOTSUPP,
        VfsError::TooSmall => S::NFS4ERR_TOOSMALL,
        VfsError::ServerFault => S::NFS4ERR_SERVERFAULT,
        VfsError::BadType => S::NFS4ERR_BADTYPE,
        VfsError::Busy => S::NFS4ERR_SHARE_DENIED,
        VfsError::Again => S::NFS4ERR_LOCKED,
        VfsError::Deadlock => S::NFS4ERR_DEADLOCK,
        VfsError::NoLocks => S::NFS4ERR_DELAY,
    }
}

/// The status of an I/O error: a share reservation in the way of a
/// `READ` or `WRITE` is `NFS4ERR_LOCKED`.
#[must_use]
pub fn io_status(e: VfsError) -> nfsstat4 {
    match e {
        VfsError::Busy | VfsError::Again => nfsstat4::NFS4ERR_LOCKED,
        e => status(e),
    }
}

/// Checks a component name (RFC 7530 section 12.7, RFC 8881 section
/// 14.5): not empty, not `.` or `..`, no `/` or NUL, at most 255 bytes;
/// UTF-8 when the host's path names are.
///
/// # Errors
///
/// `NFS4ERR_INVAL`, `NFS4ERR_BADNAME`, `NFS4ERR_NAMETOOLONG`,
/// `NFS4ERR_BADCHAR` (4.0) for bytes that are not UTF-8.
pub fn name(b: &[u8], utf8: bool, minor: u32) -> Result<(), nfsstat4> {
    if b.is_empty() {
        return Err(nfsstat4::NFS4ERR_INVAL);
    }
    if b == b"." || b == b".." || b.contains(&b'/') || b.contains(&0) {
        return Err(nfsstat4::NFS4ERR_BADNAME);
    }
    if b.len() > 255 {
        return Err(nfsstat4::NFS4ERR_NAMETOOLONG);
    }
    if utf8 && std::str::from_utf8(b).is_err() {
        return Err(if minor == 0 {
            nfsstat4::NFS4ERR_BADCHAR
        } else {
            nfsstat4::NFS4ERR_INVAL
        });
    }
    Ok(())
}

/// A wire time.
#[must_use]
pub fn time_of(t: &nfstime4) -> Time {
    Time {
        secs: t.seconds,
        nsecs: t.nseconds.min(999_999_999),
    }
}

/// Whether a set of times changes anything.
#[must_use]
pub fn sets_time(t: SetTime) -> bool {
    t != SetTime::Keep
}
