// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The sample system's TCP/IP: its domain, six interfaces, four routes,
//! a host table and the TCP servers.

use std::collections::BTreeMap;
use std::sync::Mutex;

use qsweb::msg::{Kind, Message};

use super::text;
use super::work::parse;
use crate::env::net::{HostEntry, Interface, Net, Route, Server, TcpConfig};
use crate::util::secs;

/// The sample system's TCP/IP, changeable by tests.
pub struct FixtureNet {
    /// The state.
    pub state: Mutex<State>,
    /// Commands run, as typed.
    pub ran: Mutex<Vec<String>>,
}

/// What the commands change.
pub struct State {
    /// The attributes and domain.
    pub config: TcpConfig,
    /// The interfaces.
    pub interfaces: Vec<Interface>,
    /// The routes.
    pub routes: Vec<Route>,
    /// The servers.
    pub servers: Vec<Server>,
}

fn ifc(address: &str, mask: &str, line: &str, active: bool, mtu: u32, alias: &str) -> Interface {
    Interface {
        address: address.into(),
        mask: mask.into(),
        line: line.into(),
        line_type: if line.starts_with('*') {
            "*NONE"
        } else {
            "*ELAN"
        }
        .into(),
        active,
        autostart: active,
        mtu,
        alias: alias.into(),
        generated: line.starts_with('*') || address.contains(':'),
    }
}

fn rte(dest: &str, mask: &str, hop: &str, ifc: &str, metric: u32, active: bool) -> Route {
    Route {
        dest: dest.into(),
        mask: mask.into(),
        next_hop: hop.into(),
        interface: ifc.into(),
        kind: "Static".into(),
        metric,
        active,
    }
}

/// Name, description, started, autostart command, ports.
type Svr = (
    &'static str,
    &'static str,
    bool,
    &'static str,
    &'static [u16],
);

const SERVERS: &[Svr] = &[
    (
        "*HTTP",
        "HTTP server (7 instances)",
        true,
        "CHGHTTPA",
        &[80, 443, 2001, 2002, 2005, 2003, 2006, 2010],
    ),
    ("*TELNET", "Telnet server", true, "CHGTELNA", &[23, 992]),
    ("*FTP", "FTP server", true, "CHGFTPA", &[21]),
    ("*SSHD", "SSH server", true, "", &[22]),
    ("*DDM", "DDM/DRDA server", true, "CHGDDMTCPA", &[446, 448]),
    ("*NTP", "NTP client/server", true, "CHGNTPA", &[123]),
    ("*SNMP", "SNMP agent", false, "", &[161]),
    ("*NETSVR", "File and print server (SMB)", true, "", &[445]),
    ("*SMTP", "SMTP server", false, "CHGSMTPA", &[25]),
    ("*DNS", "DNS server", false, "", &[53]),
    ("*LPD", "Line printer daemon", true, "CHGLPDA", &[515]),
    ("*CENTRAL", "Host server - central", true, "", &[8470, 9470]),
    (
        "*DATABASE",
        "Host server - database",
        true,
        "",
        &[8471, 9471],
    ),
    ("*SIGNON", "Host server - sign-on", true, "", &[8476, 9476]),
    (
        "*RMTCMD",
        "Host server - remote command",
        true,
        "",
        &[8475, 9475],
    ),
    ("*FILE", "Host server - file", true, "", &[8473, 9473]),
    ("*SVRMAP", "Server mapper", true, "", &[449]),
];

impl Default for FixtureNet {
    fn default() -> Self {
        let state = State {
            config: TcpConfig {
                host: "qshq400".into(),
                domain: "quartz.example".into(),
                search: "quartz.example hq.quartz.example".into(),
                priority: "*LOCAL".into(),
                dns: vec!["10.0.10.1".into(), "10.0.10.2".into()],
                forwarding: false,
                ipv6: Some(2),
                active_since: Some(secs(2026, 10, 27, 5, 59, 40)),
            },
            interfaces: vec![
                ifc(
                    "10.0.10.20",
                    "255.255.255.0",
                    "ETHLIN01",
                    true,
                    1500,
                    "MGMT",
                ),
                ifc(
                    "10.0.10.21",
                    "255.255.255.0",
                    "ETHLIN02",
                    true,
                    9000,
                    "DATA",
                ),
                ifc("172.16.0.5", "255.255.255.0", "ETHLIN03", true, 1500, "LAB"),
                ifc(
                    "192.168.99.1",
                    "255.255.255.0",
                    "ETHLIN04",
                    false,
                    1500,
                    "TEST",
                ),
                ifc("127.0.0.1", "255.0.0.0", "*LOOPBACK", true, 576, ""),
                ifc("2001:db8:10::20/64", "", "ETHLIN01", true, 1500, ""),
            ],
            routes: vec![
                rte("*DFTROUTE", "*NONE", "10.0.10.1", "10.0.10.20", 1, true),
                rte(
                    "10.0.20.0",
                    "255.255.255.0",
                    "10.0.10.254",
                    "10.0.10.21",
                    1,
                    true,
                ),
                rte(
                    "10.1.0.0",
                    "255.255.0.0",
                    "172.16.0.1",
                    "172.16.0.5",
                    5,
                    true,
                ),
                rte(
                    "192.168.50.0",
                    "255.255.255.0",
                    "192.168.99.254",
                    "192.168.99.1",
                    1,
                    false,
                ),
            ],
            servers: SERVERS
                .iter()
                .map(|(name, text, started, cmd, ports)| Server {
                    name: (*name).into(),
                    text: (*text).into(),
                    started: *started,
                    autostart: *started,
                    attr_cmd: (*cmd).into(),
                    ports: ports.to_vec(),
                })
                .collect(),
        };
        Self {
            state: Mutex::new(state),
            ran: Mutex::new(Vec::new()),
        }
    }
}

