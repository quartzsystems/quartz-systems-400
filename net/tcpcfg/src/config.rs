// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The whole configuration, as the storage service loads it from the
//! `QUSRSYS` files.

use crate::model::attr::Attributes;
use crate::model::dmn::Domain;
use crate::model::host::HostTable;
use crate::model::ifc::{Interface, Interfaces};
use crate::model::port::PortRestrictions;
use crate::model::rte::Routes;
use crate::model::svr::Servers;
use crate::model::tables::{NetworkEntry, ProtocolEntry, ServicesTable, shipped_protocols};

/// Everything TCP/IP configuration holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Interfaces (`QATOCIFC`, `QATOCIFC6`).
    pub ifcs: Interfaces,
    /// Routes (`QATOCRTE`, `QATOCRTE6`).
    pub rtes: Routes,
    /// The host table (`QATOCHOST`, `QATOCHST6`).
    pub hosts: HostTable,
    /// Domain information (`QATOCTCPIP`).
    pub domain: Domain,
    /// `CHGTCPA` attributes (`QATOCTCPIP`).
    pub attrs: Attributes,
    /// Port restrictions (`QATOCPORT`).
    pub ports: PortRestrictions,
    /// Servers (`QATOCSTART`, `QATOCSVRA`).
    pub servers: Servers,
    /// Services (`QATOCPS`).
    pub services: ServicesTable,
    /// Protocols (`QATOCPP`).
    pub protocols: Vec<ProtocolEntry>,
    /// Networks (`QATOCPN`).
    pub networks: Vec<NetworkEntry>,
}

impl Default for Config {
    /// A new system's configuration: the loopback interface, the
    /// loopback host names, the shipped tables.
    fn default() -> Self {
        Self {
            ifcs: Interfaces {
                list: vec![Interface::loopback()],
            },
            rtes: Routes::default(),
            hosts: HostTable::shipped(),
            domain: Domain::default(),
            attrs: Attributes::default(),
            ports: PortRestrictions::default(),
            servers: Servers::default(),
            services: ServicesTable::shipped(),
            protocols: shipped_protocols(),
            networks: Vec::new(),
        }
    }
}
