// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The configuration applied to the kernel: `STRTCP`, `ENDTCP`,
//! `STRTCPIFC`, `ENDTCPIFC`, routes, attributes, the host name and the
//! port rules, plus [`refresh`] for line and carrier changes. Every
//! step goes through [`NetOps`]; what happens is reported as
//! [`Note`]s (the job log's and `QSYSOPR`'s messages).

use std::net::IpAddr;

use crate::addr::{IfcAddr, contains};
use crate::config::Config;
use crate::error::{Error, Result};
use crate::files::Table;
use crate::line::LineState;
use crate::model::LineRef;
use crate::model::ifc::Interface;
use crate::model::rte::{Dest, Route};
use crate::msg::{self, Note};
use crate::net::{KAddr, KRoute, NetOps};
use crate::state::{IfcRun, IfcStatus, RteRun, RteStatus, State, TcpStatus};

/// The kernel link virtual IP addresses go on.
pub const VIP_LINK: &str = "dummy0";

/// What one call works with.
pub struct Ctx<'a> {
    /// The configuration.
    pub cfg: &'a Config,
    /// The run-time state.
    pub st: &'a mut State,
    /// The kernel.
    pub net: &'a mut dyn NetOps,
    /// The line descriptions.
    pub lines: &'a mut dyn LineState,
    /// Seconds since the epoch.
    pub now: u64,
}

/// `STRTCP`'s parameters that the engine uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartOpts {
    /// `STRIFC(*YES)`: start the autostart interfaces.
    pub strifc: bool,
    /// `STRIP6(*YES)`: start the IPv6 ones too.
    pub strip6: bool,
}

impl Default for StartOpts {
    fn default() -> Self {
        Self {
            strifc: true,
            strip6: true,
        }
    }
}

/// `STRTCP`: attributes, host name, the loopback, the autostart
/// interfaces, routes and port rules.
pub fn start_tcp(c: &mut Ctx<'_>, o: StartOpts) -> Result<Vec<Note>> {
    if matches!(c.st.tcp, TcpStatus::Active | TcpStatus::Starting) {
        return Err(Error::new(msg::ALREADY_ACTIVE, "TCP/IP is already active."));
    }
    c.st.tcp = TcpStatus::Starting;
    c.st.started = c.now;
    let mut notes = apply_attrs(c);
    notes.extend(apply_hostname(c));
    loopback_up(c, &mut notes);
    let keys: Vec<(String, bool, bool, bool)> = c
        .cfg
        .ifcs
        .list
        .iter()
        .map(|i| (i.key(), i.line == LineRef::Loopback, i.autostart, i.addr.is_v6()))
        .collect();
    for (key, lo, auto, v6) in keys {
        let wanted = lo || (o.strifc && auto && (!v6 || o.strip6));
        if !wanted {
            continue;
        }
        match start_ifc(c, &key) {
            Ok(n) => notes.extend(n),
            Err(e) => notes.push(Note::diag(&e.msgid, e.text)),
        }
    }
    c.st.tcp = TcpStatus::Active;
    notes.extend(apply_ports(c));
    notes.extend(sync_routes(c));
    notes.push(Note::info(msg::STARTED, "TCP/IP started."));
    Ok(notes)
}

/// `ENDTCP`: every interface ended, our routes and port rules removed.
/// `OPTION(*IMMED)` and `*CNTRLD` differ only for the servers, which
/// the caller ends first.
pub fn end_tcp(c: &mut Ctx<'_>) -> Result<Vec<Note>> {
    if c.st.tcp == TcpStatus::Inactive {
        return Err(Error::new(msg::NOT_ACTIVE, "TCP/IP is not active."));
    }
    c.st.tcp = TcpStatus::Ending;
    let mut notes = Vec::new();
    let started: Vec<String> = c
        .st
        .ifcs
        .iter()
        .filter(|(_, r)| r.status != IfcStatus::Inactive)
        .map(|(k, _)| k.clone())
        .collect();
    for key in started {
        notes.extend(stop_ifc(c, &key));
    }
    notes.extend(sync_routes(c));
    if !c.st.rules.is_empty() {
        if let Err(e) = c.net.set_port_rules(&[]) {
            notes.push(Note::diag(msg::ATTR_FAILED, format!("Port restrictions not removed: {e}.")));
        }
        c.st.rules.clear();
    }
    c.st.tcp = TcpStatus::Inactive;
    notes.push(Note::info(msg::ENDED, "TCP/IP ended."));
    Ok(notes)
}

