// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! [`VfsError`]: why a [`super::Vfs`] call failed, one list every
//! protocol maps to its own status (`nfsstat3`, `nfsstat4`, `nlm4_stats`).

use ifs::Errno;

/// A [`super::Vfs`] call's failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum VfsError {
    /// Not the owner, or not allowed whatever the authority.
    #[error("not permitted")]
    Perm,
    /// No such object.
    #[error("no such object")]
    NoEnt,
    /// An input or output error.
    #[error("input or output error")]
    Io,
    /// No such device.
    #[error("no such device")]
    Nxio,
    /// Not authorized.
    #[error("not authorized")]
    Acces,
    /// The object exists.
    #[error("the object exists")]
    Exist,
    /// Across file systems or exports.
    #[error("across file systems")]
    XDev,
    /// Not a directory.
    #[error("not a directory")]
    NotDir,
    /// A directory.
    #[error("a directory")]
    IsDir,
    /// An argument is not valid.
    #[error("not valid")]
    Inval,
    /// The file would be too large.
    #[error("file too large")]
    FBig,
    /// No space is left.
    #[error("no space left")]
    NoSpc,
    /// The export (or the file system) is read-only for the caller.
    #[error("read-only")]
    RoFs,
    /// Too many links.
    #[error("too many links")]
    MLink,
    /// A name is too long.
    #[error("name too long")]
    NameTooLong,
    /// The directory is not empty.
    #[error("directory not empty")]
    NotEmpty,
    /// The handle's object is gone or no longer exported.
    #[error("stale handle")]
    Stale,
    /// The handle is not one of this server's, or lies outside its export.
    #[error("bad handle")]
    BadHandle,
    /// A `SETATTR` guard did not match.
    #[error("guard did not match")]
    NotSync,
    /// A directory cookie is not valid for the directory.
    #[error("bad cookie")]
    BadCookie,
    /// Not supported.
    #[error("not supported")]
    NotSupp,
    /// A buffer is too small for one entry.
    #[error("too small")]
    TooSmall,
    /// The server failed.
    #[error("server fault")]
    ServerFault,
    /// The object's type cannot be made.
    #[error("bad type")]
    BadType,
    /// A share reservation refuses the open.
    #[error("share denied")]
    Busy,
    /// A byte-range lock is in the way.
    #[error("would block")]
    Again,
    /// Waiting would never end.
    #[error("deadlock")]
    Deadlock,
    /// No locks are available.
    #[error("no locks")]
    NoLocks,
    /// Too many symbolic links.
    #[error("too many links followed")]
    Loop,
}

impl From<Errno> for VfsError {
    fn from(e: Errno) -> Self {
        match e {
            Errno::EPERM => Self::Perm,
            Errno::ENOENT | Errno::ENOREC => Self::NoEnt,
            Errno::ENXIO => Self::Nxio,
            Errno::EACCES | Errno::ENOTENROLL | Errno::EROOBJ => Self::Acces,
            Errno::EEXIST => Self::Exist,
            Errno::EXDEV => Self::XDev,
            Errno::ENOTDIR => Self::NotDir,
            Errno::EISDIR => Self::IsDir,
            Errno::EINVAL | Errno::EBADNAME | Errno::EILSEQ | Errno::ECONVERT => Self::Inval,
            Errno::EFBIG | Errno::EFBIG2 | Errno::EOVERFLOW => Self::FBig,
            Errno::ENOSPC => Self::NoSpc,
            Errno::EROFS => Self::RoFs,
            Errno::EMLINK => Self::MLink,
            Errno::ENAMETOOLONG => Self::NameTooLong,
            Errno::ENOTEMPTY => Self::NotEmpty,
            Errno::ESTALE | Errno::EBADFID => Self::Stale,
            Errno::ENOTSUP | Errno::ENOSYS | Errno::EBADFUNC => Self::NotSupp,
            Errno::EBUSY | Errno::ETXTBSY | Errno::ELOCKED => Self::Busy,
            Errno::EAGAIN => Self::Again,
            Errno::EDEADLK => Self::Deadlock,
            Errno::ENOLCK => Self::NoLocks,
            Errno::ELOOP => Self::Loop,
            _ => Self::Io,
        }
    }
}

/// A [`super::Vfs`] result.
pub type VResult<T> = Result<T, VfsError>;
