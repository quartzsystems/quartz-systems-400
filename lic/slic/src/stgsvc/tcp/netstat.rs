// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `NETSTAT`'s data in the storage service: the configured interfaces
//! with their status and the addresses the kernel has beyond them, the
//! kernel's routes, every TCP and UDP socket from `sock_diag` with the
//! job holding it (`/proc/<pid>/fd` to the job table), and the stack
//! status. `nettools::netstat` turns them into the `QTOCNETS` panels'
//! variables; option 4 on a connection ends it with `SOCK_DESTROY`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use netlink::diag::{Diag, InetSock, Proto};
use netstat::{Conn, Counters, Ifc, Rte, Stack};
use nettools::netstat;
use stg::tcp::PanelData;
use tcpcfg::addr::IfcAddr;
use tcpcfg::msg::Note;
use tcpcfg::{Config, State};

use crate::ipl::system::{System, now_unix};

/// Who last started and ended TCP/IP, and when it ended.
#[derive(Default)]
struct History {
    starter: (String, String, String),
    ender: (String, String, String),
    ended: u64,
}

thread_local! {
    static HISTORY: RefCell<History> = RefCell::new(History::default());
}

fn job_of(system: &System) -> (String, String, String) {
    let (n, u, nbr) = crate::stgsvc::msgs::sender(system, system.current_job);
    (n, u, format!("{nbr:06}"))
}

/// `STRTCP` ran in the current job.
pub fn started(system: &System) {
    let who = job_of(system);
    HISTORY.with(|h| h.borrow_mut().starter = who);
}

/// `ENDTCP` ran in the current job.
pub fn ended(system: &System) {
    let who = job_of(system);
    HISTORY.with(|h| {
        let mut h = h.borrow_mut();
        h.ender = who;
        h.ended = now_unix();
    });
}

/// The UTC offset of `QTIMZON` at a time.
fn offset(system: &System, t: u64) -> i64 {
    system
        .sysvals
        .as_ref()
        .and_then(|v| v.get("QTIMZON").ok())
        .and_then(|v| sysval::tz::find(v.text().trim()))
        .map_or(0, |z| i64::from(z.offset_at(t)))
}

/// A Unix time as the displays show it: `MM/DD/YY  HH:MM:SS`.
fn when(system: &System, t: u64) -> String {
    if t == 0 {
        return String::new();
    }
    let (d, time) = nettools::cmntrc::report::civil(t * 1_000_000, offset(system, t));
    format!("{d}  {}", &time[..8])
}

/// The line a kernel link is: the line description varied on over it,
/// `*LOOPBACK` for `lo`, else the link's name.
fn line_of(ifname: &str) -> String {
    if ifname == "lo" {
        return "*LOOPBACK".into();
    }
    let lines = crate::stgsvc::cmn::lock();
    lines
        .state
        .lines()
        .find(|l| l.ifname == ifname)
        .map_or_else(|| ifname.to_ascii_uppercase(), |l| l.name.clone())
}

/// The kernel's links by index: name, MTU, up.
fn links() -> HashMap<u32, (String, u32, bool)> {
    netlink::route::Rtnl::open()
        .and_then(|mut r| r.links())
        .map(|v| v.into_iter().map(|l| (l.index, (l.name.clone(), l.mtu, l.is_up()))).collect())
        .unwrap_or_default()
}

