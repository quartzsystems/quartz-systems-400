// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Where another host's (or this host's) RPC program listens: the
//! lock manager calls back a client's lock manager and the local status
//! monitor, the status monitor calls monitored hosts' monitors.
//! [`BinderLocator`] asks the host's binder; tests use [`FixedLocator`].

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::Mutex;
use std::time::Duration;

use crate::rpc::{ClientConfig, Proto};
use crate::rpcb::BinderClient;

/// A host, by address or by name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Host {
    /// An address.
    Addr(IpAddr),
    /// A name, resolved by the locator.
    Name(String),
}

/// Finds programs.
pub trait Locator: Send + Sync {
    /// Where `prog` version `vers` listens on `host` over `proto`.
    fn find(&self, host: &Host, prog: u32, vers: u32, proto: Proto) -> Option<SocketAddr>;
}

/// Asks the binder (port 111) of the host.
#[derive(Debug, Clone)]
pub struct BinderLocator {
    /// How the binder is called.
    pub cfg: ClientConfig,
}

impl Default for BinderLocator {
    fn default() -> Self {
        Self {
            cfg: ClientConfig {
                timeout: Duration::from_secs(2),
                retransmits: 2,
                ..ClientConfig::default()
            },
        }
    }
}

impl Locator for BinderLocator {
    fn find(&self, host: &Host, prog: u32, vers: u32, proto: Proto) -> Option<SocketAddr> {
        let ip = match host {
            Host::Addr(a) => *a,
            Host::Name(n) => (n.as_str(), 111).to_socket_addrs().ok()?.next()?.ip(),
        };
        let mut b = BinderClient::new(ip, Proto::Udp, self.cfg.clone());
        let protocol = match proto {
            Proto::Tcp => 6,
            Proto::Udp => 17,
        };
        let port = b.getport(prog, vers, protocol).ok()??;
        Some(SocketAddr::new(ip, port))
    }
}

/// Fixed answers: (host, program, version) to an address.
#[derive(Debug, Default)]
pub struct FixedLocator {
    map: Mutex<Vec<(Host, u32, u32, SocketAddr)>>,
}

impl FixedLocator {
    /// An empty locator.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Says `prog` `vers` on `host` is at `addr` (any transport).
    pub fn put(&self, host: Host, prog: u32, vers: u32, addr: SocketAddr) {
        let mut m = self
            .map
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        m.retain(|(h, p, v, _)| !(h == &host && *p == prog && *v == vers));
        m.push((host, prog, vers, addr));
    }
}

impl Locator for FixedLocator {
    fn find(&self, host: &Host, prog: u32, vers: u32, _proto: Proto) -> Option<SocketAddr> {
        let m = self
            .map
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        m.iter()
            .find(|(h, p, v, _)| h == host && *p == prog && *v == vers)
            .map(|e| e.3)
    }
}
