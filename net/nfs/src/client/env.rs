// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What a mount needs of the system around it: names to addresses, this
//! system's name, where the server's binder and this system's status
//! monitor listen, and the hooks that report the server.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;

use crate::server::locate::Locator;

/// Host names to addresses.
pub trait Resolver: Send + Sync {
    /// The addresses of `host` (a name; an address is taken as it is
    /// before this is asked).
    fn resolve(&self, host: &str) -> Vec<IpAddr>;
}

/// The host system's resolver (`getaddrinfo`).
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemResolver;

impl Resolver for SystemResolver {
    fn resolve(&self, host: &str) -> Vec<IpAddr> {
        (host, 0)
            .to_socket_addrs()
            .map(|it| it.map(|a| a.ip()).collect())
            .unwrap_or_default()
    }
}

/// The addresses of `host`: an address as written (brackets allowed),
/// else through `r`.
#[must_use]
pub fn addresses(r: &dyn Resolver, host: &str) -> Vec<IpAddr> {
    let h = host.trim().trim_matches(['[', ']']);
    if let Ok(ip) = h.parse::<IpAddr>() {
        return vec![ip];
    }
    r.resolve(h)
}

/// Called when the server stops answering (once per outage) and when it
/// answers again.
pub type Hook = Arc<dyn Fn() + Send + Sync>;

/// What reports the server.
#[derive(Clone, Default)]
pub struct Hooks {
    /// A hard mount's call passed its major timeout: the server is not
    /// responding (still trying).
    pub not_responding: Option<Hook>,
    /// A call was answered after that.
    pub responding: Option<Hook>,
}

impl std::fmt::Debug for Hooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Hooks")
            .field("not_responding", &self.not_responding.is_some())
            .field("responding", &self.responding.is_some())
            .finish()
    }
}

/// The system a mount is made from.
#[derive(Clone)]
pub struct ClientEnv {
    /// Host names.
    pub resolver: Arc<dyn Resolver>,
    /// This system's name: NLM's caller name, the `AUTH_SYS` machine
    /// name, the status monitor's `my_name`.
    pub host_name: String,
    /// The port of the server's binder (111).
    pub binder_port: u16,
    /// Where this system's status monitor (`QNFSNSMD`) listens, asked
    /// for `127.0.0.1`; `None` when it does not run (locks: `ENOLCK`).
    pub local: Option<Arc<dyn Locator>>,
    /// Send from a reserved port when one can be had.
    pub reserved_port: bool,
    /// The connections a mount keeps to one server program at most (the
    /// IFS's worker threads call at once).
    pub connections: usize,
    /// The server's reports.
    pub hooks: Hooks,
}

impl std::fmt::Debug for ClientEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientEnv")
            .field("host_name", &self.host_name)
            .field("binder_port", &self.binder_port)
            .field("local", &self.local.is_some())
            .field("connections", &self.connections)
            .finish_non_exhaustive()
    }
}

impl Default for ClientEnv {
    fn default() -> Self {
        Self {
            resolver: Arc::new(SystemResolver),
            host_name: "localhost".into(),
            binder_port: 111,
            local: None,
            reserved_port: true,
            connections: 8,
            hooks: Hooks::default(),
        }
    }
}

impl ClientEnv {
    /// The server's binder.
    #[must_use]
    pub fn binder(&self, ip: IpAddr) -> SocketAddr {
        SocketAddr::new(ip, self.binder_port)
    }
}