fn msg(kind: Kind, id: &str, data: &[&str]) -> Message {
    let t = text::second_level(id, data)
        .map(|s| s.text)
        .unwrap_or_default();
    Message::new(kind, id, &t)
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    match s.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        Some(r) => r.replace("''", "'"),
        None => s.to_string(),
    }
}

/// A parameter's text, unquoted, blank when absent.
fn val(p: &BTreeMap<String, String>, k: &str) -> String {
    p.get(k).map(|v| unquote(v)).unwrap_or_default()
}

/// A list parameter's elements (`'a' 'b'`).
fn elems(v: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in v.chars() {
        match ch {
            '\'' => quoted = !quoted,
            ' ' if !quoted => out.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    out.push(cur);
    out.into_iter().filter(|s| !s.is_empty()).collect()
}

fn server_named<'a>(st: &'a mut State, p: &BTreeMap<String, String>) -> Option<&'a mut Server> {
    let n = val(p, "SERVER");
    st.servers.iter_mut().find(|s| s.name == n)
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::many_single_char_names)]
fn run_in(st: &mut State, name: &str, p: &BTreeMap<String, String>) -> Result<Message, Message> {
    let ok = |id: &str, d: &str| Ok(msg(Kind::Done, id, &[d]));
    let addr = val(p, "INTNETADR");
    match name {
        "STRTCPSVR" if val(p, "SERVER") == "*AUTOSTART" => {
            for s in st.servers.iter_mut().filter(|s| s.autostart) {
                s.started = true;
            }
            ok("TCP1A07", "*AUTOSTART")
        }
        "STRTCPSVR" => {
            let restart = p.contains_key("RESTART");
            let Some(s) = server_named(st, p) else {
                return Err(msg(Kind::Error, "TCP1D07", &[&val(p, "SERVER")]));
            };
            if s.started && !restart {
                return Err(msg(Kind::Error, "TCP1A08", &[&s.name]));
            }
            s.started = true;
            ok("TCP1A07", &s.name.clone())
        }
        "ENDTCPSVR" => {
            let Some(s) = server_named(st, p) else {
                return Err(msg(Kind::Error, "TCP1D07", &[&val(p, "SERVER")]));
            };
            if !s.started {
                return Err(msg(Kind::Error, "TCP1A09", &[&s.name]));
            }
            s.started = false;
            ok("TCP1A0A", &s.name.clone())
        }
        n if n.starts_with("CHG") && n.ends_with('A') && p.contains_key("AUTOSTART") => {
            let Some(s) = st.servers.iter_mut().find(|s| s.attr_cmd == n) else {
                return Err(msg(Kind::Error, "CPD0030", &[n]));
            };
            s.autostart = val(p, "AUTOSTART") == "*YES";
            ok("TCP1A10", &s.name.clone())
        }
        "STRTCPIFC" | "ENDTCPIFC" => {
            let Some(i) = st.interfaces.iter_mut().find(|i| i.address == addr) else {
                return Err(msg(Kind::Error, "TCP2658", &[&addr]));
            };
            i.active = name == "STRTCPIFC";
            ok(if i.active { "TCP1A0B" } else { "TCP1A0C" }, &addr)
        }
        "ADDTCPIFC" => {
            st.interfaces.push(Interface {
                address: addr.clone(),
                mask: val(p, "SUBNETMASK"),
                line: val(p, "LIND"),
                line_type: "*ELAN".into(),
                active: false,
                autostart: val(p, "AUTOSTART") != "*NO",
                mtu: val(p, "MTU").parse().unwrap_or(1500),
                alias: val(p, "ALIASNAME"),
                generated: false,
            });
            ok("TCP1D03", &format!("Interface {addr}"))
        }
        "CHGTCPIFC" => {
            let Some(i) = st.interfaces.iter_mut().find(|i| i.address == addr) else {
                return Err(msg(Kind::Error, "TCP2658", &[&addr]));
            };
            if p.contains_key("SUBNETMASK") {
                i.mask = val(p, "SUBNETMASK");
            }
            if let Ok(m) = val(p, "MTU").parse() {
                i.mtu = m;
            }
            if p.contains_key("AUTOSTART") {
                i.autostart = val(p, "AUTOSTART") == "*YES";
            }
            if p.contains_key("ALIASNAME") {
                i.alias = val(p, "ALIASNAME").replace("*NONE", "");
            }
            ok("TCP1D04", &format!("Interface {addr}"))
        }
        "RMVTCPIFC" => {
            let before = st.interfaces.len();
            st.interfaces.retain(|i| i.address != addr);
            if st.interfaces.len() == before {
                return Err(msg(Kind::Error, "TCP2658", &[&addr]));
            }
            ok("TCP1D05", &format!("Interface {addr}"))
        }
        "ADDTCPRTE" | "CHGTCPRTE" | "RMVTCPRTE" => {
            let (d, m, h) = (val(p, "RTEDEST"), val(p, "SUBNETMASK"), val(p, "NEXTHOP"));
            let what = format!("Route {d}");
            let at = st
                .routes
                .iter()
                .position(|r| r.dest == d && r.mask == m && r.next_hop == h);
            match (name, at) {
                ("ADDTCPRTE", _) => {
                    st.routes.push(Route {
                        dest: d,
                        mask: m,
                        next_hop: h,
                        interface: val(p, "BINDIFC"),
                        kind: "Static".into(),
                        metric: val(p, "METRIC").parse().unwrap_or(1),
                        active: true,
                    });
                    ok("TCP1D03", &what)
                }
                ("CHGTCPRTE", Some(i)) => {
                    if let Ok(n) = val(p, "METRIC").parse() {
                        st.routes[i].metric = n;
                    }
                    ok("TCP1D04", &what)
                }
                ("RMVTCPRTE", Some(i)) => {
                    st.routes.remove(i);
                    ok("TCP1D05", &what)
                }
                _ => Err(msg(Kind::Error, "TCP1D07", &[&what])),
            }
        }
        "CHGTCPDMN" => {
            let c = &mut st.config;
            if p.contains_key("HOSTNAME") {
                c.host = val(p, "HOSTNAME");
            }
            if p.contains_key("DMNNAME") {
                c.domain = val(p, "DMNNAME");
            }
            if p.contains_key("DMNSCHLIST") {
                c.search = val(p, "DMNSCHLIST");
            }
            if p.contains_key("HOSTSCHPTY") {
                c.priority = val(p, "HOSTSCHPTY");
            }
            if let Some(v) = p.get("INTNETADR") {
                c.dns = elems(v).into_iter().filter(|a| a != "*NONE").collect();
            }
            ok("TCP1A10", "Domain")
        }
        "CHGTCPA" => {
            st.config.forwarding = val(p, "IPDTGFWD") == "*YES";
            Ok(msg(Kind::Done, "TCP1D09", &[]))
        }
        _ => Err(msg(Kind::Error, "CPD0030", &[name])),
    }
}

