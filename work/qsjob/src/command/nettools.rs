// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The network tools in the job: `PING`/`VFYTCPCNN` and
//! `TRACEROUTE`/`TRCTCPRTE` send from the job's own process (an ICMP
//! datagram or UDP socket, `nettools::sys`), names resolved through the
//! host table and the name servers the storage service holds; `NETSTAT`
//! in a job without a display lists its views as text.

use std::fmt::Write as _;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use cmd::Resolved;
use cmd::block::Parms;
use mi::Job;
use nettools::msg::{self as tmsg, Kind, Msg};
use nettools::{netstat, ping, route};
use qir::MsgType;
use stg::Client;
use stg::tcp::{TcpOp, TcpReply};
use tcpcfg::resolver::HostConfig;

use super::{Fail, parms_of};
use crate::console::host::Host;

/// The job-site network tools; `None` for another command.
pub fn run(job: &mut Job, host: &Host, r: &Resolved) -> Option<Result<String, Fail>> {
    Some(match r.name.as_str() {
        "PING" | "VFYTCPCNN" => ping_cmd(job, host, &parms_of(r)),
        "TRACEROUTE" | "TRCTCPRTE" => trace_cmd(job, host, &parms_of(r)),
        "NETSTAT" | "WRKTCPSTS" => netstat_text(host, crate::panels::tcp::netstat_option(r)),
        _ => return None,
    })
}

/// The messages into the job log and the console's text; an escape
/// last ends the command.
fn finish(job: &mut Job, msgs: &[Msg]) -> Result<String, Fail> {
    let mut text = String::new();
    for m in msgs {
        let _ = writeln!(text, "{}", m.line());
        if m.kind == Kind::Escape {
            return Err(Fail::new(m.id, text));
        }
        let kind = match m.kind {
            Kind::Comp => MsgType::Comp,
            Kind::Diag => MsgType::Diag,
            _ => MsgType::Info,
        };
        super::log(job, m.id, &[], kind, Some(&m.text));
    }
    Ok(text)
}

fn escape(id: &'static str, text: impl Into<String>) -> Msg {
    Msg::new(id, Kind::Escape, text)
}

/// The resolver's configuration from the storage service.
fn host_config(host: &Host) -> HostConfig {
    match host.link.borrow_mut().with(|l| l.tcp(TcpOp::HostConfig)) {
        TcpReply::HostConfig { cfg } => cfg,
        _ => HostConfig::default(),
    }
}

/// `ADRVERFMT`: `Some(true)` for `*IP6`, `Some(false)` for `*IP4`.
fn version(p: &Parms) -> Option<bool> {
    match p.name("ADRVERFMT").as_str() {
        "*IP6" | "6" => Some(true),
        "*IP4" | "4" => Some(false),
        _ => None,
    }
}

fn unquote(s: &str) -> String {
    s.trim().trim_matches('\'').trim().to_string()
}

