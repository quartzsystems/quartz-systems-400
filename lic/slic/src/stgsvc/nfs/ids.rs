// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NFS client's `NFSv4` identity mapping (`nfs::registry::IdMap`),
//! answered inside the service: the profiles' names, UIDs and GIDs as
//! the service's thread last indexed them (again whenever a profile,
//! group or list changes: `qsyslib::env::stamp`), names mapped only
//! while the `*RGY` daemon is ready, the domain the NFS options' or the
//! TCP/IP domain name. A mount's threads read the snapshot; the
//! service's thread takes it ([`refresh`]) before each `MOUNT` and at
//! each round of its loop ([`tick`]).

use std::sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError};

use nfs::registry::{IdMap, Registry, TTL};
use nfs::server::SystemClock;
use qsyslib::ids::{IdIndex, Row};
use stg::nfs::Daemon;

use crate::ipl::system::System;

/// What the mounts' threads read.
#[derive(Default)]
struct Snapshot {
    index: Option<Arc<IdIndex>>,
    stamp: Option<(u64, u64)>,
    mapping: bool,
    domain: String,
}

static SNAP: LazyLock<Mutex<Snapshot>> = LazyLock::new(Mutex::default);

fn snap() -> MutexGuard<'static, Snapshot> {
    SNAP.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The snapshot as an [`IdMap`].
pub struct ServiceIdMap;

impl IdMap for ServiceIdMap {
    fn domain(&self) -> String {
        snap().domain.clone()
    }
    fn user_name(&self, uid: u32) -> Option<String> {
        snap().index.as_ref()?.by_uid(uid).map(|r| r.name.clone())
    }
    fn user_id(&self, name: &str) -> Option<u32> {
        snap().index.as_ref()?.by_name(name).map(|r| r.uid)
    }
    fn group_name(&self, gid: u32) -> Option<String> {
        snap().index.as_ref()?.by_gid(gid).map(|r| r.name.clone())
    }
    fn group_id(&self, name: &str) -> Option<u32> {
        snap()
            .index
            .as_ref()?
            .by_name(name)
            .filter(|r| r.gid != 0)
            .map(|r| r.gid)
    }
    fn mapping(&self) -> bool {
        snap().mapping
    }
}

static REGISTRY: LazyLock<Arc<Registry>> = LazyLock::new(|| {
    Arc::new(Registry::new(
        Arc::new(ServiceIdMap),
        TTL,
        Arc::new(SystemClock),
    ))
});

/// The registry every mount shares.
#[must_use]
pub fn registry() -> Arc<Registry> {
    Arc::clone(&REGISTRY)
}

/// The profiles as rows, read from the store.
fn rows(system: &mut System) -> Vec<Row> {
    sec::guard::as_system(|| {
        crate::stgsvc::spool::with_store(system, |txn, qsys| {
            let mut v = Vec::new();
            for n in sec::profile::objects::list(txn, qsys).map_err(|e| e.to_string())? {
                if let Ok((_, p)) = sec::profile::objects::read(txn, qsys, &n) {
                    v.push(Row {
                        name: p.name.clone(),
                        uid: p.uid,
                        gid: p.gid,
                        groups: p.groups(),
                    });
                }
            }
            Ok(v)
        })
    })
    .unwrap_or_default()
}

/// Whether names are mapped now: the `*RGY` daemon is ready.
fn mapping_now() -> bool {
    super::state().registry.ready(Daemon::Rgy) > 0
}

/// Take the profiles again when they changed, and whether names are
/// mapped; the cached answers go when either moved.
pub fn tick(system: &mut System) {
    let stamp = qsyslib::env::stamp();
    let mapping = mapping_now();
    let (stale, flipped) = {
        let s = snap();
        if s.stamp.is_none() {
            // No mount was made yet: nothing reads it.
            return;
        }
        (s.stamp != Some(stamp), s.mapping != mapping)
    };
    if !stale && !flipped {
        return;
    }
    if stale {
        let index = Arc::new(IdIndex::new(rows(system)));
        let mut s = snap();
        s.index = Some(index);
        s.stamp = Some(stamp);
    }
    if flipped {
        let domain = super::domain(system);
        let mut s = snap();
        s.mapping = mapping;
        s.domain = domain;
    }
    REGISTRY.clear();
}

/// Before a `MOUNT`: the profiles, whether names are mapped and the
/// domain, all taken now.
pub fn refresh(system: &mut System) {
    snap().stamp.get_or_insert((u64::MAX, u64::MAX));
    tick(system);
    let domain = super::domain(system);
    let changed = {
        let mut s = snap();
        let changed = s.domain != domain;
        s.domain = domain;
        changed
    };
    if changed {
        REGISTRY.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(mapping: bool) {
        let mut s = snap();
        s.index = Some(Arc::new(IdIndex::new(vec![
            Row {
                name: "BOB".into(),
                uid: 100,
                gid: 0,
                groups: vec!["DEV".into()],
            },
            Row {
                name: "DEV".into(),
                uid: 500,
                gid: 500,
                groups: Vec::new(),
            },
        ])));
        s.mapping = mapping;
        s.domain = "lab.example".into();
        drop(s);
        REGISTRY.clear();
    }

    /// One test: the snapshot is the process's.
    #[test]
    fn a_mounts_owners_follow_the_registry_daemon() {
        set(false);
        let r = registry();
        assert_eq!(r.owner(100, None), "100");
        assert_eq!(
            r.uid(b"BOB@lab.example"),
            Err(nfs::registry::IdError::BadOwner)
        );
        assert_eq!(r.uid(b"100"), Ok(100));
        set(true);
        assert_eq!(r.owner(100, None), "BOB@lab.example");
        assert_eq!(r.group(500, None), "DEV@lab.example");
        assert_eq!(r.uid(b"bob@LAB.EXAMPLE"), Ok(100));
        assert_eq!(r.gid(b"DEV@lab.example"), Ok(500));
    }
}
