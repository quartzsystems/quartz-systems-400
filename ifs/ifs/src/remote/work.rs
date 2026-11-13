// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The work a mount's queue holds: fetches that fill the cache and
//! changes made once for a request, and what a change answers.

use super::types::{
    CreateMode, Cred, LockConflict, LockKind, LockOwner, OpenToken, RAttr, RHandle, RKind, SetAttr,
    Share,
};

/// A change to the remote file system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Mutation {
    Setattr {
        h: RHandle,
        set: SetAttr,
    },
    Create {
        dir: RHandle,
        name: Vec<u8>,
        how: CreateMode,
    },
    Mkdir {
        dir: RHandle,
        name: Vec<u8>,
        set: SetAttr,
    },
    Symlink {
        dir: RHandle,
        name: Vec<u8>,
        target: Vec<u8>,
        set: SetAttr,
    },
    Mknod {
        dir: RHandle,
        name: Vec<u8>,
        kind: RKind,
        set: SetAttr,
    },
    Remove {
        dir: RHandle,
        name: Vec<u8>,
        /// The handle the name had (dropped from the cache).
        h: Option<RHandle>,
    },
    Rmdir {
        dir: RHandle,
        name: Vec<u8>,
        h: Option<RHandle>,
    },
    Rename {
        from_dir: RHandle,
        from: Vec<u8>,
        to_dir: RHandle,
        to: Vec<u8>,
    },
    Link {
        h: RHandle,
        dir: RHandle,
        name: Vec<u8>,
    },
    Open {
        h: RHandle,
        share: Share,
        owner: LockOwner,
    },
    Close {
        h: RHandle,
        token: OpenToken,
    },
    Lock {
        h: RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
        token: OpenToken,
    },
    Unlock {
        h: RHandle,
        owner: LockOwner,
        off: u64,
        len: u64,
    },
    Test {
        h: RHandle,
        owner: LockOwner,
        kind: LockKind,
        off: u64,
        len: u64,
    },
    /// Write a file's dirty blocks and commit them.
    Flush {
        h: RHandle,
    },
    /// Write every file's dirty blocks (before an unmount).
    FlushAll,
    /// Tell the server the mount ends.
    Unmount,
}

/// What a change answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Answer {
    Unit,
    Attr(RAttr),
    Node(RHandle, RAttr),
    Token(OpenToken),
    Conflict(Option<LockConflict>),
}

/// One piece of queued work.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Work {
    /// Attributes.
    Getattr { h: RHandle, cred: Cred },
    /// The caller's access bits.
    Access { h: RHandle, cred: Cred },
    /// Names looked up one after the other from `dir` (stopping at the
    /// first that is not a directory or fails).
    Walk {
        dir: RHandle,
        names: Vec<Vec<u8>>,
        cred: Cred,
    },
    /// A link's text.
    Readlink { h: RHandle, cred: Cred },
    /// A whole directory.
    Readdir { dir: RHandle, cred: Cred },
    /// One block of data.
    Read { h: RHandle, cred: Cred, block: u64 },
    /// One dirty block written ahead, unstable (write-behind).
    WriteBlock { h: RHandle, cred: Cred, block: u64 },
    /// Capacity.
    Statfs { cred: Cred },
    /// A change, for request `token`'s slot `key` (none: nobody waits).
    Mutate {
        m: Mutation,
        cred: Cred,
        token: Option<(u64, String)>,
    },
}

impl Work {
    /// Whether a block I/O helper job may take it.
    pub(crate) fn for_helper(&self) -> bool {
        matches!(self, Self::Read { .. } | Self::WriteBlock { .. })
    }
}
