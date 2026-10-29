// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the resolver reads from TCP/IP configuration: the host table
//! (`ADDTCPHTE`) and the domain attributes (`CHGTCPDMN`). TCP/IP
//! configuration implements [`HostConfig`] over its objects;
//! [`StaticHostConfig`] holds fixed values.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::Duration;

/// `HOSTSCHPTY`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchPriority {
    /// `*LOCAL`: the host table first, then the name servers.
    #[default]
    Local,
    /// `*REMOTE`: the name servers first, then the host table.
    Remote,
}

/// `PROTOCOL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DnsProtocol {
    /// `*UDP`, with TCP when an answer is truncated.
    #[default]
    Udp,
    /// `*TCP` only.
    Tcp,
}

/// One host table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostEntry {
    /// `INTNETADR`.
    pub addr: IpAddr,
    /// `HOSTNAME`: the first is the entry's name, the rest aliases.
    pub names: Vec<String>,
}

/// The `CHGTCPDMN` attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomainConfig {
    /// `HOSTNAME`.
    pub host_name: String,
    /// `DMNNAME`.
    pub domain_name: String,
    /// `DMNSCHLIST`; empty is `*DFT`, the domain name alone.
    pub search_list: Vec<String>,
    /// `HOSTSCHPTY`.
    pub priority: SearchPriority,
    /// `INTNETADR`: up to three name servers.
    pub servers: Vec<IpAddr>,
    /// `PORT`.
    pub port: u16,
    /// `PROTOCOL`.
    pub protocol: DnsProtocol,
    /// `INITQRYINT`: how long the first query waits for an answer;
    /// every retry waits twice as long as the one before.
    pub initial_query_interval: Duration,
    /// `QRYTIMOUT`: the most a lookup may take in all.
    pub query_timeout: Duration,
    /// Retries after the first query, across the servers.
    pub retries: u32,
}

impl Default for DomainConfig {
    /// IBM's defaults: no domain, no servers, port 53, UDP, a 2 second
    /// first interval, 2 retries.
    fn default() -> Self {
        Self {
            host_name: String::new(),
            domain_name: String::new(),
            search_list: Vec::new(),
            priority: SearchPriority::Local,
            servers: Vec::new(),
            port: 53,
            protocol: DnsProtocol::Udp,
            initial_query_interval: Duration::from_secs(2),
            query_timeout: Duration::from_secs(30),
            retries: 2,
        }
    }
}

impl DomainConfig {
    /// The domains an unqualified name is tried in.
    #[must_use]
    pub fn domains(&self) -> Vec<String> {
        if self.search_list.is_empty() {
            if self.domain_name.is_empty() {
                Vec::new()
            } else {
                vec![self.domain_name.clone()]
            }
        } else {
            self.search_list.clone()
        }
    }
}

/// The configuration the resolver reads.
pub trait HostConfig {
    /// The domain attributes, read at each `res_init`.
    fn domain(&self) -> DomainConfig;
    /// The host table.
    fn hosts(&self) -> Vec<HostEntry>;
}

/// Fixed values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaticHostConfig {
    /// The domain attributes.
    pub domain: DomainConfig,
    /// The host table.
    pub hosts: Vec<HostEntry>,
}

impl Default for StaticHostConfig {
    /// The shipped host table: the loopback names for 127.0.0.1 and
    /// `::1`; IBM's defaults otherwise.
    fn default() -> Self {
        Self {
            domain: DomainConfig::default(),
            hosts: shipped_hosts(),
        }
    }
}

impl HostConfig for StaticHostConfig {
    fn domain(&self) -> DomainConfig {
        self.domain.clone()
    }

    fn hosts(&self) -> Vec<HostEntry> {
        self.hosts.clone()
    }
}

/// The entries a new system's host table has.
#[must_use]
pub fn shipped_hosts() -> Vec<HostEntry> {
    vec![
        HostEntry {
            addr: IpAddr::V4(Ipv4Addr::LOCALHOST),
            names: vec!["LOOPBACK".into(), "LOCALHOST".into()],
        },
        HostEntry {
            addr: IpAddr::V6(Ipv6Addr::LOCALHOST),
            names: vec!["IPV6-LOOPBACK".into(), "IPV6-LOCALHOST".into()],
        },
    ]
}
