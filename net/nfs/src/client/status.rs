// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The servers' statuses as the IFS's errors (docs/NFS.md section 7.7
//! has the table).

use ifs::Errno;
use ifs::remote::RemoteError;

use crate::types::mount::mountstat3;
use crate::types::nfs3::nfsstat3;

/// An NFS version 3 status (not `NFS3_OK`, not `JUKEBOX`, which is
/// retried before this).
#[must_use]
pub fn nfs3(s: nfsstat3) -> RemoteError {
    use nfsstat3 as S;
    let e = match s {
        S::NFS3ERR_STALE | S::NFS3ERR_BADHANDLE => return RemoteError::Stale,
        S::NFS3ERR_PERM => Errno::EPERM,
        S::NFS3ERR_NOENT => Errno::ENOENT,
        S::NFS3ERR_NXIO => Errno::ENXIO,
        S::NFS3ERR_ACCES => Errno::EACCES,
        S::NFS3ERR_EXIST => Errno::EEXIST,
        S::NFS3ERR_XDEV => Errno::EXDEV,
        S::NFS3ERR_NODEV => Errno::ENODEV,
        S::NFS3ERR_NOTDIR => Errno::ENOTDIR,
        S::NFS3ERR_ISDIR => Errno::EISDIR,
        S::NFS3ERR_INVAL | S::NFS3ERR_BAD_COOKIE | S::NFS3ERR_TOOSMALL => Errno::EINVAL,
        S::NFS3ERR_FBIG => Errno::EFBIG,
        S::NFS3ERR_NOSPC | S::NFS3ERR_DQUOT => Errno::ENOSPC,
        S::NFS3ERR_ROFS => Errno::EROFS,
        S::NFS3ERR_MLINK => Errno::EMLINK,
        S::NFS3ERR_NAMETOOLONG => Errno::ENAMETOOLONG,
        S::NFS3ERR_NOTEMPTY => Errno::ENOTEMPTY,
        // The guard's time no longer holds: nothing changed.
        S::NFS3ERR_NOT_SYNC => Errno::EAGAIN,
        S::NFS3ERR_NOTSUPP | S::NFS3ERR_BADTYPE | S::NFS3ERR_REMOTE => Errno::ENOTSUP,
        S::NFS3_OK | S::NFS3ERR_IO | S::NFS3ERR_SERVERFAULT | S::NFS3ERR_JUKEBOX => Errno::EIO,
    };
    RemoteError::Errno(e)
}

/// A MOUNT version 3 status.
#[must_use]
pub fn mnt3(s: mountstat3) -> RemoteError {
    use mountstat3 as S;
    RemoteError::Errno(match s {
        S::MNT3ERR_PERM => Errno::EPERM,
        S::MNT3ERR_NOENT => Errno::ENOENT,
        S::MNT3ERR_ACCES => Errno::EACCES,
        S::MNT3ERR_NOTDIR => Errno::ENOTDIR,
        S::MNT3ERR_INVAL => Errno::EINVAL,
        S::MNT3ERR_NAMETOOLONG => Errno::ENAMETOOLONG,
        S::MNT3ERR_NOTSUPP => Errno::ENOTSUP,
        S::MNT3_OK | S::MNT3ERR_IO | S::MNT3ERR_SERVERFAULT => Errno::EIO,
    })
}