fn loopback_up(c: &mut Ctx<'_>, notes: &mut Vec<Note>) {
    let lo = match c.net.link("lo") {
        Ok(Some(l)) => l,
        Ok(None) => {
            notes.push(Note::diag(msg::IFC_FAILED, "The kernel has no loopback link."));
            return;
        }
        Err(e) => {
            notes.push(Note::diag(msg::IFC_FAILED, format!("Loopback link: {e}.")));
            return;
        }
    };
    if !lo.up && let Err(e) = c.net.set_up(lo.index, true) {
        notes.push(Note::diag(msg::IFC_FAILED, format!("Loopback link not started: {e}.")));
    }
    for (ip, prefix) in [("127.0.0.1", 8), ("::1", 128)] {
        let a = KAddr {
            link: lo.index,
            addr: ip.parse().expect("loopback"),
            prefix,
        };
        if let Err(e) = c.net.add_address(&a)
            && !e.exists
        {
            notes.push(Note::diag(msg::IFC_FAILED, format!("Loopback address {ip}: {e}.")));
        }
    }
}

fn find<'c>(cfg: &'c Config, key: &str) -> Result<&'c Interface> {
    let addr = IfcAddr::parse(key)?;
    cfg.ifcs
        .get(&addr)
        .ok_or_else(|| Error::new(msg::NOT_FOUND, format!("Interface {key} is not configured.")))
}

fn set(c: &mut Ctx<'_>, key: &str, f: impl FnOnce(&mut IfcRun)) {
    let now = c.now;
    let r = c.st.ifcs.entry(key.to_string()).or_default();
    f(r);
    r.since = now;
}

/// `STRTCPIFC`: start one interface (varying its line on first).
pub fn start_ifc(c: &mut Ctx<'_>, key: &str) -> Result<Vec<Note>> {
    let i = find(c.cfg, key)?.clone();
    let key = i.key();
    if !matches!(c.st.tcp, TcpStatus::Active | TcpStatus::Starting) {
        return Err(Error::new(
            msg::NOT_AVAILABLE,
            "TCP/IP is not active; start it with STRTCP first.",
        ));
    }
    if c.st.ifc_status(&key).started() {
        return Err(Error::new(msg::IFC_ACTIVE, format!("Interface {key} is already active.")));
    }
    set(c, &key, |r| r.status = IfcStatus::Starting);
    match bring_up(c, &i) {
        Ok((status, link, made, installed)) => {
            set(c, &key, |r| {
                r.status = status;
                r.link = Some(link);
                r.made = made;
                r.installed = installed;
                r.error.clear();
            });
            let mut notes = vec![Note::info(msg::IFC_STARTED, format!("Interface {key} started."))];
            if c.st.tcp == TcpStatus::Active {
                notes.extend(sync_routes(c));
            }
            Ok(notes)
        }
        Err(e) => {
            set(c, &key, |r| {
                r.status = IfcStatus::Failed;
                r.error.clone_from(&e.text);
            });
            Err(Error::new(
                &e.msgid,
                format!("Interface {key} not started: {}", e.text),
            ))
        }
    }
}

type Up = (IfcStatus, String, Option<String>, Option<KAddr>);

