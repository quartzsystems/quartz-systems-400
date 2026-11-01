// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! TCP/IP: the configuration `CFGTCP` shows, interfaces, routes, the
//! host table and the TCP servers. Every change is a CL command run as
//! the user through [`Net::run`].

use qsweb::msg::Message;

/// The TCP/IP attributes and domain (`CFGTCP` options 12 and 3,
/// `NETSTAT`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TcpConfig {
    /// `HOSTNAME`.
    pub host: String,
    /// `DMNNAME`.
    pub domain: String,
    /// `DMNSCHLIST`, blank-separated.
    pub search: String,
    /// `HOSTSCHPTY`: `*LOCAL` or `*REMOTE`.
    pub priority: String,
    /// `INTNETADR`: up to three name servers.
    pub dns: Vec<String>,
    /// `CHGTCPA IPDTGFWD(*YES)`.
    pub forwarding: bool,
    /// IPv6 interfaces active; `None` when IPv6 is not enabled.
    pub ipv6: Option<u32>,
    /// When TCP/IP started (seconds since 1970); `None` when ended.
    pub active_since: Option<u64>,
}

/// An interface (`CFGTCP` option 1, `NETSTAT` option 1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Interface {
    /// `10.0.10.20`, `2001:db8:10::20/64`.
    pub address: String,
    /// `255.255.255.0`, blank for IPv6.
    pub mask: String,
    /// `ETHLIN01`, `*LOOPBACK`.
    pub line: String,
    /// `*ELAN`, `*NONE`.
    pub line_type: String,
    /// Started.
    pub active: bool,
    /// `AUTOSTART(*YES)`.
    pub autostart: bool,
    /// `MTU`.
    pub mtu: u32,
    /// `ALIASNAME`.
    pub alias: String,
    /// The system's own (the loopback, a generated IPv6 address): not
    /// removed by `RMVTCPIFC`.
    pub generated: bool,
}

/// A route (`CFGTCP` option 2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Route {
    /// `*DFTROUTE` or a network.
    pub dest: String,
    /// `*NONE` or a mask.
    pub mask: String,
    /// The next hop.
    pub next_hop: String,
    /// The preferred interface's address.
    pub interface: String,
    /// `Static`, `Dynamic`.
    pub kind: String,
    /// `METRIC`.
    pub metric: u32,
    /// Usable now (its interface is active).
    pub active: bool,
}

/// A host table entry (`CFGTCP` option 10).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostEntry {
    /// The address.
    pub address: String,
    /// Its host names.
    pub names: Vec<String>,
}

/// A TCP server (`STRTCPSVR` special value).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Server {
    /// `*HTTP`.
    pub name: String,
    /// The description.
    pub text: String,
    /// Started.
    pub started: bool,
    /// Its autostart attribute.
    pub autostart: bool,
    /// The command that changes the autostart attribute (`CHGTELNA`),
    /// blank when there is none.
    pub attr_cmd: String,
    /// The ports it listens on.
    pub ports: Vec<u16>,
}

/// What the network pages need. `user` is the signed-on user.
pub trait Net: Send + Sync {
    /// The TCP/IP attributes and domain.
    fn config(&self, user: &str) -> TcpConfig;
    /// The interfaces.
    fn interfaces(&self, user: &str) -> Vec<Interface>;
    /// The routes.
    fn routes(&self, user: &str) -> Vec<Route>;
    /// The host table.
    fn hosts(&self, user: &str) -> Vec<HostEntry>;
    /// The TCP servers.
    fn servers(&self, user: &str) -> Vec<Server>;
    /// Run CL command `command` as `user`: its completion message, or
    /// the message it failed with.
    fn run(&self, user: &str, command: &str) -> Result<Message, Message>;
}
