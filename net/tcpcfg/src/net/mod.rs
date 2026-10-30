// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The kernel operations the engine calls. [`NetOps`] is all the engine
//! knows of the kernel: [`kernel::Kernel`] answers it over netlink,
//! sysctl and nftables on Linux, [`fake::FakeNet`] in tests.

pub mod fake;
pub mod kernel;

use std::net::IpAddr;

use crate::model::Proto;

/// A kernel link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkInfo {
    /// Interface index.
    pub index: u32,
    /// Name (`eth0`, `lo`).
    pub name: String,
    /// Administratively up.
    pub up: bool,
    /// Carrier, when the driver reports it.
    pub carrier: Option<bool>,
    /// MTU.
    pub mtu: u32,
}

/// An address on a link.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KAddr {
    /// The link's index.
    pub link: u32,
    /// The address.
    pub addr: IpAddr,
    /// Its prefix length.
    pub prefix: u8,
}

/// A route in the main table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KRoute {
    /// Destination; `None` is a default route.
    pub dst: Option<IpAddr>,
    /// Destination prefix length.
    pub prefix: u8,
    /// Next hop.
    pub gateway: Option<IpAddr>,
    /// Output link.
    pub link: Option<u32>,
    /// Path MTU.
    pub mtu: Option<u32>,
    /// TOS bits.
    pub tos: u8,
    /// Metric.
    pub metric: Option<u32>,
    /// Installed by QS/400 (`RTPROT_QS400`).
    pub ours: bool,
}

/// A kernel refusal.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{text}")]
pub struct NetError {
    /// What went wrong.
    pub text: String,
    /// The thing was already there (`EEXIST`).
    pub exists: bool,
    /// The thing was not there (`ENOENT`, `ESRCH`, `ENODEV`).
    pub not_found: bool,
}

impl NetError {
    /// A plain failure.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            exists: false,
            not_found: false,
        }
    }
}

/// A result of a kernel operation.
pub type NetResult<T> = std::result::Result<T, NetError>;

/// What the engine asks of the kernel.
pub trait NetOps {
    /// Every link.
    fn links(&mut self) -> NetResult<Vec<LinkInfo>>;
    /// Bring a link up or down.
    fn set_up(&mut self, index: u32, up: bool) -> NetResult<()>;
    /// Set a link's MTU.
    fn set_mtu(&mut self, index: u32, mtu: u32) -> NetResult<()>;
    /// Create a `dummy` link (virtual IP addresses live on one).
    fn create_dummy(&mut self, name: &str) -> NetResult<LinkInfo>;
    /// Create an 802.1Q link on `parent`.
    fn create_vlan(&mut self, name: &str, parent: u32, id: u16) -> NetResult<LinkInfo>;
    /// Delete a link.
    fn delete_link(&mut self, index: u32) -> NetResult<()>;
    /// Every address.
    fn addresses(&mut self) -> NetResult<Vec<KAddr>>;
    /// Add an address.
    fn add_address(&mut self, a: &KAddr) -> NetResult<()>;
    /// Remove an address.
    fn del_address(&mut self, a: &KAddr) -> NetResult<()>;
    /// The main table's routes.
    fn routes(&mut self) -> NetResult<Vec<KRoute>>;
    /// Add a route.
    fn add_route(&mut self, r: &KRoute) -> NetResult<()>;
    /// Remove a route.
    fn del_route(&mut self, r: &KRoute) -> NetResult<()>;
    /// A `/proc/sys/net` value.
    fn sysctl_read(&mut self, key: &str) -> NetResult<String>;
    /// Write a `/proc/sys/net` value.
    fn sysctl_write(&mut self, key: &str, value: &str) -> NetResult<()>;
    /// Replace the backstop's port rules with `rules`.
    fn set_port_rules(&mut self, rules: &[(Proto, u16, u16)]) -> NetResult<()>;
    /// Set the kernel's host name.
    fn set_hostname(&mut self, name: &str) -> NetResult<()>;

    /// A link by name.
    fn link(&mut self, name: &str) -> NetResult<Option<LinkInfo>> {
        Ok(self.links()?.into_iter().find(|l| l.name == name))
    }
}
