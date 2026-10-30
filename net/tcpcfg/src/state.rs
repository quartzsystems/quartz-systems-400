// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What is running: TCP/IP itself, each interface and route, the port
//! rules installed. The storage service holds one [`State`], like the
//! job table, and answers [`Snapshot`]s for the panels and `NETSTAT`.

use std::collections::BTreeMap;

use crate::config::Config;
use crate::model::Proto;
use crate::model::rte::RouteKey;

/// TCP/IP as a whole.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TcpStatus {
    /// Not started, or ended.
    #[default]
    Inactive,
    /// `STRTCP` is running.
    Starting,
    /// Started.
    Active,
    /// `ENDTCP` is running.
    Ending,
}

impl TcpStatus {
    /// The word the displays show.
    pub fn name(self) -> &'static str {
        match self {
            Self::Inactive => "Inactive",
            Self::Starting => "Starting",
            Self::Active => "Active",
            Self::Ending => "Ending",
        }
    }
}

/// An interface's status, as `NETSTAT *IFC` shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IfcStatus {
    /// Not started.
    #[default]
    Inactive,
    /// Being started.
    Starting,
    /// Started.
    Active,
    /// Being ended.
    Ending,
    /// Could not be started.
    Failed,
    /// Its line lost its carrier; it comes back with the line.
    RcyPnd,
}

impl IfcStatus {
    /// The word the displays show.
    pub fn name(self) -> &'static str {
        match self {
            Self::Inactive => "Inactive",
            Self::Starting => "Starting",
            Self::Active => "Active",
            Self::Ending => "Ending",
            Self::Failed => "Failed",
            Self::RcyPnd => "RCYPND",
        }
    }

    /// Started (active or waiting for its line).
    pub fn started(self) -> bool {
        matches!(self, Self::Active | Self::RcyPnd)
    }
}

/// A route's status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RteStatus {
    /// Not in the kernel.
    #[default]
    Inactive,
    /// In the kernel.
    Active,
    /// The kernel refused it.
    Failed,
}

impl RteStatus {
    /// The word the displays show.
    pub fn name(self) -> &'static str {
        match self {
            Self::Inactive => "Inactive",
            Self::Active => "Active",
            Self::Failed => "Failed",
        }
    }
}

/// One interface at run time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IfcRun {
    /// The status.
    pub status: IfcStatus,
    /// The kernel link it is on.
    pub link: Option<String>,
    /// A link TCP/IP made for it (a VLAN, the virtual IP link), removed
    /// when nothing uses it.
    pub made: Option<String>,
    /// The address TCP/IP added (and removes when the interface ends).
    pub installed: Option<crate::net::KAddr>,
    /// Seconds since the epoch of the last change.
    pub since: u64,
    /// The last failure.
    pub error: String,
}

/// One route at run time.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RteRun {
    /// The status.
    pub status: RteStatus,
    /// The kernel link the route went out on.
    pub link: Option<String>,
    /// The route as installed, to take it out again.
    pub installed: Option<crate::net::KRoute>,
    /// The last failure.
    pub error: String,
}

/// Everything that runs.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct State {
    /// TCP/IP.
    pub tcp: TcpStatus,
    /// Interfaces by key (the address as typed).
    pub ifcs: BTreeMap<String, IfcRun>,
    /// Routes by key.
    pub rtes: BTreeMap<RouteKey, RteRun>,
    /// The port rules installed in the backstop.
    pub rules: Vec<(Proto, u16, u16)>,
    /// When TCP/IP last started.
    pub started: u64,
}

impl State {
    /// An interface's status.
    pub fn ifc_status(&self, key: &str) -> IfcStatus {
        self.ifcs.get(key).map(|r| r.status).unwrap_or_default()
    }

    /// A route's status.
    pub fn rte_status(&self, key: &RouteKey) -> RteStatus {
        self.rtes.get(key).map(|r| r.status).unwrap_or_default()
    }

    /// The panels' and `NETSTAT`'s view of the configuration with the
    /// run-time status.
    pub fn snapshot(&self, cfg: &Config) -> Snapshot {
        let ifcs = cfg
            .ifcs
            .sorted()
            .into_iter()
            .map(|i| {
                let run = self.ifcs.get(&i.key()).cloned().unwrap_or_default();
                IfcView {
                    addr: i.key(),
                    mask: i.mask.map_or_else(
                        || if i.addr.is_v6() { String::new() } else { "*".into() },
                        |m| m.to_string(),
                    ),
                    prefix: i.prefix(),
                    line: i.line.name().to_string(),
                    vlan: i.vlan.unwrap_or(0),
                    line_type: i.line_type().into(),
                    status: run.status.name().into(),
                    alias: i.alias.clone().unwrap_or_else(|| "*NONE".into()),
                    local_ifc: i.local_ifc.map_or_else(|| "*NONE".into(), |l| l.to_string()),
                    tos: i.tos.name().into(),
                    mtu: i.mtu.map_or_else(|| "*LIND".into(), |m| m.to_string()),
                    autostart: i.autostart,
                    text: i.text.clone(),
                    link: run.link.unwrap_or_default(),
                    error: run.error,
                }
            })
            .collect();
        let rtes = cfg
            .rtes
            .sorted()
            .into_iter()
            .map(|r| {
                let run = self.rtes.get(&r.key()).cloned().unwrap_or_default();
                RteView {
                    dest: r.dest.to_string(),
                    mask: r.mask_text(),
                    next_hop: r.next_hop.to_string(),
                    bind_ifc: r.bind_ifc.map_or_else(|| "*NONE".into(), |b| b.to_string()),
                    tos: r.tos.name().into(),
                    mtu: r.mtu.map_or_else(|| "*IFC".into(), |m| m.to_string()),
                    dup_pty: r.dup_pty,
                    metric: r.metric,
                    redist: r.redist,
                    text: r.text.clone(),
                    status: run.status.name().into(),
                    link: run.link.unwrap_or_default(),
                }
            })
            .collect();
        Snapshot {
            tcp: self.tcp.name().into(),
            ifcs,
            rtes,
        }
    }
}

/// An interface as the displays show it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(missing_docs)] // the columns, named as the displays name them
pub struct IfcView {
    pub addr: String,
    pub mask: String,
    pub prefix: u8,
    pub line: String,
    pub vlan: u16,
    pub line_type: String,
    pub status: String,
    pub alias: String,
    pub local_ifc: String,
    pub tos: String,
    pub mtu: String,
    pub autostart: bool,
    pub text: String,
    /// The kernel link, empty when not started.
    pub link: String,
    /// The last failure.
    pub error: String,
}

/// A route as the displays show it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(missing_docs)] // the columns, named as the displays name them
pub struct RteView {
    pub dest: String,
    pub mask: String,
    pub next_hop: String,
    pub bind_ifc: String,
    pub tos: String,
    pub mtu: String,
    pub dup_pty: u8,
    pub metric: u8,
    pub redist: bool,
    pub text: String,
    pub status: String,
    pub link: String,
}

/// TCP/IP's status with every interface and route.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// `Active`, `Inactive`, ...
    pub tcp: String,
    /// Interfaces in the panels' order.
    pub ifcs: Vec<IfcView>,
    /// Routes in the panels' order.
    pub rtes: Vec<RteView>,
}
