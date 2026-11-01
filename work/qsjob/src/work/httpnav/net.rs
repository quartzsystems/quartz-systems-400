// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `navigator::env::net::Net` as the user, over the `tcp` request the
//! `CFGTCP` and `NETSTAT` panels use: the snapshot (interfaces, routes),
//! the resolver's configuration (domain, host table), `NETSTAT STS`,
//! the `CHGTCPA` attributes the kernel report shows, and the servers
//! from the registry with each one's autostart and active jobs.

use navigator::env::net::{HostEntry, Interface, Net, Route, Server, TcpConfig};
use qsweb::msg::Message;
use servers::attrs::ntp::NtpAttrs;
use servers::registry::{self, JobKind, Readiness, ServerSpec};
use stg::Client;
use stg::tcp::{ServerInfo, TcpOp, TcpReply};
use tcpcfg::state::Snapshot;

use super::Link;
use super::perf::val;
use crate::work::web::bridge::Main;

/// TCP/IP and the TCP servers.
pub struct LiveNet {
    /// The job's thread.
    pub link: Link,
}

fn tcp(m: &Main, op: TcpOp) -> TcpReply {
    m.link.borrow_mut().with(|l| l.tcp(op))
}

fn snapshot(m: &Main) -> Snapshot {
    match tcp(m, TcpOp::Snapshot) {
        TcpReply::Snapshot { snap } => snap,
        _ => Snapshot::default(),
    }
}

fn host_config(m: &Main) -> Option<tcpcfg::resolver::HostConfig> {
    match tcp(m, TcpOp::HostConfig) {
        TcpReply::HostConfig { cfg } => Some(cfg),
        _ => None,
    }
}

/// Whether `CHGTCPA IPDTGFWD(*YES)` is in effect: the configuration's
/// forwarding sysctl in the attribute report.
fn forwarding(lines: &[String]) -> bool {
    lines.iter().any(|l| {
        let w: Vec<&str> = l.split_whitespace().collect();
        matches!(w.as_slice(), ["attr", k, "want", v, ..] if k.contains("forward") && *v == "1")
    })
}

fn config(m: &Main) -> TcpConfig {
    let snap = snapshot(m);
    let mut c = TcpConfig::default();
    if let Some(h) = host_config(m) {
        c.host = h.host_name;
        c.domain = h.dmn_name;
        c.search = h.dmn_sch_list.join(" ");
        c.priority = if h.host_sch_pty_local { "*LOCAL" } else { "*REMOTE" }.into();
        c.dns = h.dns.iter().map(ToString::to_string).collect();
    }
    if let TcpReply::Lines { lines } = tcp(m, TcpOp::Report { what: "attr".into() }) {
        c.forwarding = forwarding(&lines);
    }
    let v6 = snap.ifcs.iter().filter(|i| i.addr.contains(':')).count();
    let active = snap.tcp == "Active";
    c.ipv6 = (v6 > 0).then(|| {
        let up = snap.ifcs.iter().filter(|i| i.addr.contains(':') && i.status == "Active").count();
        u32::try_from(up).unwrap_or(0)
    });
    if active
        && let TcpReply::Panel { data } = tcp(
            m,
            TcpOp::Netstat {
                what: "STS".into(),
                args: Vec::new(),
            },
        )
    {
        let secs: u64 = val(&data.0, "SACT").trim().parse().unwrap_or(0);
        let now = m.link.borrow_mut().with(Client::now).unix;
        c.active_since = Some(now.saturating_sub(secs));
    }
    c
}

fn interfaces(m: &Main) -> Vec<Interface> {
    snapshot(m)
        .ifcs
        .into_iter()
        .map(|i| {
            let v6 = i.addr.contains(':');
            let generated = i.line == "*LOOPBACK" || i.addr.to_ascii_lowercase().starts_with("fe80");
            Interface {
                address: if v6 && i.prefix > 0 && !i.addr.contains('/') {
                    format!("{}/{}", i.addr, i.prefix)
                } else {
                    i.addr
                },
                mask: if v6 { String::new() } else { i.mask },
                active: i.status == "Active",
                autostart: i.autostart,
                mtu: i.mtu.parse().unwrap_or(0),
                alias: if i.alias == "*NONE" { String::new() } else { i.alias },
                line: i.line,
                line_type: i.line_type,
                generated,
            }
        })
        .collect()
}