/// A name's addresses: the host table first when it says so, then the
/// name servers.
fn resolve(hc: &HostConfig, name: &str, v6: Option<bool>) -> Vec<IpAddr> {
    let fits = |a: &IpAddr| v6.is_none_or(|six| a.is_ipv6() == six);
    let table: Vec<IpAddr> = hc.lookup(name).into_iter().filter(fits).collect();
    if !table.is_empty() && (hc.host_sch_pty_local || hc.dns.is_empty()) {
        return table;
    }
    if hc.dns.is_empty() {
        return table;
    }
    let domain = sockets::resolver::DomainConfig {
        host_name: hc.host_name.clone(),
        domain_name: hc.dmn_name.clone(),
        search_list: hc.dmn_sch_list.clone(),
        servers: hc.dns.clone(),
        port: hc.port,
        protocol: if hc.tcp {
            sockets::resolver::DnsProtocol::Tcp
        } else {
            sockets::resolver::DnsProtocol::Udp
        },
        initial_query_interval: Duration::from_secs(u64::from(hc.init_qry_int.max(1))),
        query_timeout: Duration::from_secs(u64::from(hc.qry_timeout.max(1))),
        ..sockets::resolver::DomainConfig::default()
    };
    let cfg = sockets::resolver::StaticHostConfig {
        domain,
        hosts: hc
            .hosts
            .iter()
            .map(|(a, n)| sockets::resolver::HostEntry { addr: *a, names: n.clone() })
            .collect(),
    };
    let mut r = sockets::resolver::Resolver::new(Box::new(cfg), Box::new(sockets::resolver::DefaultServices));
    let family = match v6 {
        Some(true) => sockets::consts::AF_INET6,
        _ => sockets::consts::AF_INET,
    };
    match r.by_name(name, family) {
        Ok(h) => h.addrs.into_iter().filter(fits).collect(),
        Err(_) if table.is_empty() && v6.is_none() => r
            .by_name(name, sockets::consts::AF_INET6)
            .map(|h| h.addrs)
            .unwrap_or_default(),
        Err(_) => table,
    }
}

/// `RMTSYS` (and `INTNETADR` for `*INTNETADR`) as a target.
fn target(host: &Host, p: &Parms) -> Result<ping::Target, Msg> {
    let rmtsys = unquote(&p.text("RMTSYS"));
    let v6 = version(p);
    if rmtsys.len() > 255 {
        return Err(escape(tmsg::NAME_TOO_LONG, "The remote system name is longer than 255 characters."));
    }
    let typed = if rmtsys.eq_ignore_ascii_case("*INTNETADR") {
        unquote(&p.text("INTNETADR"))
    } else {
        rmtsys.clone()
    };
    if let Ok(addr) = typed.parse::<IpAddr>() {
        if v6.is_some_and(|six| addr.is_ipv6() != six) {
            return Err(escape(
                tmsg::BAD_VERSION,
                format!("{addr} is not an address of the version {} asks for.", p.name("ADRVERFMT")),
            ));
        }
        let shown = if rmtsys.eq_ignore_ascii_case("*INTNETADR") { "*INTNETADR".to_string() } else { typed };
        return Ok(ping::Target { rmtsys: shown, name: None, addr });
    }
    let hc = host_config(host);
    let found = resolve(&hc, &typed, v6);
    match found.first() {
        Some(addr) => Ok(ping::Target { rmtsys: typed.clone(), name: Some(typed.to_ascii_uppercase()), addr: *addr }),
        None => Err(escape(tmsg::UNKNOWN_HOST, format!("No address is known for host {typed}."))),
    }
}

/// `LCLINTNETA`: `*ANY` or an address.
fn local(p: &Parms) -> Result<Option<IpAddr>, Msg> {
    let t = unquote(&p.text("LCLINTNETA"));
    if t.is_empty() || t.eq_ignore_ascii_case("*ANY") {
        return Ok(None);
    }
    t.parse()
        .map(Some)
        .map_err(|_| escape(tmsg::BAD_LOCAL, format!("{t} is not a local internet address.")))
}

/// `TOS` as the IP header's byte.
fn tos(p: &Parms) -> u8 {
    match p.name("TOS").as_str() {
        "*MINDELAY" | "2" => 0x10,
        "*MAXTHRPUT" | "3" => 0x08,
        "*MAXRLB" | "4" => 0x04,
        "*MINCOST" | "5" => 0x02,
        _ => 0,
    }
}

fn int(p: &Parms, kwd: &str, dft: i64, lo: i64, hi: i64) -> i64 {
    p.int(kwd).unwrap_or(dft).clamp(lo, hi)
}

