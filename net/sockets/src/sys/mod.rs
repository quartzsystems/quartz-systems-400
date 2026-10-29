// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system calls, through rustix on Linux and as `ENOSYS` stubs
//! elsewhere. Every function takes and gives IBM's numbers (families,
//! types, flags, options, `errno`); the translation to Linux's lives
//! here and nowhere else. A [`Fd`] owns a kernel descriptor;
//! [`crate::Sockets`] keeps them by number.

#[cfg(not(target_os = "linux"))]
mod host;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod opts;

#[cfg(not(target_os = "linux"))]
pub use host::*;
#[cfg(target_os = "linux")]
pub use linux::*;

/// What `recvmsg` brought back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Received {
    /// Bytes stored.
    pub bytes: usize,
    /// The sender, for an unconnected socket.
    pub from: Option<crate::SockAddr>,
    /// `MSG_*` flags on the message (`MSG_TRUNC`, `MSG_OOB`, ...).
    pub flags: i32,
}

/// What a socket is: family, type and protocol in IBM's numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    /// `AF_*`.
    pub family: i32,
    /// `SOCK_*`.
    pub socktype: i32,
    /// `IPPROTO_*`.
    pub protocol: i32,
}
