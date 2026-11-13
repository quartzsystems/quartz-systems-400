// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The kernel's file handles (`name_to_handle_at`, `open_by_handle_at`)
//! behind a safe API: the handle of a name under a directory, and a
//! node opened from a handle under any descriptor of its file system.
//! The calls are in `raw`, the crate's only `unsafe`; on another Unix
//! they answer `Unsupported`, and elsewhere there is only [`Handle`].
//! Opening by handle needs
//! `CAP_DAC_READ_SEARCH` (the storage service runs as root) and a
//! kernel built with `CONFIG_FHANDLE`.

#![deny(missing_docs)]

#[cfg(target_os = "linux")]
mod raw;

#[cfg(unix)]
use std::io;
#[cfg(unix)]
use std::os::fd::{BorrowedFd, OwnedFd};

/// Most bytes a handle holds (`MAX_HANDLE_SZ`).
pub const MAX_BYTES: usize = 128;

/// A file handle: the file system's type of it and its bytes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Handle {
    /// `handle_type`, the file system's (xfs: 0x81 for 64-bit inode
    /// numbers, 1 for 32-bit ones).
    pub kind: i32,
    /// `f_handle`.
    pub bytes: Vec<u8>,
}

#[cfg(unix)]
/// The handle of `name` under the directory `dir` (`""`: `dir` itself),
/// never following a final symbolic link.
///
/// # Errors
///
/// The system call's; `Unsupported` off Linux.
pub fn handle_at(dir: BorrowedFd<'_>, name: &str) -> io::Result<Handle> {
    #[cfg(target_os = "linux")]
    {
        raw::name_to_handle(dir, name)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (dir, name);
        Err(io::ErrorKind::Unsupported.into())
    }
}

#[cfg(unix)]
/// Open the node `h` names on the file system `mount` is on, with
/// `flags` (`O_PATH`, `O_RDONLY` ...; `O_CLOEXEC` is always added).
///
/// # Errors
///
/// `ESTALE` when no node has the handle (gone, another generation),
/// `EINVAL` for a malformed one; `Unsupported` off Linux.
pub fn open_by(mount: BorrowedFd<'_>, h: &Handle, flags: i32) -> io::Result<OwnedFd> {
    #[cfg(target_os = "linux")]
    {
        raw::open_by_handle(mount, h, flags)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (mount, h, flags);
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// `O_PATH`: open only to name the node.
#[cfg(target_os = "linux")]
pub const O_PATH: i32 = libc::O_PATH;

/// `O_PATH`: open only to name the node (no meaning off Linux).
#[cfg(all(unix, not(target_os = "linux")))]
pub const O_PATH: i32 = 0o10_000_000;

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn a_handle_opens_its_directory() {
        use std::os::fd::AsFd;
        let dir = std::fs::File::open(std::env::temp_dir()).unwrap();
        let h = match handle_at(dir.as_fd(), "") {
            Ok(h) => h,
            // A file system without handles (tmpfs has them; some do not).
            Err(e) if e.raw_os_error() == Some(libc::EOPNOTSUPP) => return,
            Err(e) => panic!("{e}"),
        };
        assert!(!h.bytes.is_empty() && h.bytes.len() <= MAX_BYTES);
        match open_by(dir.as_fd(), &h, O_PATH) {
            Ok(_) => {}
            // Opening by handle needs CAP_DAC_READ_SEARCH.
            Err(e) if e.raw_os_error() == Some(libc::EPERM) => {}
            Err(e) => panic!("{e}"),
        }
        let bad = Handle {
            kind: h.kind,
            bytes: vec![0xFF; MAX_BYTES + 1],
        };
        assert!(open_by(dir.as_fd(), &bad, O_PATH).is_err());
    }
}