/// `PING` and `VFYTCPCNN`.
fn ping_cmd(job: &mut Job, host: &Host, p: &Parms) -> Result<String, Fail> {
    let t = match target(host, p) {
        Ok(t) => t,
        Err(m) => return finish(job, &[m]),
    };
    let local = match local(p) {
        Ok(l) => l,
        Err(m) => return finish(job, &[m]),
    };
    let modes = p.elems("MSGMODE");
    let mode = |i: usize| modes.get(i).copied().flatten().map(|v| v.source().to_ascii_uppercase()).unwrap_or_default();
    let len = usize::try_from(int(p, "PKTLEN", 256, 8, 65_500)).unwrap_or(256).min(ping::max_len(t.addr.is_ipv6()));
    let o = ping::Options {
        count: u32::try_from(int(p, "NBRPKT", 5, 1, 999)).unwrap_or(5),
        len,
        wait: Duration::from_secs(u64::try_from(int(p, "WAITTIME", 1, 1, 120)).unwrap_or(1)),
        verbose: mode(0) != "*QUIET",
        escape: mode(1) == "*ESCAPE",
    };
    let ttl = p.int("IPTTL").filter(|n| *n > 0).and_then(|n| u32::try_from(n.min(255)).ok());
    let mut pinger = match nettools::sys::Pinger::open(t.addr, local, ttl, tos(p)) {
        Ok(x) => x,
        Err(e) => {
            return finish(
                job,
                &[escape(tmsg::SOCKET_FAILED, format!("The echo socket could not be opened: {e}."))],
            );
        }
    };
    let out = ping::run(&t, &o, &mut pinger, &mut Instant::now);
    finish(job, &out.msgs)
}

/// `TRACEROUTE` and `TRCTCPRTE`.
fn trace_cmd(job: &mut Job, host: &Host, p: &Parms) -> Result<String, Fail> {
    let t = match target(host, p) {
        Ok(t) => t,
        Err(m) => return finish(job, &[m]),
    };
    let local = match local(p) {
        Ok(l) => l,
        Err(m) => return finish(job, &[m]),
    };
    let range = p.elems("RANGE");
    let elem = |i: usize, d: i64| {
        range
            .get(i)
            .copied()
            .flatten()
            .and_then(|v| v.source().trim().parse::<i64>().ok())
            .unwrap_or(d)
            .clamp(1, 255)
    };
    let o = route::Options {
        first: u8::try_from(elem(0, 1)).unwrap_or(1),
        max: u8::try_from(elem(1, 30)).unwrap_or(30),
        probes: u32::try_from(int(p, "PROBES", 3, 1, 64)).unwrap_or(3),
        wait: Duration::from_secs(u64::try_from(int(p, "WAITTIME", 3, 1, 120)).unwrap_or(3)),
        lookup: !matches!(p.name("NAMELOOKUP").as_str(), "*NO" | "0"),
    };
    // PKTLEN counts the IP and transport headers.
    let headers = if t.addr.is_ipv6() { 48 } else { 28 };
    let data = usize::try_from(int(p, "PKTLEN", 40, 40, 65_535)).unwrap_or(40).saturating_sub(headers);
    let kind = match p.name("PROBEPCL").as_str() {
        "*UDP" | "0" => nettools::sys::ProbeKind::Udp {
            port: u16::try_from(int(p, "RMTPORT", 33_434, 1, 65_535)).unwrap_or(33_434),
        },
        _ => nettools::sys::ProbeKind::Icmp,
    };
    let mut msgs = vec![route::probing(t.addr, t.name.as_deref(), local, o.max)];
    let mut prober = match nettools::sys::RouteProber::open(t.addr, local, kind, data) {
        Ok(x) => x,
        Err(e) => {
            msgs.push(escape(tmsg::SOCKET_FAILED, format!("The probe socket could not be opened: {e}.")));
            return finish(job, &msgs);
        }
    };
    let hc = host_config(host);
    msgs.extend(route::run(&o, &mut prober, &mut |a| hc.name_of(a).map(str::to_string)));
    finish(job, &msgs)
}

