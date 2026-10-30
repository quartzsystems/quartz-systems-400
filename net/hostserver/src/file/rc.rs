// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The file server's return codes and the errno each comes from.

use ifs::Errno;

/// Success.
pub const SUCCESS: u16 = 0;
/// The file is in use.
pub const FILE_IN_USE: u16 = 1;
/// No such file.
pub const FILE_NOT_FOUND: u16 = 2;
/// No such path.
pub const PATH_NOT_FOUND: u16 = 3;
/// The name exists.
pub const DUPLICATE: u16 = 4;
/// Not authorized to the entry.
pub const ACCESS_DENIED: u16 = 5;
/// Not a valid handle.
pub const INVALID_HANDLE: u16 = 6;
/// Not a valid name.
pub const INVALID_NAME: u16 = 7;
/// The directory is not empty.
pub const DIR_NOT_EMPTY: u16 = 9;
/// Not authorized to the request.
pub const ACCESS_DENIED_TO_REQUEST: u16 = 13;
/// No more entries.
pub const NO_MORE_FILES: u16 = 18;
/// Not supported.
pub const REQUEST_NOT_SUPPORTED: u16 = 23;
/// Anything else.
pub const UNKNOWN: u16 = 25;
/// A share mode refused the open.
pub const SHARING_VIOLATION: u16 = 32;
/// A byte lock refused it.
pub const LOCK_VIOLATION: u16 = 33;

/// The return code for an IFS error; `listing` makes a missing
/// directory a missing path.
pub fn of(e: Errno, listing: bool) -> u16 {
    match e {
        Errno::ENOENT if listing => PATH_NOT_FOUND,
        Errno::ENOENT => FILE_NOT_FOUND,
        Errno::ENOTDIR => PATH_NOT_FOUND,
        Errno::EEXIST => DUPLICATE,
        Errno::EACCES | Errno::EPERM => ACCESS_DENIED,
        Errno::EBADF => INVALID_HANDLE,
        Errno::ENOTEMPTY => DIR_NOT_EMPTY,
        Errno::EBUSY => FILE_IN_USE,
        Errno::ENAMETOOLONG | Errno::EINVAL => INVALID_NAME,
        _ => UNKNOWN,
    }
}