fn routes(m: &Main) -> Vec<Route> {
    snapshot(m)
        .rtes
        .into_iter()
        .map(|r| Route {
            dest: r.dest,
            mask: r.mask,
            next_hop: r.next_hop,
            interface: r.bind_ifc,
            kind: "Static".into(),
            metric: u32::from(r.metric),
            active: r.status == "Active",
        })
        .collect()
}

fn hosts(m: &Main) -> Vec<HostEntry> {
    host_config(m).map_or_else(Vec::new, |h| {
        h.hosts
            .into_iter()
            .map(|(a, names)| HostEntry {
                address: a.to_string(),
                names,
            })
            .collect()
    })
}

/// A server's description.
fn text(s: &ServerSpec) -> String {
    match s.special {
        "*TELNET" => "Telnet server".into(),
        "*FTP" => "FTP server".into(),
        "*LPD" => "Line printer daemon".into(),
        "*HTTP" => "HTTP administration and Navigator".into(),
        "*DDM" => "DDM/DRDA server".into(),
        "*NTP" => "SNTP client and server".into(),
        "*SSHD" => "SSH server".into(),
        "*HOSTSVR" => "Host servers".into(),
        _ => format!("{} server", s.name),
    }
}

/// Whether a server's own attributes say it autostarts (`*NTP` has two
/// halves: the client's `AUTOSTART` and the server's `SVRAUTOSTR`).
fn autostarts(info: &ServerInfo) -> bool {
    info.autostart || (info.server == "*NTP" && NtpAttrs::from_bag(&info.bag).autostarts())
}

/// The servers this release runs, with their state: started when a
/// listener job of theirs is active.
pub fn servers(m: &Main) -> Vec<Server> {
    let list = match tcp(m, TcpOp::Servers) {
        TcpReply::Servers { list } => list,
        _ => Vec::new(),
    };
    let jobs = m.link.borrow_mut().with(Client::active_jobs);
    registry::SPECS
        .iter()
        .filter(|s| s.readiness != Readiness::NotPlanned)
        .map(|s| {
            let started = jobs.iter().any(|j| {
                registry::of_job(&j.name)
                    .is_some_and(|(o, js)| o.special == s.special && js.kind == JobKind::Listener)
            });
            let autostart = list.iter().find(|i| i.server == s.special).is_some_and(autostarts);
            Server {
                name: s.special.to_string(),
                text: text(s),
                started,
                autostart,
                attr_cmd: s.attrs_cmd.unwrap_or("").to_string(),
                ports: s.ports.iter().map(|p| p.port).collect(),
            }
        })
        .collect()
}

impl Net for LiveNet {
    fn config(&self, user: &str) -> TcpConfig {
        self.link.read(user, |m| config(m))
    }

    fn interfaces(&self, user: &str) -> Vec<Interface> {
        self.link.read(user, |m| interfaces(m))
    }

    fn routes(&self, user: &str) -> Vec<Route> {
        self.link.read(user, |m| routes(m))
    }

    fn hosts(&self, user: &str) -> Vec<HostEntry> {
        self.link.read(user, |m| hosts(m))
    }

    fn servers(&self, user: &str) -> Vec<Server> {
        self.link.read(user, |m| servers(m))
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.link.run(user, command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attrs() {
        let lines = vec![
            "attr ipv4/ip_forward want 1 kernel 1".to_string(),
            "attr ipv4/tcp_keepalive_time want 7200 kernel 7200".to_string(),
        ];
        assert!(forwarding(&lines));
        assert!(!forwarding(&lines[1..]));
        let ntp = ServerInfo {
            server: "*NTP".into(),
            autostart: false,
            bag: Vec::new(),
        };
        assert!(!autostarts(&ntp));
        let http = registry::lookup("*HTTP").unwrap();
        assert!(text(http).contains("HTTP"));
    }
}
