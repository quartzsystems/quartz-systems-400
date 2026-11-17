// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Host names for every LDAP client in a job (`QGLDCLNT`, the QSH
//! `ldap*` tools, the publishing agents): the host table and the name
//! servers of `CFGTCP` (`nfs::names::HostNames`, as the NFS daemons
//! name hosts), which the C library's resolver knows nothing of. The
//! configuration is read on a link of its own, again after a minute.

use std::net::IpAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use nfs::exports::Resolver as _;
use stg::Client as _;

use super::super::nfs::names::HostNames;

/// How long the configuration read is used.
const KEEP: Duration = Duration::from_secs(60);

static NAMES: Mutex<Option<(Instant, Arc<HostNames>)>> = Mutex::new(None);

/// Name hosts for the LDAP clients of job `number` in this process.
pub fn install(number: u32) {
    let pid = std::process::id();
    ldap::client::set_names(Box::new(move |name| addresses(number, pid, name)));
}

fn addresses(number: u32, pid: u32, name: &str) -> Vec<IpAddr> {
    names(number, pid).map_or_else(Vec::new, |n| n.addresses(name))
}

/// The system's names, read again when the copy is a minute old.
fn names(number: u32, pid: u32) -> Option<Arc<HostNames>> {
    let mut kept = NAMES.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((at, n)) = kept.as_ref()
        && at.elapsed() < KEEP
    {
        return Some(Arc::clone(n));
    }
    let link = super::super::nfs::host::thread_link(number, pid).ok()?;
    let reply = link
        .borrow_mut()
        .with(|c| c.tcp(stg::tcp::TcpOp::HostConfig));
    let stg::tcp::TcpReply::HostConfig { cfg } = reply else {
        return None;
    };
    let n = Arc::new(HostNames::new(cfg));
    *kept = Some((Instant::now(), Arc::clone(&n)));
    Some(n)
}
