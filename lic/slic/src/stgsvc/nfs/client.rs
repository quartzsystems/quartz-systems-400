// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The NFS client in the storage service: `nfs::client::Factory`
//! registered as the IFS's maker of `*NFS` mounts. A mount runs on its
//! own thread, away from the service's: what it needs of the system
//! (the host table and this system's name, whether this system's
//! status monitor and lock manager run) is a snapshot [`refresh`] takes
//! on the service's thread before each `MOUNT`. `MOUNT`'s options and
//! `MFS` are checked here first ([`check`]) so a bad one gets its own
//! message. The server's reports are the IFS's (`CPDA1B9` from
//! `ifsremote`), timed by the client's major timeout.

use std::net::IpAddr;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ifs::remote::{CcsidSel, MountCcsid};
use ifscmd::Msg;
use nfs::client::{ClientEnv, Factory, Resolver, SystemResolver};
use nfs::rpc::ClientConfig;
use nfs::server::locate::{BinderLocator, Locator};
use stg::nfs::Daemon;
use tcpcfg::resolver::HostConfig;

use crate::ipl::system::System;

/// What a mount takes from the system.
#[derive(Default)]
struct Snapshot {
    hosts: Option<HostConfig>,
    lockd: bool,
}

static SNAP: Mutex<Snapshot> = Mutex::new(Snapshot {
    hosts: None,
    lockd: false,
});

fn snap() -> std::sync::MutexGuard<'static, Snapshot> {
    SNAP.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Names through the TCP/IP host table, then the system's resolver.
struct HostTable(Option<HostConfig>);

impl Resolver for HostTable {
    fn resolve(&self, host: &str) -> Vec<IpAddr> {
        if let Some(h) = &self.0 {
            let found = h.lookup(host);
            if !found.is_empty() {
                return found;
            }
        }
        SystemResolver.resolve(host)
    }
}

/// The environment of a mount made now.
fn env() -> ClientEnv {
    let s = snap();
    let host_name = s
        .hosts
        .as_ref()
        .map(|h| match (h.host_name.as_str(), h.dmn_name.as_str()) {
            ("", _) => String::new(),
            (n, "") => n.to_string(),
            (n, d) => format!("{n}.{d}"),
        })
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "localhost".into());
    let local: Option<Arc<dyn Locator>> = s.lockd.then(|| {
        Arc::new(BinderLocator {
            cfg: ClientConfig {
                timeout: Duration::from_secs(2),
                retransmits: 2,
                ..ClientConfig::default()
            },
        }) as Arc<dyn Locator>
    });
    ClientEnv {
        resolver: Arc::new(HostTable(s.hosts.clone())),
        host_name: host_name.to_ascii_lowercase(),
        local,
        connections: super::super::ifsremote::THREADS * 2,
        ..ClientEnv::default()
    }
}

/// Register the client with the IFS (at the service's start).
pub fn register() {
    super::super::ifsremote::set_factory(Some(Arc::new(Factory::new(Box::new(env)))));
}

/// Take what the next mounts need of the system: the host table and
/// whether `QNFSNSMD` and `QNFSNLMD` are ready (else locks are
/// `ENOLCK`).
pub fn refresh(system: &mut System) {
    let hosts = super::super::tcp::with(system, |_, rt| HostConfig::from_config(&rt.cfg)).ok();
    let lockd = {
        let st = super::state();
        st.registry.ready(Daemon::Nsm) > 0 && st.registry.ready(Daemon::Nlm) > 0
    };
    let mut s = snap();
    s.hosts = hosts;
    s.lockd = lockd;
}

/// `MOUNT TYPE(*NFS)`'s own checks before the IFS is asked: the
/// options (`CPFA1B0`, `CPFA1B1`, `CPFA1BB`), `MFS` (`CPFA1C6`) and a
/// double-byte data CCSID (`CPDA1CA`).
///
/// # Errors
///
/// The message of the first that fails.
pub fn check(options: &str, mfs: &str, ccsid: MountCcsid) -> Result<(), Msg> {
    nfs::exports::mount::parse(options).map_err(|e| super::export::opt_msg(&e))?;
    nfs::exports::mount::parse_mfs(mfs).map_err(|_| Msg::new("CPFA1C6"))?;
    if let CcsidSel::Ccsid(n) = ccsid.data
        && nfs::client::double_byte(n)
    {
        return Err(Msg::new("CPDA1CA").bin4(i64::from(n)).chars("*SBCS", 9));
    }
    Ok(())
}

/// A server this system mounts restarted (`QNFSNLMD` heard it from this
/// system's status monitor): the mounts of it reclaim their locks.
pub fn restarted(name: &str, state: i32) -> usize {
    nfs::client::restart::server_restarted(name.as_bytes(), state, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mount_checks() {
        let c = MountCcsid::default();
        assert!(check("*DFT", "srv:/x", c).is_ok());
        assert_eq!(check("rw,bg", "srv:/x", c).unwrap_err().id, "CPFA1B0");
        assert_eq!(check("ro,rw", "srv:/x", c).unwrap_err().id, "CPFA1B1");
        assert_eq!(check("vers=2", "srv:/x", c).unwrap_err().id, "CPFA1BB");
        assert_eq!(check("", "/dev/x", c).unwrap_err().id, "CPFA1C6");
        let dbcs = MountCcsid {
            data: CcsidSel::Ccsid(1200),
            ..c
        };
        assert_eq!(check("", "srv:/x", dbcs).unwrap_err().id, "CPDA1CA");
    }

    #[test]
    fn names_from_the_host_table() {
        let h = HostConfig {
            hosts: vec![("10.1.2.3".parse().unwrap(), vec!["NFSSRV".into()])],
            ..HostConfig::default()
        };
        assert_eq!(
            HostTable(Some(h)).resolve("nfssrv"),
            vec!["10.1.2.3".parse::<IpAddr>().unwrap()]
        );
    }
}