/// One view of the service's data.
fn view(host: &Host, what: &str) -> Result<stg::tcp::PanelData, Fail> {
    match host
        .link
        .borrow_mut()
        .with(|l| l.tcp(TcpOp::Netstat { what: what.into(), args: Vec::new() }))
    {
        TcpReply::Panel { data } => Ok(data),
        TcpReply::Failed { note } => Err(Fail::new(&note.msgid, note.line())),
        other => Err(Fail::new("CPF9898", format!("unexpected answer {other:?}"))),
    }
}

/// `NETSTAT OPTION(n)` as text: `*SELECT` shows the IPv4 views and the
/// stack status.
fn netstat_text(host: &Host, option: u32) -> Result<String, Fail> {
    let mut lines: Vec<String> = Vec::new();
    let mut section = |what: &str, title: &str, cols: &[(&str, &str, usize)]| -> Result<(), Fail> {
        let data = view(host, what)?;
        let rows = data.1.first().map(|(_, r)| r.clone()).unwrap_or_default();
        let rows: Vec<netstat::Row> = rows
            .into_iter()
            .map(|r| r.into_iter().map(|(k, v)| (k, v.trim().to_string())).collect())
            .collect();
        lines.extend(netstat::text(title, &rows, cols));
        lines.push(String::new());
        Ok(())
    };
    let ifc: &[(&str, &str, usize)] = &[("IADR", "Internet address", 15), ("INET", "Network address", 15), ("ILIND", "Line", 10), ("ISTS", "Status", 10)];
    let ifc6: &[(&str, &str, usize)] = &[("XADR", "Internet address", 39), ("XPFX", "Prefix", 6), ("XLIND", "Line", 10), ("XSTS", "Status", 10)];
    let rte: &[(&str, &str, usize)] = &[("RDEST", "Route destination", 17), ("RMASK", "Subnet mask", 15), ("RHOP", "Next hop", 15), ("RAVAIL", "Available", 9), ("RTYPE", "Type", 9)];
    let rte6: &[(&str, &str, usize)] = &[("YDEST", "Route destination", 39), ("YMASK", "Prefix", 6), ("YHOP", "Next hop", 39), ("YSTS", "Status", 9)];
    let cnn: &[(&str, &str, usize)] = &[("CRADR", "Remote address", 15), ("CRPORTN", "Port", 5), ("CLADR", "Local address", 15), ("CLPORTN", "Port", 5), ("CTYPE", "Type", 4), ("CSTATE", "State", 12), ("CUSER", "User", 10)];
    let cnn6: &[(&str, &str, usize)] = &[("ZRADR", "Remote address", 39), ("ZRPORTN", "Port", 5), ("ZLADR", "Local address", 39), ("ZLPORTN", "Port", 5), ("ZTYPE", "Type", 4), ("ZSTATE", "State", 12)];
    match option {
        1 => section("IFC", "IPv4 interfaces", ifc)?,
        2 => section("RTE", "IPv4 routes", rte)?,
        3 => section("CNN", "IPv4 connections", cnn)?,
        4 => section("IFC6", "IPv6 interfaces", ifc6)?,
        5 => section("RTE6", "IPv6 routes", rte6)?,
        6 => section("CNN6", "IPv6 connections", cnn6)?,
        10 => {}
        _ => {
            section("IFC", "IPv4 interfaces", ifc)?;
            section("RTE", "IPv4 routes", rte)?;
            section("CNN", "IPv4 connections", cnn)?;
        }
    }
    if option == 10 || option == 0 {
        let data = view(host, "STS")?;
        lines.push("TCP/IP stack status".into());
        for (k, v) in &data.0 {
            let label = match k.as_str() {
                "S4" => "IPv4 stack status",
                "S6" => "IPv6 stack status",
                "SACT" => "Seconds active",
                "SSTR" => "Last started",
                "SEND" => "Last ended",
                _ => continue,
            };
            lines.push(format!("{label:<20}{v}"));
        }
    }
    Ok(lines.join("\n") + "\n")
}