/// Link, VLAN, MTU and address for an interface.
fn bring_up(c: &mut Ctx<'_>, i: &Interface) -> Result<Up> {
    let kerr = |e: crate::net::NetError| Error::new(msg::IFC_FAILED, format!("{e}."));
    let (base, carrier_line) = match &i.line {
        LineRef::Loopback => ("lo".to_string(), None),
        LineRef::Opc => {
            return Err(Error::new(msg::LINE_NOT_VALID, "*OPC interfaces are not supported."));
        }
        LineRef::VirtualIp => {
            if c.net.link(VIP_LINK).map_err(kerr)?.is_none() {
                c.net.create_dummy(VIP_LINK).map_err(kerr)?;
            }
            (VIP_LINK.to_string(), None)
        }
        LineRef::Line(n) => {
            if !c.lines.exists(n) {
                return Err(Error::new(msg::LINE_NOT_FOUND, format!("Line {n} not found.")));
            }
            if !c.lines.varied_on(n) {
                c.lines
                    .vary_on(n)
                    .map_err(|e| Error::new(msg::LINE_FAILED, format!("Line {n} could not be varied on: {e}.")))?;
            }
            let k = c
                .lines
                .interface(n)
                .ok_or_else(|| Error::new(msg::LINE_NOT_VALID, format!("Line {n} has no network port.")))?;
            (k, Some(n.clone()))
        }
    };
    let Some(mut link) = c.net.link(&base).map_err(kerr)? else {
        return Err(Error::new(msg::LINE_NOT_VALID, format!("Network link {base} does not exist.")));
    };
    let mut made = None;
    if let Some(vid) = i.vlan {
        let name: String = format!("{base}.{vid}").chars().take(15).collect();
        link = if let Some(l) = c.net.link(&name).map_err(kerr)? {
            l
        } else {
            let l = c.net.create_vlan(&name, link.index, vid).map_err(kerr)?;
            made = Some(name);
            l
        };
    }
    if !link.up {
        c.net.set_up(link.index, true).map_err(kerr)?;
    }
    // The loopback's MTU is IBM's figure; the kernel's lo keeps its own.
    if let Some(m) = i.mtu
        && m != link.mtu
        && i.line != LineRef::Loopback
    {
        c.net.set_mtu(link.index, m).map_err(kerr)?;
    }
    let installed = match i.addr {
        IfcAddr::Ip(ip) => {
            let a = KAddr {
                link: link.index,
                addr: ip,
                prefix: i.prefix(),
            };
            match c.net.add_address(&a) {
                Ok(()) => (i.line != LineRef::Loopback).then_some(a),
                Err(e) if e.exists => (i.line != LineRef::Loopback).then_some(a),
                Err(e) => return Err(kerr(e)),
            }
        }
        IfcAddr::Slaac6 => {
            let n = &link.name;
            for (k, v) in [("autoconf", "1"), ("accept_ra", "1")] {
                c.net
                    .sysctl_write(&format!("ipv6/conf/{n}/{k}"), v)
                    .map_err(kerr)?;
            }
            None
        }
        IfcAddr::Dhcp4 => {
            return Err(Error::new(
                msg::IFC_FAILED,
                "Addresses from DHCP are not available in this release.",
            ));
        }
    };
    let status = if carrier_line.is_some() && link.carrier == Some(false) {
        IfcStatus::RcyPnd
    } else {
        IfcStatus::Active
    };
    Ok((status, link.name, made, installed))
}

/// `ENDTCPIFC`: end one interface.
pub fn end_ifc(c: &mut Ctx<'_>, key: &str) -> Result<Vec<Note>> {
    let key = find(c.cfg, key).map_or_else(|_| key.to_string(), Interface::key);
    if c.st.ifc_status(&key) == IfcStatus::Inactive {
        return Err(Error::new(msg::IFC_INACTIVE, format!("Interface {key} is not active.")));
    }
    let mut notes = stop_ifc(c, &key);
    if c.st.tcp == TcpStatus::Active {
        notes.extend(sync_routes(c));
    }
    Ok(notes)
}

