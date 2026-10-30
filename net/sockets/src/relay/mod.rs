// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The descriptor relay between job processes. The storage service
//! runs [`serve`] on a Unix socket ([`SOCKET`]); a job gives a kernel
//! descriptor to another job's 16-byte identifier, or takes one given
//! to it, each over a connection of its own with the descriptor riding
//! `SCM_RIGHTS`. A prestart job takes as its server's pool, so a daemon
//! hands a connection to whichever of them asks first. [`Relay`] is
//! `givedescriptor`/`takedescriptor` for a job.
//!
//! The messages: give is `1`, target, source, a note's length and the
//! note, with the descriptor, answered by one status byte; take is `2`,
//! the taker, the source wanted (zeros for any) and a wait in
//! milliseconds (u32, 0 forever), answered by a status byte, the
//! source, the note and the descriptor; idle is `3` and a pool,
//! answered by the count of takers waiting (u32).

#[cfg(target_os = "linux")]
mod linux;
mod state;

#[cfg(target_os = "linux")]
pub use linux::{give, idle, serve, take};
pub use state::State;

use crate::pass::{DescriptorPassing, JobId};
use crate::sys::Fd;
use crate::Errno;

/// Where the storage service listens.
pub const SOCKET: &str = "/run/qs400/fdrelay";

/// Operation codes.
pub mod op {
    /// Give a descriptor.
    pub const GIVE: u8 = 1;
    /// Take one.
    pub const TAKE: u8 = 2;
    /// Count the takers waiting.
    pub const IDLE: u8 = 3;
}

/// Status bytes.
pub mod status {
    /// Done.
    pub const OK: u8 = 0;
    /// Nothing came in the time asked.
    pub const TIMEOUT: u8 = 1;
    /// The request was not understood.
    pub const BAD: u8 = 2;
}

/// An identifier for a pool of prestart jobs: the program name in
/// ASCII, blank padded, with `*` first so it is never a job's.
pub fn pool(program: &str) -> JobId {
    let mut id = [b' '; 16];
    id[0] = b'*';
    for (i, b) in program.bytes().take(15).enumerate() {
        id[i + 1] = b;
    }
    id
}

/// What a take brought.
#[derive(Debug)]
pub struct Taken {
    /// The descriptor.
    pub fd: Fd,
    /// Who gave it.
    pub source: JobId,
    /// The giver's note.
    pub note: Vec<u8>,
}

/// `givedescriptor`/`takedescriptor` through the relay of [`SOCKET`].
#[derive(Clone, Debug)]
pub struct Relay {
    /// This job.
    pub me: JobId,
    /// The relay's socket.
    pub path: String,
}

impl Relay {
    /// A job on the system's relay.
    #[must_use]
    pub fn new(me: JobId) -> Self {
        Self {
            me,
            path: SOCKET.to_string(),
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod host {
    use super::{Errno, Fd, JobId, Taken};

    /// No relay off Linux.
    pub fn give(_path: &str, _fd: &Fd, _target: &JobId, _source: &JobId, _note: &[u8]) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    /// No relay off Linux.
    pub fn take(_path: &str, _me: &JobId, _source: Option<&JobId>, _wait_ms: u32) -> Result<Option<Taken>, Errno> {
        Err(Errno::ENOSYS)
    }

    /// No relay off Linux.
    pub fn idle(_path: &str, _pool: &JobId) -> Result<u32, Errno> {
        Err(Errno::ENOSYS)
    }
}

#[cfg(not(target_os = "linux"))]
pub use host::{give, idle, take};

impl DescriptorPassing for Relay {
    fn give(&mut self, fd: Fd, target: &JobId) -> Result<(), Errno> {
        give(&self.path, &fd, target, &self.me, &[])
    }

    fn take(&mut self, source: Option<&JobId>) -> Result<Fd, Errno> {
        loop {
            if let Some(t) = take(&self.path, &self.me, source, 0)? {
                return Ok(t.fd);
            }
        }
    }
}
