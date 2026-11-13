// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NFS versions 4.0 and 4.1 mounts: TCP to port 2049 (or `port=`), no
//! binder; the client ID and session ([`core`]), the export found from
//! the server's root (`PUTROOTFH`, a `LOOKUP` per component, crossing
//! its pseudo file system), the lease kept by a thread of the mount
//! (`SEQUENCE` or `RENEW` at two thirds of it), every call as a
//! `COMPOUND` ([`fs`]), the opens and locks ([`state`]).

// Target, options, core: the short names are the domain's.
#![allow(clippy::many_single_char_names)]

pub mod attr;
mod core;
mod fs;
mod state;

use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ifs::Errno;
use ifs::remote::{RHandle, RKind, RResult, RemoteError};

use self::core::Core;
use super::Target;
use super::channel::{Channel, Status};
use super::settings::{Policy, Settled, Version};
use crate::rpc::Proto;
use crate::types::nfs4::{
    GETATTR4args, GETATTR4res, GETFH4res, LOOKUP4args, nfs_argop4, nfs_resop4, utf8string,
};

/// A version 4 mount.
pub struct V4 {
    core: Arc<Core>,
    root: RHandle,
    version: Version,
    rsize: u32,
    status: Arc<Status>,
    renewer: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for V4 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("V4")
            .field("core", &self.core)
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

impl V4 {
    /// The minor version in use.
    #[must_use]
    pub fn version(&self) -> Version {
        self.version
    }

    /// Locked ranges held.
    #[must_use]
    pub fn locks_held(&self) -> usize {
        self.core.state().locks.values().map(|l| l.held.len()).sum()
    }

    /// Opens held.
    #[must_use]
    pub fn opens_held(&self) -> usize {
        self.core.state().opens.len()
    }
}

impl Drop for V4 {
    fn drop(&mut self) {
        self.core.stopped.store(true, Ordering::Relaxed);
    }
}

/// The lease kept: `SEQUENCE` or `RENEW` at two thirds of it.
fn renewer(core: &Weak<Core>) {
    let mut last = Instant::now();
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let Some(c) = core.upgrade() else { return };
        if c.stopped.load(Ordering::Relaxed) {
            return;
        }
        let lease = *c.lease.lock().unwrap_or_else(PoisonError::into_inner);
        if last.elapsed() >= lease * 2 / 3 {
            if let Err(e) = c.renew() {
                tracing::warn!(?e, "NFSv4 lease not renewed");
            }
            last = Instant::now();
        }
    }
}

/// Mount `t` with version 4.1 (`minor1`) or 4.0.
pub(super) fn mount(t: &Target<'_>, status: &Arc<Status>, minor1: bool) -> RResult<(V4, Settled)> {
    let o = t.opts;
    let port = o.port.unwrap_or(2049);
    let policy = Policy::of(o);
    let soft = Policy {
        soft: true,
        ..policy
    };
    let chan = Channel::new(
        SocketAddr::new(t.ip, port),
        Proto::Tcp,
        soft,
        Arc::clone(status),
        t.env.reserved_port,
        t.env.connections,
    );
    let host = t.env.host_name.as_bytes().to_vec();
    let owner = super::wire_cred(&host, t.cred);
    let mut core = Core::new(
        chan,
        u32::from(minor1),
        policy,
        &host,
        owner.clone(),
        t.env.registry.clone(),
    );
    core.establish()?;
    core.reclaim_complete()?;
    // The export from the server's root, a component at a time.
    let comps: Vec<Vec<u8>> = t
        .path
        .split(|b| *b == b'/')
        .filter(|c| !c.is_empty())
        .map(<[u8]>::to_vec)
        .collect();
    let mut want = attr::OBJECT.to_vec();
    want.extend_from_slice(attr::LIMITS);
    let list = core.run(&owner, &|_| {
        let mut v = vec![nfs_argop4::OP_PUTROOTFH];
        v.extend(comps.iter().map(|c| {
            nfs_argop4::OP_LOOKUP(LOOKUP4args {
                objname: utf8string(c.clone()),
            })
        }));
        v.push(nfs_argop4::OP_GETFH);
        v.push(nfs_argop4::OP_GETATTR(GETATTR4args {
            attr_request: attr::bitmap(&want),
        }));
        v
    })?;
    let bad = || RemoteError::Errno(Errno::EIO);
    let n = list.len();
    let (root, a) = match (list.get(n.wrapping_sub(2)), list.last()) {
        (
            Some(nfs_resop4::OP_GETFH(GETFH4res::NFS4_OK(fh))),
            Some(nfs_resop4::OP_GETATTR(GETATTR4res::NFS4_OK(a))),
        ) => (
            RHandle(fh.object.0.clone()),
            attr::decode(&a.obj_attributes).map_err(|_| bad())?,
        ),
        _ => return Err(bad()),
    };
    if attr::kind(a.ftype.unwrap_or(0)) != RKind::Dir {
        return Err(RemoteError::Errno(Errno::ENOTDIR));
    }
    let cap = |want: u32, max: Option<u64>| {
        max.filter(|m| *m > 0)
            .map_or(want, |m| want.min(u32::try_from(m).unwrap_or(u32::MAX)))
            .max(512)
    };
    let rsize = cap(o.rsize, a.maxread);
    let wsize = cap(o.wsize, a.maxwrite);
    *core.lease.lock().unwrap_or_else(PoisonError::into_inner) =
        Duration::from_secs(u64::from(a.lease_time.unwrap_or(90).max(1)));
    // From now on calls follow the mount's policy.
    core.chan = core.chan.with_policy(policy);
    let core = Arc::new(core);
    let w = Arc::downgrade(&core);
    let renew = std::thread::Builder::new()
        .name("qs-nfs4-lease".into())
        .spawn(move || renewer(&w))
        .ok();
    let version = if minor1 { Version::V41 } else { Version::V40 };
    let settled = Settled {
        version,
        proto: Proto::Tcp,
        port,
        mountport: None,
        rsize,
        wsize,
        case_sensitive: !a.case_insensitive.unwrap_or(false),
        locks: true,
    };
    let v = V4 {
        core,
        root,
        version,
        rsize,
        status: Arc::clone(status),
        renewer: Mutex::new(renew),
    };
    Ok((v, settled))
}
