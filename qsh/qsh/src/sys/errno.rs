// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `errno` values as the system numbers them (`QSYSINC/H(ERRNO)`), with
//! Quartz-written texts. `ENOEXEC` has no system value; 3600 is QS/400's
//! and never leaves the shell.

use std::fmt;
use std::io;

/// An `errno` value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Errno(pub i32);

macro_rules! errnos {
    ($($name:ident = $n:expr, $text:expr;)*) => {
        impl Errno {
            $(
                #[doc = $text]
                pub const $name: Errno = Errno($n);
            )*

            /// The message text.
            pub fn text(self) -> &'static str {
                match self.0 {
                    $($n => $text,)*
                    _ => "Unknown error.",
                }
            }
        }
    };
}

errnos! {
    EINVAL = 3021, "The argument is not valid.";
    EIO = 3006, "An input or output error occurred.";
    EBUSY = 3029, "The resource is busy.";
    ENOENT = 3025, "No such path or directory.";
    EPERM = 3027, "The operation is not permitted.";
    EACCES = 3401, "Permission is denied.";
    ENOTDIR = 3403, "The path is not a directory.";
    ENOSPC = 3404, "No space is left.";
    EXDEV = 3405, "The link crosses file systems.";
    EAGAIN = 3406, "The operation would block.";
    EINTR = 3407, "The call was interrupted.";
    EBADF = 3450, "The descriptor is not valid.";
    EMFILE = 3452, "Too many files are open in this job.";
    EPIPE = 3455, "The pipe has no reader.";
    EEXIST = 3457, "The file already exists.";
    ENOMEM = 3460, "Storage could not be allocated.";
    ESPIPE = 3469, "The object cannot seek.";
    EOPNOTSUPP = 3440, "The operation is not supported.";
    ENOSYS = 3470, "The function is not available.";
    EISDIR = 3471, "The target is a directory.";
    EROFS = 3472, "The file system is read-only.";
    ELOOP = 3485, "Symbolic links form a loop.";
    ENAMETOOLONG = 3486, "The path name is too long.";
    ENOTEMPTY = 3488, "The directory is not empty.";
    ESRCH = 3515, "No such process.";
    ECHILD = 3517, "No child process.";
    ENOTTY = 3536, "The descriptor is not a terminal.";
    E2BIG = 3491, "The argument list is too long.";
    ENXIO = 3415, "There is no such device or address.";
    ECONNREFUSED = 3425, "The connection was refused.";
    EHOSTUNREACH = 3429, "There is no route to the host.";
    ETIMEDOUT = 3447, "The connection timed out.";
    EIDRM = 3509, "The identifier was removed.";
    ENOMSG = 3510, "No message of the wanted type is available.";
    ENOEXEC = 3600, "The file is not a program.";
}

impl fmt::Display for Errno {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.text())
    }
}

/// A failed system operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{errno}")]
pub struct SysError {
    /// Why.
    pub errno: Errno,
}

impl SysError {
    /// An error with this `errno`.
    pub fn new(errno: Errno) -> Self {
        Self { errno }
    }
}

impl From<Errno> for SysError {
    fn from(errno: Errno) -> Self {
        Self { errno }
    }
}

impl From<io::Error> for SysError {
    fn from(e: io::Error) -> Self {
        use io::ErrorKind as K;
        if let Some(inner) = e.get_ref().and_then(|r| r.downcast_ref::<SysError>()) {
            return *inner;
        }
        let errno = match e.kind() {
            K::NotFound => Errno::ENOENT,
            K::PermissionDenied => Errno::EACCES,
            K::AlreadyExists => Errno::EEXIST,
            K::BrokenPipe => Errno::EPIPE,
            K::InvalidInput | K::InvalidData => Errno::EINVAL,
            K::Interrupted => Errno::EINTR,
            K::WouldBlock => Errno::EAGAIN,
            K::NotADirectory => Errno::ENOTDIR,
            K::IsADirectory => Errno::EISDIR,
            K::DirectoryNotEmpty => Errno::ENOTEMPTY,
            K::ReadOnlyFilesystem => Errno::EROFS,
            K::Unsupported => Errno::ENOSYS,
            K::OutOfMemory => Errno::ENOMEM,
            _ => Errno::EIO,
        };
        Self { errno }
    }
}

impl From<SysError> for io::Error {
    fn from(e: SysError) -> Self {
        let kind = match e.errno {
            Errno::ENOENT => io::ErrorKind::NotFound,
            Errno::EACCES => io::ErrorKind::PermissionDenied,
            Errno::EPIPE => io::ErrorKind::BrokenPipe,
            Errno::EEXIST => io::ErrorKind::AlreadyExists,
            _ => io::ErrorKind::Other,
        };
        io::Error::new(kind, e)
    }
}
