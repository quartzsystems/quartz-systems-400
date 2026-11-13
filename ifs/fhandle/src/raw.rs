// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The two system calls, on a `struct file_handle` this module owns: the
//! crate's only `unsafe`.

#![allow(unsafe_code)]

use std::ffi::CString;
use std::io;
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};

use super::{Handle, MAX_BYTES};

/// `struct file_handle` with room for the largest handle.
#[repr(C)]
struct FileHandle {
    handle_bytes: u32,
    handle_type: i32,
    f_handle: [u8; MAX_BYTES],
}

impl FileHandle {
    fn empty() -> Self {
        Self {
            handle_bytes: u32::try_from(MAX_BYTES).expect("small"),
            handle_type: 0,
            f_handle: [0; MAX_BYTES],
        }
    }
}

pub(super) fn name_to_handle(dir: BorrowedFd<'_>, name: &str) -> io::Result<Handle> {
    let c = CString::new(name).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let flags = if name.is_empty() {
        libc::AT_EMPTY_PATH
    } else {
        0
    };
    let mut fh = FileHandle::empty();
    let mut mount_id: libc::c_int = 0;
    // SAFETY: the path is a NUL-terminated string that lives across the
    // call; `fh` says it has MAX_BYTES of room, which it has, and the
    // kernel writes at most that; `mount_id` is a valid c_int.
    let r = unsafe {
        libc::syscall(
            libc::SYS_name_to_handle_at,
            dir.as_raw_fd(),
            c.as_ptr(),
            &raw mut fh,
            &raw mut mount_id,
            flags,
        )
    };
    if r < 0 {
        return Err(io::Error::last_os_error());
    }
    let n = usize::try_from(fh.handle_bytes)
        .unwrap_or(MAX_BYTES)
        .min(MAX_BYTES);
    Ok(Handle {
        kind: fh.handle_type,
        bytes: fh.f_handle[..n].to_vec(),
    })
}

pub(super) fn open_by_handle(mount: BorrowedFd<'_>, h: &Handle, flags: i32) -> io::Result<OwnedFd> {
    if h.bytes.len() > MAX_BYTES || h.bytes.is_empty() {
        return Err(io::Error::from_raw_os_error(libc::EINVAL));
    }
    let mut fh = FileHandle::empty();
    fh.handle_bytes = u32::try_from(h.bytes.len()).expect("small");
    fh.handle_type = h.kind;
    fh.f_handle[..h.bytes.len()].copy_from_slice(&h.bytes);
    // SAFETY: `fh` is a complete file_handle whose handle_bytes does not
    // pass its buffer; the kernel only reads it.
    let r = unsafe {
        libc::syscall(
            libc::SYS_open_by_handle_at,
            mount.as_raw_fd(),
            &raw const fh,
            flags | libc::O_CLOEXEC,
        )
    };
    if r < 0 {
        return Err(io::Error::last_os_error());
    }
    let fd = libc::c_int::try_from(r).map_err(|_| io::Error::from_raw_os_error(libc::EBADF))?;
    // SAFETY: the kernel just returned `fd` as a new descriptor that
    // nothing else owns.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}
