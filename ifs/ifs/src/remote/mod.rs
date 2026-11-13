// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Mounts whose file system is somewhere else (`MOUNT TYPE(*NFS)`).
//! The IFS knows no protocol: a remote file system implements
//! [`RemoteFs`] and a [`RemoteFactory`] the storage service registers
//! makes one per mount. The service answers every job on one thread,
//! so nothing here blocks the caller of [`crate::Ifs`]: a mount works
//! on its [`cache`] and, for what is missing and for every change,
//! queues work its own threads do ([`mount`], [`worker`]); the
//! operation defers (`EAGAIN` with a `Deferral::Remote`, as a lease
//! wait does) and runs again when the work is done ([`tickets`]). A
//! change is made once per request ([`request`]). `docs/NFS.md`
//! section 7 has the design.

pub mod cache;
pub mod fs;
pub mod helper;
pub mod mem;
pub mod mount;
pub mod request;
pub mod tickets;
pub mod types;
pub mod wire;
pub(crate) mod work;
mod worker;

use std::sync::{Arc, Mutex, PoisonError};

pub use fs::{Mounted, RemoteFactory, RemoteFs, access};
pub use helper::{HelperOp, HelperResult, HelperWork};
pub use mount::{Events, RemoteConfig, RemoteEvent, RemoteMount};
pub use types::*;

/// A remote mount in the mount table: compared by identity.
#[derive(Clone)]
pub struct RemoteRef(pub Arc<RemoteMount>);

impl PartialEq for RemoteRef {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for RemoteRef {}

impl std::fmt::Debug for RemoteRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// What the storage service registered: the factory, the settings of
/// every mount, and where events go.
#[derive(Default)]
pub(crate) struct Registry {
    factory: Mutex<Option<Arc<dyn RemoteFactory>>>,
    config: Mutex<RemoteConfig>,
    events: Mutex<Option<Events>>,
}

impl Registry {
    pub(crate) fn set_factory(&self, f: Option<Arc<dyn RemoteFactory>>, cfg: RemoteConfig) {
        *self.factory.lock().unwrap_or_else(PoisonError::into_inner) = f;
        *self.config.lock().unwrap_or_else(PoisonError::into_inner) = cfg;
    }

    pub(crate) fn factory(&self) -> Option<Arc<dyn RemoteFactory>> {
        self.factory
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(crate) fn config(&self) -> RemoteConfig {
        *self.config.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn set_events(&self, e: Option<Events>) {
        *self.events.lock().unwrap_or_else(PoisonError::into_inner) = e;
    }

    pub(crate) fn events(&self) -> Option<Events> {
        self.events
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// The ASCII CCSID matching a job CCSID (`*ASCII`): 819 for the Latin
/// EBCDIC CCSIDs, the job's own when it is ASCII-based or UTF-8.
#[must_use]
pub fn ascii_of(job: u16) -> Option<u16> {
    match job {
        37 | 273 | 277 | 278 | 280 | 284 | 285 | 297 | 500 | 871 => Some(819),
        819 | 850 | 1252 | 367 | 1208 => Some(job),
        _ => None,
    }
}

/// Who a job is on the wire: its user profile's UID, its first group
/// profile's GID and every group's GID (from the profile environment).
#[must_use]
pub fn cred_of(cx: &crate::ops::Ctx<'_>) -> Cred {
    let uid = cx.env.uid_of(&cx.principal.user).unwrap_or(Cred::NOBODY);
    let gids: Vec<u32> = cx
        .principal
        .groups
        .iter()
        .filter_map(|g| cx.env.gid_of(g))
        .collect();
    Cred {
        uid,
        gid: gids.first().copied().unwrap_or(Cred::NOBODY),
        gids,
    }
}

/// Remote attributes in the backend's form (for the fallback record
/// `stat` and `DSPAUT` show).
#[must_use]
pub fn bstat_of(a: &RAttr) -> crate::backend::BStat {
    use crate::backend::Kind;
    crate::backend::BStat {
        kind: match a.kind {
            RKind::File => Kind::File,
            RKind::Dir => Kind::Dir,
            RKind::Symlink => Kind::Symlink,
            RKind::Fifo => Kind::Fifo,
            RKind::Socket => Kind::Socket,
            RKind::Chr | RKind::Blk => Kind::CharDev,
        },
        ino: a.fileid,
        dev: 0,
        nlink: a.nlink,
        size: a.size,
        alloc: a.used,
        blksize: 4096,
        atime: a.atime,
        mtime: a.mtime,
        ctime: a.ctime,
        mode: a.mode,
        uid: a.uid,
        gid: a.gid,
    }
}