fn stop_ifc(c: &mut Ctx<'_>, key: &str) -> Vec<Note> {
    let mut notes = Vec::new();
    let run = c.st.ifcs.get(key).cloned().unwrap_or_default();
    if let Some(a) = &run.installed
        && let Err(e) = c.net.del_address(a)
        && !e.not_found
    {
        notes.push(Note::diag(msg::IFC_FAILED, format!("Address of {key} not removed: {e}.")));
    }
    if let Some(made) = &run.made {
        let still_used = c
            .st
            .ifcs
            .iter()
            .any(|(k, r)| k != key && r.status.started() && r.link.as_deref() == Some(made));
        if !still_used
            && let Ok(Some(l)) = c.net.link(made)
            && let Err(e) = c.net.delete_link(l.index)
        {
            notes.push(Note::diag(msg::IFC_FAILED, format!("Link {made} not removed: {e}.")));
        }
    }
    set(c, key, |r| {
        r.status = IfcStatus::Inactive;
        r.link = None;
        r.made = None;
        r.installed = None;
    });
    notes.push(Note::info(msg::IFC_ENDED, format!("Interface {key} ended.")));
    notes
}

/// The link and index a route goes out on: an active interface whose
/// network holds the next hop (the binding interface when one is named).
fn route_link(c: &mut Ctx<'_>, r: &Route) -> Option<(String, u32)> {
    if c.st.tcp != TcpStatus::Active {
        return None;
    }
    let candidates: Vec<&Interface> = c
        .cfg
        .ifcs
        .list
        .iter()
        .filter(|i| c.st.ifc_status(&i.key()) == IfcStatus::Active)
        .filter(|i| match r.bind_ifc {
            Some(b) => i.addr == IfcAddr::Ip(IpAddr::V4(b)),
            None => true,
        })
        .collect();
    let hop = r.next_hop;
    let on = candidates.iter().find(|i| match i.addr {
        IfcAddr::Ip(ip) => contains(ip, i.prefix(), hop),
        IfcAddr::Slaac6 => hop.is_ipv6(),
        IfcAddr::Dhcp4 => false,
    })?;
    let link = c.st.ifcs.get(&on.key())?.link.clone()?;
    let index = c.net.link(&link).ok().flatten()?.index;
    Some((link, index))
}

fn kroute(r: &Route, index: u32) -> KRoute {
    KRoute {
        dst: match r.dest {
            Dest::DftRoute => None,
            Dest::Ip(ip) if ip.is_unspecified() => None,
            d => Some(d.ip()),
        },
        prefix: r.prefix,
        gateway: Some(r.next_hop),
        link: Some(index),
        mtu: r.mtu,
        tos: r.tos.bits(),
        // DUPRTEPTY 10 is the most preferred; the kernel prefers the
        // lowest metric.
        metric: Some(u32::from(11 - r.dup_pty.clamp(1, 10))),
        ours: true,
    }
}

