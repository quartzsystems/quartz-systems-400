// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `NFSv4` identity mapping of a server job (`nfs::registry::IdMap`):
//! profiles by UID and GID from the storage service (`NfsOp::NameOfUid`
//! ...), the domain and whether names are mapped at all from
//! `NfsOp::Mapping` (the `*RGY` daemon ready), asked at most every few
//! seconds. `nfs::registry::Registry` keeps the answers.

use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use nfs::registry::{IdMap, Registry, TTL};
use nfs::server::SystemClock;
use stg::nfs::{NfsOp, NfsReply};

use super::host::Host;

/// How long whether names are mapped, and the domain, are kept.
const MAPPING_KEPT: Duration = Duration::from_secs(2);

/// The service's profiles as an [`IdMap`].
pub struct JobIdMap<H: Host> {
    host: Arc<H>,
    mapping: Mutex<Option<(Instant, bool, String)>>,
}

impl<H: Host> JobIdMap<H> {
    /// Through `host`.
    #[must_use]
    pub fn new(host: Arc<H>) -> Self {
        Self {
            host,
            mapping: Mutex::new(None),
        }
    }

    fn mapping_now(&self) -> (bool, String) {
        let mut m = self.mapping.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((at, on, d)) = m.as_ref()
            && at.elapsed() < MAPPING_KEPT
        {
            return (*on, d.clone());
        }
        let (on, d) = match self.host.op(NfsOp::Mapping) {
            NfsReply::Mapping { enabled, domain } => (enabled, domain),
            _ => (false, String::new()),
        };
        *m = Some((Instant::now(), on, d.clone()));
        (on, d)
    }
}

impl<H: Host> IdMap for JobIdMap<H> {
    fn domain(&self) -> String {
        self.mapping_now().1
    }
    fn user_name(&self, uid: u32) -> Option<String> {
        match self.host.op(NfsOp::NameOfUid { uid }) {
            NfsReply::Name { name } => name,
            _ => None,
        }
    }
    fn user_id(&self, name: &str) -> Option<u32> {
        match self.host.op(NfsOp::UidOfName { name: name.into() }) {
            NfsReply::Id { id } => id,
            _ => None,
        }
    }
    fn group_name(&self, gid: u32) -> Option<String> {
        match self.host.op(NfsOp::NameOfGid { gid }) {
            NfsReply::Name { name } => name,
            _ => None,
        }
    }
    fn group_id(&self, name: &str) -> Option<u32> {
        match self.host.op(NfsOp::GidOfName { name: name.into() }) {
            NfsReply::Id { id } => id,
            _ => None,
        }
    }
    fn mapping(&self) -> bool {
        self.mapping_now().0
    }
}

/// The registry a server job's threads share.
#[must_use]
pub fn registry<H: Host>(host: &Arc<H>) -> Arc<Registry> {
    Arc::new(Registry::new(
        Arc::new(JobIdMap::new(Arc::clone(host))),
        TTL,
        Arc::new(SystemClock),
    ))
}