fn interfaces(system: &System, cfg: &Config, st: &State) -> Vec<Ifc> {
    let links = links();
    let kernel: Vec<netlink::route::Address> = netlink::route::Rtnl::open()
        .and_then(|mut r| r.addresses(None))
        .unwrap_or_default();
    let host = |a: IpAddr| -> String {
        cfg.hosts
            .list
            .iter()
            .find(|e| e.addr == a)
            .and_then(|e| e.names.first().cloned())
            .unwrap_or_default()
    };
    let mut out = Vec::new();
    for i in &cfg.ifcs.list {
        let run = st.ifcs.get(&i.key());
        let addr = match i.addr {
            IfcAddr::Ip(a) => Some(a),
            _ => None,
        };
        let prefix = match (addr, i.mask, i.prefix_len) {
            (Some(IpAddr::V4(_)), Some(m), _) => tcpcfg::addr::mask_prefix(m).unwrap_or(32),
            (Some(IpAddr::V6(_)), _, Some(p)) => p,
            (Some(IpAddr::V6(_)), _, None) => 64,
            _ => 0,
        };
        let link_mtu = run
            .and_then(|r| r.link.as_ref())
            .and_then(|n| links.values().find(|(name, _, _)| name == n))
            .map_or(1500, |l| l.1);
        out.push(Ifc {
            addr,
            special: if addr.is_none() { i.key() } else { String::new() },
            prefix,
            line: i.line.name().to_string(),
            line_type: i.line_type().to_string(),
            status: st.ifc_status(&i.key()).name().to_string(),
            alias: i.alias.clone().unwrap_or_default(),
            host: addr.map(host).unwrap_or_default(),
            mtu: i.mtu.unwrap_or(if addr.is_some_and(|a| a.is_loopback()) { 576 } else { link_mtu }),
            autostart: i.autostart,
            dhcp: matches!(i.addr, IfcAddr::Dhcp4),
            vlan: i.vlan,
            local_ifc: i.local_ifc.map(|l| l.to_string()).unwrap_or_default(),
            tos: i.tos.name().to_string(),
            text: i.text.clone(),
            changed: run.map_or_else(String::new, |r| when(system, r.since)),
        });
    }
    // The addresses the kernel has that TCP/IP did not configure (the
    // IPL's own address, link-local IPv6 addresses).
    for a in kernel {
        if out.iter().any(|i| i.addr == Some(a.addr)) {
            continue;
        }
        let (name, mtu, up) = links.get(&a.index).cloned().unwrap_or_default();
        out.push(Ifc {
            addr: Some(a.addr),
            prefix: a.prefix,
            line: line_of(&name),
            line_type: if name == "lo" { "*NONE".into() } else { "*ELAN".into() },
            status: if up { "Active".into() } else { "Inactive".into() },
            host: host(a.addr),
            mtu,
            autostart: false,
            ..Ifc::default()
        });
    }
    out
}

fn routes(cfg: &Config, ifcs: &[Ifc]) -> Vec<Rte> {
    let links = links();
    let kernel: Vec<netlink::route::Route> = netlink::route::Rtnl::open()
        .and_then(|mut r| r.routes(None))
        .unwrap_or_default();
    let mut out: Vec<Rte> = Vec::new();
    for r in kernel {
        // The main table's unicast routes.
        if r.table != netlink::route::consts::RT_TABLE_MAIN || r.kind != 1 {
            continue;
        }
        let link = r.oif.and_then(|i| links.get(&i));
        let configured = cfg.rtes.list.iter().find(|c| {
            c.next_hop == r.gateway.unwrap_or(c.next_hop) && r.gateway.is_some() && c.prefix == r.dst_len
        });
        let ifc = r.prefsrc.or_else(|| {
            let name = &link?.0;
            ifcs.iter().find(|i| line_of(name) == i.line && i.addr.is_some_and(|a| a.is_ipv6() == r.dst.or(r.gateway).is_some_and(|d| d.is_ipv6()))).and_then(|i| i.addr)
        });
        out.push(Rte {
            dest: if r.dst_len == 0 { None } else { r.dst },
            prefix: r.dst_len,
            hop: r.gateway,
            mtu: r.mtu.or(link.map(|l| l.1)).unwrap_or(1500),
            available: link.is_none_or(|l| l.2),
            configured: configured.is_some(),
            tos: configured.map_or_else(|| "*NORMAL".into(), |c| c.tos.name().to_string()),
            dup_pty: configured.map_or(5, |c| c.dup_pty),
            text: configured.map(|c| c.text.clone()).unwrap_or_default(),
            ifc,
            order: 1,
        });
    }
    // The loopback network lives in the kernel's local table; the
    // system lists it with the others.
    let lo: IpAddr = Ipv4Addr::new(127, 0, 0, 0).into();
    if ifcs.iter().any(|i| i.addr.is_some_and(|a| a.is_loopback() && a.is_ipv4())) && !out.iter().any(|r| r.dest == Some(lo)) {
        out.push(Rte {
            dest: Some(lo),
            prefix: 8,
            mtu: 576,
            available: true,
            tos: "*NORMAL".into(),
            dup_pty: 5,
            order: 1,
            ..Rte::default()
        });
    }
    out
}

/// IBM's state name, or `*UDP`.
fn state(s: &InetSock) -> String {
    let n = match s.state {
        netlink::diag::TcpState::Established => 1,
        netlink::diag::TcpState::SynSent => 2,
        netlink::diag::TcpState::SynRecv | netlink::diag::TcpState::NewSynRecv => 3,
        netlink::diag::TcpState::FinWait1 => 4,
        netlink::diag::TcpState::FinWait2 => 5,
        netlink::diag::TcpState::TimeWait => 6,
        netlink::diag::TcpState::CloseWait => 8,
        netlink::diag::TcpState::LastAck => 9,
        netlink::diag::TcpState::Listen => 10,
        netlink::diag::TcpState::Closing => 11,
        _ => 7,
    };
    netstat::state_name(n).to_string()
}

fn sockets() -> Vec<InetSock> {
    Diag::open().and_then(|mut d| d.all()).unwrap_or_default()
}