impl Net for FixtureNet {
    fn config(&self, _user: &str) -> TcpConfig {
        self.state.lock().expect("net").config.clone()
    }

    fn interfaces(&self, _user: &str) -> Vec<Interface> {
        self.state.lock().expect("net").interfaces.clone()
    }

    fn routes(&self, _user: &str) -> Vec<Route> {
        self.state.lock().expect("net").routes.clone()
    }

    fn hosts(&self, _user: &str) -> Vec<HostEntry> {
        let e = |a: &str, n: &[&str]| HostEntry {
            address: a.into(),
            names: n.iter().map(ToString::to_string).collect(),
        };
        vec![
            e("10.0.10.20", &["qshq400", "qshq400.quartz.example"]),
            e("10.0.10.1", &["gw01", "gw01.quartz.example"]),
            e("127.0.0.1", &["loopback", "localhost"]),
        ]
    }

    fn servers(&self, _user: &str) -> Vec<Server> {
        self.state.lock().expect("net").servers.clone()
    }

    fn run(&self, _user: &str, command: &str) -> Result<Message, Message> {
        self.ran.lock().expect("ran").push(command.to_string());
        let (name, p) = parse(command);
        let mut st = self.state.lock().expect("net");
        run_in(&mut st, &name, &p)
    }
}