/// Put every route whose next hop is reachable into the kernel, and
/// take out the ones that no longer are (or are no longer configured).
pub fn sync_routes(c: &mut Ctx<'_>) -> Vec<Note> {
    let mut notes = Vec::new();
    let keys: Vec<_> = c.cfg.rtes.list.iter().map(Route::key).collect();
    let gone: Vec<_> = c.st.rtes.keys().filter(|k| !keys.contains(k)).cloned().collect();
    for k in gone {
        if let Some(run) = c.st.rtes.remove(&k)
            && let Some(kr) = run.installed
        {
            let _ = c.net.del_route(&kr);
        }
    }
    let routes: Vec<Route> = c.cfg.rtes.list.clone();
    for r in routes {
        let key = r.key();
        let want = route_link(c, &r);
        let run = c.st.rtes.get(&key).cloned().unwrap_or_default();
        match (run.status, want) {
            (RteStatus::Active, Some((link, _))) if run.link.as_deref() == Some(link.as_str()) => {}
            (status, want) => {
                if status == RteStatus::Active
                    && let Some(kr) = &run.installed
                {
                    let _ = c.net.del_route(kr);
                }
                let mut next = RteRun::default();
                if let Some((link, index)) = want {
                    let kr = kroute(&r, index);
                    match c.net.add_route(&kr) {
                        Ok(()) => {
                            next = RteRun {
                                status: RteStatus::Active,
                                link: Some(link),
                                installed: Some(kr),
                                error: String::new(),
                            };
                        }
                        Err(e) if e.exists => {
                            next = RteRun {
                                status: RteStatus::Active,
                                link: Some(link),
                                installed: None,
                                error: String::new(),
                            };
                        }
                        Err(e) => {
                            notes.push(Note::diag(
                                msg::RTE_FAILED,
                                format!("Route to {} through {} not added: {e}.", r.dest, r.next_hop),
                            ));
                            next.status = RteStatus::Failed;
                            next.error = e.text;
                        }
                    }
                }
                c.st.rtes.insert(key, next);
            }
        }
    }
    notes
}

/// The `CHGTCPA` attributes into their sysctls.
pub fn apply_attrs(c: &mut Ctx<'_>) -> Vec<Note> {
    let net = &mut *c.net;
    let mut reads: Vec<(String, Option<String>)> = Vec::new();
    for key in ["ipv4/tcp_rmem", "ipv4/tcp_wmem"] {
        reads.push((key.into(), net.sysctl_read(key).ok()));
    }
    let writes = c.cfg.attrs.sysctls(&mut |k| {
        reads.iter().find(|(x, _)| x == k).and_then(|(_, v)| v.clone())
    });
    let mut notes = Vec::new();
    for (k, v) in writes {
        if let Err(e) = net.sysctl_write(&k, &v) {
            notes.push(Note::diag(msg::ATTR_FAILED, format!("Kernel setting {k} not changed: {e}.")));
        }
    }
    notes
}

/// The local host name (`CHGTCPDMN HOSTNAME`) as the kernel's.
pub fn apply_hostname(c: &mut Ctx<'_>) -> Vec<Note> {
    let name = c.cfg.domain.host_name.to_ascii_lowercase();
    if name.is_empty() {
        return Vec::new();
    }
    match c.net.set_hostname(&name) {
        Ok(()) => Vec::new(),
        Err(e) => vec![Note::diag(msg::ATTR_FAILED, format!("Host name not set: {e}."))],
    }
}

/// The port restrictions into the backstop.
pub fn apply_ports(c: &mut Ctx<'_>) -> Vec<Note> {
    let want = c.cfg.ports.ranges();
    if want == c.st.rules {
        return Vec::new();
    }
    match c.net.set_port_rules(&want) {
        Ok(()) => {
            c.st.rules = want;
            Vec::new()
        }
        Err(e) => vec![Note::diag(msg::ATTR_FAILED, format!("Port restrictions not applied: {e}."))],
    }
}

/// After a command changed a table while TCP/IP is active.
pub fn on_change(c: &mut Ctx<'_>, t: Table) -> Vec<Note> {
    if c.st.tcp != TcpStatus::Active {
        return Vec::new();
    }
    match t {
        Table::Rte | Table::Ifc => sync_routes(c),
        Table::TcpIp => {
            let mut n = apply_attrs(c);
            n.extend(apply_hostname(c));
            n
        }
        Table::Port => apply_ports(c),
        _ => Vec::new(),
    }
}