fn conns(system: &System, socks: &[InetSock]) -> Vec<Conn> {
    let owners = netlink::procnet::socket_owners();
    let jobs = system.jobs.all();
    socks
        .iter()
        .map(|s| {
            let job = owners
                .get(&u64::from(s.inode))
                .and_then(|pid| jobs.iter().find(|j| j.pid == *pid));
            let user = job.map_or_else(|| "QSYS".to_string(), |j| j.user.clone());
            let info = s.info.as_ref();
            let idle = info.map_or(0, |i| u64::from(i.last_data_recv.min(i.last_data_sent)));
            Conn {
                tcp: s.proto == Proto::Tcp,
                local: s.id.local,
                remote: s.id.remote,
                state: state(s),
                idle_ms: if s.state == netlink::diag::TcpState::Listen { 0 } else { idle },
                user: user.clone(),
                jobs: job
                    .map(|j| {
                        vec![(
                            j.name.clone(),
                            j.user.clone(),
                            format!("{:06}", j.number),
                            match j.kind {
                                'I' => "*INT",
                                'S' => "*SYS",
                                'X' => "*SBS",
                                _ => "*BCH",
                            }
                            .to_string(),
                            j.user.clone(),
                        )]
                    })
                    .unwrap_or_default(),
                counters: Counters {
                    bytes_out: info.map_or(0, |i| i.bytes_acked),
                    bytes_in: info.map_or(0, |i| i.bytes_received),
                    out_buffered: u64::from(s.wqueue),
                    in_buffered: u64::from(s.rqueue),
                    rtt_us: info.map_or(0, |i| i.rtt),
                    rttvar_us: info.map_or(0, |i| i.rttvar),
                    retrans: info.map_or(0, |i| i.total_retrans),
                    cur_retrans: u32::from(s.retrans),
                    cwnd: info.map_or(0, |i| u64::from(i.snd_cwnd) * u64::from(i.snd_mss)),
                    mss: info.map_or(0, |i| i.snd_mss),
                },
                passive: s.state == netlink::diag::TcpState::Listen || s.id.local.port() < 1024,
            }
        })
        .collect()
}

/// The service name of a port, from the services table.
fn port_namer(cfg: &Config) -> impl Fn(u16, bool) -> Option<String> + '_ {
    move |p, tcp| {
        let proto = if tcp { "tcp" } else { "udp" };
        cfg.services
            .list
            .iter()
            .find(|e| e.port == p && e.proto.eq_ignore_ascii_case(proto))
            .map(|e| e.name.clone())
    }
}

fn find_conn<'a>(list: &'a [Conn], key: &str) -> Option<&'a Conn> {
    list.iter().find(|c| c.key() == key)
}

/// The panel data `what` names: `IFC`, `IFC6`, `IFC5 addr`, `RTE`,
/// `RTE6`, `RTE5 key`, `RTEOF addr` (option 8 on an interface), `CNN`,
/// `CNN6`, `CNN5 key`, `CNN8 key`, `STS`.
pub fn panel(system: &mut System, what: &str, args: &[String]) -> Result<PanelData, String> {
    let (cfg, st) = super::with(system, |_, rt| (rt.cfg.clone(), rt.st.clone()))?;
    let arg = args.first().map_or("", String::as_str);
    let none = || format!("The entry was not found: {arg}");
    let list = |name: &str, rows: Vec<netstat::Row>| (Vec::new(), vec![(name.to_string(), rows)]);
    Ok(match what {
        "IFC" | "IFC6" => {
            let v6 = what == "IFC6";
            let rows = netstat::ifcs(&interfaces(system, &cfg, &st), v6);
            list(if v6 { "IFC6S" } else { "IFCS" }, rename(rows, v6, "I", "X"))
        }
        "IFC5" => {
            let all = interfaces(system, &cfg, &st);
            let i = all.iter().find(|i| i.key() == arg).ok_or_else(none)?;
            (netstat::ifc_detail(i), Vec::new())
        }
        "RTE" | "RTE6" | "RTEOF" => {
            let ifcs = interfaces(system, &cfg, &st);
            let mut all = routes(&cfg, &ifcs);
            let v6 = what == "RTE6" || (what == "RTEOF" && arg.contains(':'));
            if what == "RTEOF" {
                let a: Option<IpAddr> = arg.parse().ok();
                all.retain(|r| r.ifc == a || a.is_some_and(|a| r.dest.is_some_and(|d| same_net(d, a, r.prefix))));
            }
            let rows = netstat::rtes(&all, v6)
                .into_iter()
                .map(|mut r| {
                    for (k, v) in &mut r {
                        if k == "RAVAIL" {
                            *v = format!("  {v}");
                        }
                    }
                    r
                })
                .collect();
            list(if v6 { "RTE6S" } else { "RTES" }, rename(rows, v6, "R", "Y"))
        }
        "RTE5" => {
            let ifcs = interfaces(system, &cfg, &st);
            let all = routes(&cfg, &ifcs);
            let r = all.iter().find(|r| r.key() == arg).ok_or_else(none)?;
            (netstat::rte_detail(r), Vec::new())
        }
        "CNN" | "CNN6" => {
            let v6 = what == "CNN6";
            let all = conns(system, &sockets());
            let rows = netstat::conns(&all, v6, &port_namer(&cfg));
            list(if v6 { "CNN6S" } else { "CNNS" }, rename(rows, v6, "C", "Z"))
        }
        "CNN5" => {
            let all = conns(system, &sockets());
            let c = find_conn(&all, arg).ok_or_else(none)?;
            let names = |a: IpAddr| {
                if a.is_loopback() {
                    Some("LOOPBACK".to_string())
                } else {
                    cfg.hosts.list.iter().find(|e| e.addr == a).and_then(|e| e.names.first().cloned())
                }
            };
            let last_seen = now_unix().saturating_sub(c.idle_ms / 1000);
            (netstat::conn_detail(c, &names, &port_namer(&cfg), &when(system, last_seen)), Vec::new())
        }
        "CNN8" => {
            let all = conns(system, &sockets());
            netstat::conn_jobs(find_conn(&all, arg).ok_or_else(none)?)
        }
        "STS" => {
            let active = st.tcp == tcpcfg::state::TcpStatus::Active;
            let (who_started, who_ended, end_time) = HISTORY.with(|h| {
                let h = h.borrow();
                (h.starter.clone(), h.ender.clone(), h.ended)
            });
            let s = Stack {
                v4: active,
                v6: active,
                active_secs: if active { now_unix().saturating_sub(st.started) } else { 0 },
                started: when(system, st.started),
                ended: when(system, end_time),
                starter: who_started,
                ender: who_ended,
            };
            (netstat::status(&s), Vec::new())
        }
        other => return Err(format!("NETSTAT has no view {other}")),
    })
}

/// The IPv6 lists' variables have their own first letter.
fn rename(rows: Vec<netstat::Row>, v6: bool, from: &str, to: &str) -> Vec<netstat::Row> {
    if !v6 {
        return rows;
    }
    rows.into_iter()
        .map(|r| {
            r.into_iter()
                .map(|(k, v)| (k.strip_prefix(from).map_or_else(|| k.clone(), |rest| format!("{to}{rest}")), v))
                .collect()
        })
        .collect()
}

fn same_net(a: IpAddr, b: IpAddr, prefix: u8) -> bool {
    match (a, b) {
        (IpAddr::V4(a), IpAddr::V4(b)) => {
            let m = if prefix == 0 { 0 } else { u32::MAX << (32 - u32::from(prefix.min(32))) };
            u32::from(a) & m == u32::from(b) & m
        }
        (IpAddr::V6(a), IpAddr::V6(b)) => {
            let m = if prefix == 0 { 0 } else { u128::MAX << (128 - u32::from(prefix.min(128))) };
            u128::from(a) & m == u128::from(b) & m
        }
        _ => false,
    }
}

/// Option 4 on a connection: end it (`*IOSYSCFG`).
pub fn end(_system: &mut System, key: &str) -> Result<Note, Note> {
    if let Some(session) = sec::guard::current() {
        let p = session.borrow().principal.clone();
        if let Err(e) = p.require(sec::profile::SpecialAuth::IOSYSCFG, "NETSTAT option 4") {
            return Err(Note::diag("CPF2222", e.to_string()));
        }
    }
    let socks = sockets();
    let parts: Vec<&str> = key.splitn(3, '|').collect();
    let [proto, local, remote] = parts[..] else {
        return Err(Note::diag("TCP1D07", format!("The connection was not found: {key}")));
    };
    let (local, remote): (SocketAddr, SocketAddr) = match (local.parse(), remote.parse()) {
        (Ok(l), Ok(r)) => (l, r),
        _ => return Err(Note::diag("TCP1D07", format!("The connection was not found: {key}"))),
    };
    let tcp = proto == "T";
    let Some(s) = socks
        .iter()
        .find(|s| (s.proto == Proto::Tcp) == tcp && s.id.local == local && s.id.remote == remote)
    else {
        return Err(Note::diag("TCP1D07", format!("The connection was not found: {key}")));
    };
    match Diag::open().and_then(|mut d| d.destroy(s)) {
        Ok(()) => Ok(Note::info("TCP1D05", format!("The connection from {local} to {remote} was ended."))),
        Err(e) => Err(Note::diag("TCP1D07", format!("The connection could not be ended: {e}"))),
    }
}