/// Line and carrier changes: an interface whose line was varied off
/// ends, one whose carrier went away waits (`RCYPND`) and comes back
/// with it.
pub fn refresh(c: &mut Ctx<'_>) -> Vec<Note> {
    let mut notes = Vec::new();
    if c.st.tcp != TcpStatus::Active {
        return notes;
    }
    let links = c.net.links().unwrap_or_default();
    let ifcs: Vec<Interface> = c.cfg.ifcs.list.clone();
    let mut changed = false;
    for i in ifcs {
        let LineRef::Line(line) = &i.line else {
            continue;
        };
        let key = i.key();
        let run = c.st.ifcs.get(&key).cloned().unwrap_or_default();
        if !run.status.started() {
            continue;
        }
        if !c.lines.varied_on(line) {
            notes.extend(stop_ifc(c, &key));
            changed = true;
            continue;
        }
        let carrier = run
            .link
            .as_ref()
            .and_then(|n| links.iter().find(|l| &l.name == n))
            .map(|l| l.carrier);
        let next = match carrier {
            None => IfcStatus::Failed,
            Some(Some(false)) => IfcStatus::RcyPnd,
            Some(_) => IfcStatus::Active,
        };
        if next != run.status {
            if next == IfcStatus::Active
                && let Some(a) = &run.installed
            {
                let _ = c.net.add_address(a);
            }
            set(c, &key, |r| r.status = next);
            changed = true;
            let id = if next == IfcStatus::Active { msg::IFC_STARTED } else { msg::IFC_FAILED };
            notes.push(Note::diag(id, format!("Interface {key} is now {}.", next.name())));
        }
    }
    if changed {
        notes.extend(sync_routes(c));
    }
    notes
}

/// What the kernel has against what the configuration says, one line
/// each (`svc.tcp`): links, addresses, routes, port rules, sysctls.
pub fn report(cfg: &Config, st: &State, net: &mut dyn NetOps, what: &str) -> Vec<String> {
    let mut out = Vec::new();
    let links = net.links().unwrap_or_default();
    let name = |i: u32| links.iter().find(|l| l.index == i).map_or("?".to_string(), |l| l.name.clone());
    let all = what.is_empty() || what == "*ALL";
    if all || what == "ifc" {
        out.push(format!("TCP/IP {}", st.tcp.name()));
        for i in cfg.ifcs.sorted() {
            let run = st.ifcs.get(&i.key()).cloned().unwrap_or_default();
            out.push(format!(
                "ifc {} /{} {} {} link={}{}",
                i.key(),
                i.prefix(),
                i.line,
                run.status.name(),
                run.link.unwrap_or_default(),
                if run.error.is_empty() { String::new() } else { format!(" ({})", run.error) }
            ));
        }
        for a in net.addresses().unwrap_or_default() {
            out.push(format!("kernel addr {} {}/{}", name(a.link), a.addr, a.prefix));
        }
    }
    if all || what == "rte" {
        for r in cfg.rtes.sorted() {
            out.push(format!(
                "rte {} {} via {} {}",
                r.dest,
                r.mask_text(),
                r.next_hop,
                st.rte_status(&r.key()).name()
            ));
        }
        for r in net.routes().unwrap_or_default() {
            out.push(format!(
                "kernel route {}/{} via {} dev {}{}",
                r.dst.map_or("default".into(), |d| d.to_string()),
                r.prefix,
                r.gateway.map_or("-".into(), |g| g.to_string()),
                r.link.map_or("-".into(), name),
                if r.ours { " qs400" } else { "" }
            ));
        }
    }
    if all || what == "port" {
        for (p, a, b) in cfg.ports.ranges() {
            out.push(format!("port {} {a}-{b}", p.name()));
        }
        out.push(format!("kernel port rules {}", st.rules.len()));
    }
    if all || what == "attr" {
        for (k, v) in cfg.attrs.sysctls(&mut |_| None) {
            let cur = net.sysctl_read(&k).unwrap_or_else(|e| format!("({e})"));
            out.push(format!("attr {k} want {v} kernel {}", cur.replace('\t', " ")));
        }
    }
    if all || what == "host" {
        for e in cfg.hosts.sorted() {
            out.push(format!("host {} {}", e.addr, e.names.join(" ")));
        }
        out.push(format!("domain {} search {}", cfg.domain.fqdn(), cfg.domain.search().join(" ")));
    }
    out
}
