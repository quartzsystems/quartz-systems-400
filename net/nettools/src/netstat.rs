// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `NETSTAT` views as panel data: the interfaces (options 1 and 4),
//! the routes (2 and 5), the connections (3 and 6), their option 5
//! displays, the jobs of a connection (option 8) and the stack status
//! (10). The storage service gathers what the kernel and the TCP/IP
//! configuration say into the plain records here; the functions name
//! the variables of the `QTOCNETS` panel group and format the values
//! as the oracle's displays show them.

use std::fmt::Write as _;
use std::net::{IpAddr, SocketAddr};

/// One list entry: `(variable, value)` pairs.
pub type Row = Vec<(String, String)>;

/// A panel's variables and its named lists.
pub type Data = (Vec<(String, String)>, Vec<(String, Vec<Row>)>);

fn row(pairs: &[(&str, String)]) -> Row {
    pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
}

/// An interface, configured or found on a kernel link.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ifc {
    /// The address; `None` for `*IP4DHCP` (and `*IP6SAC`) entries.
    pub addr: Option<IpAddr>,
    /// The special value naming an entry without an address.
    pub special: String,
    /// The prefix length.
    pub prefix: u8,
    /// The line description (`*LOOPBACK`, `*VIRTUALIP`, a line).
    pub line: String,
    /// The line's type (`*ELAN`, `*NONE`, ...).
    pub line_type: String,
    /// `Active`, `Inactive`, `Starting`, `Failed`, ...
    pub status: String,
    /// The alias name, or empty.
    pub alias: String,
    /// The host name the address has in the host table, or empty.
    pub host: String,
    /// The MTU.
    pub mtu: u32,
    /// Starts with TCP/IP.
    pub autostart: bool,
    /// Made by DHCP.
    pub dhcp: bool,
    /// The VLAN identifier.
    pub vlan: Option<u16>,
    /// Associated local interface.
    pub local_ifc: String,
    /// Type of service.
    pub tos: String,
    /// Text.
    pub text: String,
    /// When the status last changed, as `MM/DD/YY  HH:MM:SS`.
    pub changed: String,
}

impl Ifc {
    /// The key option 5 passes back.
    pub fn key(&self) -> String {
        self.addr.map_or_else(|| self.special.clone(), |a| a.to_string())
    }
}

fn mask_of(prefix: u8, v6: bool) -> IpAddr {
    if v6 {
        let bits = if prefix == 0 { 0 } else { u128::MAX << (128 - u32::from(prefix.min(128))) };
        IpAddr::V6(bits.into())
    } else {
        let bits = if prefix == 0 { 0 } else { u32::MAX << (32 - u32::from(prefix.min(32))) };
        IpAddr::V4(bits.into())
    }
}

fn and(a: IpAddr, m: IpAddr, invert: bool) -> IpAddr {
    match (a, m) {
        (IpAddr::V4(a), IpAddr::V4(m)) => {
            let m = u32::from(m);
            IpAddr::V4((u32::from(a) & if invert { !m } else { m }).into())
        }
        (IpAddr::V6(a), IpAddr::V6(m)) => {
            let m = u128::from(m);
            IpAddr::V6((u128::from(a) & if invert { !m } else { m }).into())
        }
        _ => a,
    }
}

/// Option 1's entries (IPv4) or option 4's (IPv6).
pub fn ifcs(list: &[Ifc], v6: bool) -> Vec<Row> {
    let mut v: Vec<&Ifc> = list
        .iter()
        .filter(|i| i.addr.map_or(i.special.contains('6'), |a| a.is_ipv6()) == v6)
        .collect();
    v.sort_by(|a, b| {
        let k = |i: &Ifc| (i.addr.is_some(), i.addr);
        k(a).cmp(&k(b))
    });
    v.into_iter()
        .map(|i| {
            let net = i.addr.map_or_else(|| "*".into(), |a| and(a, mask_of(i.prefix, v6), false).to_string());
            row(&[
                ("IADR", i.key()),
                ("INET", net),
                ("IPFX", i.prefix.to_string()),
                ("ILIND", i.line.clone()),
                ("ISTS", i.status.clone()),
                ("IVLAN", i.vlan.map_or_else(|| "*NONE".into(), |v| v.to_string())),
            ])
        })
        .collect()
}

/// Option 5 on an interface.
pub fn ifc_detail(i: &Ifc) -> Vec<(String, String)> {
    let v6 = i.addr.is_some_and(|a| a.is_ipv6());
    let (mask, net, host, bcast) = match i.addr {
        Some(a) => {
            let m = mask_of(i.prefix, v6);
            let bcast = match (a, m) {
                (IpAddr::V4(a4), IpAddr::V4(m4)) if i.prefix < 31 && !a4.is_loopback() => {
                    std::net::Ipv4Addr::from(u32::from(a4) | !u32::from(m4)).to_string()
                }
                _ => "*NONE".into(),
            };
            (m.to_string(), and(a, m, false).to_string(), and(a, m, true).to_string(), bcast)
        }
        None => ("*".into(), "*".into(), "*".into(), "*NONE".into()),
    };
    let host_name = if !i.host.is_empty() {
        i.host.clone()
    } else if !i.alias.is_empty() {
        i.alias.clone()
    } else {
        "*NONE".into()
    };
    row(&[
        ("DHOST", host_name),
        ("DADR", i.key()),
        ("DMASK", if v6 { i.prefix.to_string() } else { mask }),
        ("DNET", net),
        ("DHOSTA", host),
        ("DBCAST", bcast),
        ("DSTS", i.status.clone()),
        ("DCHG", i.changed.clone()),
        ("DLIND", i.line.clone()),
        ("DVLAN", i.vlan.map_or_else(|| "*NONE".into(), |v| v.to_string())),
        ("DLTYPE", i.line_type.clone()),
        ("DLCLIFC", if i.local_ifc.is_empty() { "*NONE".into() } else { i.local_ifc.clone() }),
        ("DTOS", if i.tos.is_empty() { "*NORMAL".into() } else { i.tos.clone() }),
        ("DMTU", i.mtu.to_string()),
        ("DAUTO", yes(i.autostart)),
        ("DDHCP", yes(i.dhcp)),
        ("DALIAS", if i.alias.is_empty() { "*NONE".into() } else { i.alias.clone() }),
        ("DTEXT", if i.text.is_empty() { "*NONE".into() } else { i.text.clone() }),
    ])
}

fn yes(b: bool) -> String {
    if b { "*YES".into() } else { "*NO".into() }
}

/// A route in the kernel's main table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rte {
    /// The destination; `None` is the default route.
    pub dest: Option<IpAddr>,
    /// The destination's prefix length.
    pub prefix: u8,
    /// The next hop; `None` is `*DIRECT`.
    pub hop: Option<IpAddr>,
    /// The route's MTU.
    pub mtu: u32,
    /// The kernel can use it (its link is up).
    pub available: bool,
    /// Configured (`ADDTCPRTE`) rather than made for an interface.
    pub configured: bool,
    /// Type of service.
    pub tos: String,
    /// Duplicate route priority.
    pub dup_pty: u8,
    /// Text.
    pub text: String,
    /// The interface it goes out of (its address), when known.
    pub ifc: Option<IpAddr>,
    /// The order the kernel uses it in among equal routes.
    pub order: u32,
}

impl Rte {
    /// `*DFTROUTE`, or the destination.
    pub fn dest_text(&self) -> String {
        self.dest.map_or_else(|| "*DFTROUTE".into(), |d| d.to_string())
    }

    /// The route type: `*DFTROUTE`, `*DIRECT`, `*HOST` or `*NET`.
    pub fn kind(&self) -> &'static str {
        let host = self.dest.is_some_and(|d| u32::from(self.prefix) == if d.is_ipv6() { 128 } else { 32 });
        match (self.dest, self.hop) {
            (None, _) => "*DFTROUTE",
            (Some(_), None) => "*DIRECT",
            (Some(_), Some(_)) if host => "*HOST",
            _ => "*NET",
        }
    }

    /// The key option 5 passes back: destination, prefix, next hop.
    pub fn key(&self) -> String {
        format!(
            "{}/{}/{}",
            self.dest_text(),
            self.prefix,
            self.hop.map_or_else(|| "*DIRECT".into(), |h| h.to_string())
        )
    }
}

fn sort_routes(list: &[Rte], v6: bool) -> Vec<&Rte> {
    let mut v: Vec<&Rte> = list
        .iter()
        .filter(|r| r.dest.or(r.hop).is_some_and(|a| a.is_ipv6()) == v6)
        .collect();
    // Destinations in address order, the default route last.
    v.sort_by_key(|r| (r.dest.is_none(), r.dest, r.prefix));
    v
}

/// Option 2's entries (IPv4) or option 5's (IPv6).
pub fn rtes(list: &[Rte], v6: bool) -> Vec<Row> {
    sort_routes(list, v6)
        .into_iter()
        .map(|r| {
            let mask = match r.dest {
                None => "*NONE".into(),
                Some(_) if v6 => r.prefix.to_string(),
                Some(_) => mask_of(r.prefix, false).to_string(),
            };
            row(&[
                ("RDEST", r.dest_text()),
                ("RMASK", mask),
                ("RHOP", r.hop.map_or_else(|| "*DIRECT".into(), |h| h.to_string())),
                ("RAVAIL", yes(r.available)),
                ("RSTS", if r.available { "*ACTIVE".into() } else { "*INACTIVE".into() }),
                ("RTOS", if r.tos.is_empty() { "*NORMAL".into() } else { r.tos.clone() }),
                ("RMTU", r.mtu.to_string()),
                ("RTYPE", r.kind().into()),
                ("RSRC", "*CFG".into()),
                ("RKEY", r.key()),
            ])
        })
        .collect()
}

/// Option 5 on a route.
pub fn rte_detail(r: &Rte) -> Vec<(String, String)> {
    let v6 = r.dest.or(r.hop).is_some_and(|a| a.is_ipv6());
    row(&[
        ("DDEST", r.dest_text()),
        ("DMASK", if r.dest.is_none() { "*NONE".into() } else if v6 { r.prefix.to_string() } else { mask_of(r.prefix, false).to_string() }),
        ("DORDER", r.order.max(1).to_string()),
        ("DHOP", r.hop.map_or_else(|| "*DIRECT".into(), |h| h.to_string())),
        ("DTOS", if r.tos.is_empty() { "*NORMAL".into() } else { r.tos.clone() }),
        ("DAVAIL", yes(r.available)),
        ("DTYPE", r.kind().into()),
        ("DSRC", "*CFG".into()),
        ("DCHG", String::new()),
        ("DMTU", r.mtu.to_string()),
        ("DDUP", r.dup_pty.to_string()),
        ("DIFC", r.ifc.map_or_else(|| "*NONE".into(), |a| a.to_string())),
        ("DTEXT", if r.text.is_empty() { "*NONE".into() } else { r.text.clone() }),
    ])
}

/// TCP counters for a connection's option 5.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Bytes sent.
    pub bytes_out: u64,
    /// Bytes received.
    pub bytes_in: u64,
    /// Bytes queued to send.
    pub out_buffered: u64,
    /// Bytes queued to read.
    pub in_buffered: u64,
    /// Smoothed round trip, microseconds.
    pub rtt_us: u32,
    /// Its variance, microseconds.
    pub rttvar_us: u32,
    /// Retransmissions in all.
    pub retrans: u32,
    /// Retransmissions of the segment now waiting.
    pub cur_retrans: u32,
    /// Congestion window in bytes.
    pub cwnd: u64,
    /// Maximum segment size.
    pub mss: u32,
}

/// A TCP or UDP socket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conn {
    /// TCP rather than UDP.
    pub tcp: bool,
    /// Local address and port.
    pub local: SocketAddr,
    /// Remote address and port (unspecified for a listener).
    pub remote: SocketAddr,
    /// The TCP state, as IBM names it (`Listen`, `Established`, ...).
    pub state: String,
    /// Milliseconds since the last data either way.
    pub idle_ms: u64,
    /// The user profile of the job holding it.
    pub user: String,
    /// The jobs holding it: `(name, user, number, type, current user)`.
    pub jobs: Vec<(String, String, String, String, String)>,
    /// TCP counters.
    pub counters: Counters,
    /// Opened by `accept` (passive) rather than `connect`.
    pub passive: bool,
}

impl Conn {
    /// The key the options pass back: protocol, local, remote.
    pub fn key(&self) -> String {
        format!("{}|{}|{}", if self.tcp { "T" } else { "U" }, self.local, self.remote)
    }

    /// On an IPv4 address (an IPv4-mapped one included).
    fn v4(&self) -> bool {
        match self.local.ip() {
            IpAddr::V4(_) => true,
            IpAddr::V6(a) => a.to_ipv4_mapped().is_some(),
        }
    }

    /// Bound to every IPv6 address, which takes IPv4 traffic as well.
    fn dual(&self) -> bool {
        self.local.ip().is_ipv6() && self.local.ip().is_unspecified()
    }
}

/// The IBM name of a kernel TCP state number.
pub fn state_name(n: u8) -> &'static str {
    match n {
        1 => "Established",
        2 => "SYN-sent",
        3 | 12 => "SYN-received",
        4 => "FIN-wait-1",
        5 => "FIN-wait-2",
        6 => "Time-wait",
        8 => "Close-wait",
        9 => "Last-ACK",
        10 => "Listen",
        11 => "Closing",
        _ => "Closed",
    }
}

/// A value cut to `w` columns as the lists cut it: the first `w - 2`
/// characters and ` >`.
pub fn cut(s: &str, w: usize) -> String {
    if s.chars().count() <= w {
        s.to_string()
    } else {
        format!("{} >", s.chars().take(w.saturating_sub(2)).collect::<String>())
    }
}

/// An idle time as the list shows it: `hhh:mm:ss`.
pub fn idle(ms: u64) -> String {
    let s = ms / 1000;
    format!("{:03}:{:02}:{:02}", (s / 3600).min(999), s / 60 % 60, s % 60)
}

/// An idle time as option 5 shows it: `hhh:mm:ss.mmm`.
pub fn idle_detail(ms: u64) -> String {
    format!("{}.{:03}", idle(ms), ms % 1000)
}

fn addr_or_star(a: IpAddr) -> String {
    let a = match a {
        IpAddr::V6(v) => v.to_ipv4_mapped().map_or(a, IpAddr::V4),
        IpAddr::V4(_) => a,
    };
    if a.is_unspecified() { "*".into() } else { a.to_string() }
}

fn port_or_star(p: u16) -> String {
    if p == 0 { "*".into() } else { p.to_string() }
}

/// Option 3's entries (IPv4, which includes IPv4 traffic on IPv6
/// sockets) or option 6's (IPv6). `name` gives a port's service name
/// (`tcp` true for TCP); `w` is the port column's width.
pub fn conns(list: &[Conn], v6: bool, name: &dyn Fn(u16, bool) -> Option<String>) -> Vec<Row> {
    let w = if v6 { 7 } else { 9 };
    // A socket on every IPv6 address shows in the IPv4 list too, unless
    // an IPv4 socket has its port.
    let shown = |c: &Conn| {
        if v6 {
            !c.v4()
        } else {
            c.v4() || (c.dual() && !list.iter().any(|o| o.tcp == c.tcp && o.v4() && o.local.port() == c.local.port()))
        }
    };
    let mut v: Vec<&Conn> = list.iter().filter(|c| shown(c)).collect();
    v.sort_by_key(|c| (c.local.port(), !c.tcp, c.remote.ip(), c.remote.port()));
    v.into_iter()
        .map(|c| {
            let pname = |p: u16| {
                if p == 0 {
                    "*".to_string()
                } else {
                    cut(&name(p, c.tcp).unwrap_or_else(|| p.to_string()), w)
                }
            };
            row(&[
                ("CRADR", addr_or_star(c.remote.ip())),
                ("CRPORT", pname(c.remote.port())),
                ("CLPORT", pname(c.local.port())),
                ("CIDLE", idle(c.idle_ms)),
                ("CSTATE", if c.tcp { c.state.clone() } else { "*UDP".into() }),
                ("CUSER", c.user.clone()),
                ("COUT", c.counters.bytes_out.to_string()),
                ("CIN", c.counters.bytes_in.to_string()),
                ("CLADR", addr_or_star(c.local.ip())),
                ("CRPORTN", port_or_star(c.remote.port())),
                ("CLPORTN", port_or_star(c.local.port())),
                ("CTYPE", if c.tcp { "*TCP".into() } else { "*UDP".into() }),
                ("CKEY", c.key()),
            ])
        })
        .collect()
}

/// Option 5 on a connection. `names` gives a host name for an address;
/// `ports` the service name of a port; `when` formats the last
/// activity as `MM/DD/YY  HH:MM:SS`.
pub fn conn_detail(
    c: &Conn,
    names: &dyn Fn(IpAddr) -> Option<String>,
    ports: &dyn Fn(u16, bool) -> Option<String>,
    when: &str,
) -> Vec<(String, String)> {
    let host = |a: IpAddr| if a.is_unspecified() { String::new() } else { names(a).unwrap_or_default() };
    let port = |p: u16| {
        if p == 0 {
            "*".to_string()
        } else {
            ports(p, c.tcp).unwrap_or_else(|| p.to_string())
        }
    };
    let k = &c.counters;
    let secs = |us: u32| format!("{}.{:03}", us / 1_000_000, us / 1000 % 1000);
    row(&[
        ("DRHOST", host(c.remote.ip())),
        ("DRADR", addr_or_star(c.remote.ip())),
        ("DRPORT", port(c.remote.port())),
        ("DLHOST", host(c.local.ip())),
        ("DLADR", addr_or_star(c.local.ip())),
        ("DLPORT", port(c.local.port())),
        ("DUSER", c.user.clone()),
        ("DSTATE", c.state.clone()),
        ("DOPEN", if c.passive { "Passive".into() } else { "Active".into() }),
        ("DIDLE", idle_detail(c.idle_ms)),
        ("DLAST", when.to_string()),
        ("DRTT", secs(k.rtt_us)),
        ("DRTTV", secs(k.rttvar_us)),
        ("DBOUT", k.bytes_out.to_string()),
        ("DOBUF", k.out_buffered.to_string()),
        ("DUSNDNXT", "0".into()),
        ("DSNDNXT", "0".into()),
        ("DSNDUNA", "0".into()),
        ("DOPUSH", "0".into()),
        ("DOURG", "0".into()),
        ("DOWIN", "0".into()),
        ("DBIN", k.bytes_in.to_string()),
        ("DIBUF", k.in_buffered.to_string()),
        ("DRCVNXT", "0".into()),
        ("DURCVNXT", "0".into()),
        ("DIPUSH", "0".into()),
        ("DIURG", "0".into()),
        ("DIWIN", "0".into()),
        ("DRETR", k.retrans.to_string()),
        ("DCRETR", k.cur_retrans.to_string()),
        ("DSWMAX", "0".into()),
        ("DSWCUR", "0".into()),
        ("DSWUPD", "0".into()),
        ("DSWACK", "0".into()),
        ("DCWND", k.cwnd.to_string()),
        ("DSSTH", "0".into()),
        ("DMSS", k.mss.to_string()),
        ("DPREC", "0".into()),
        ("DISS", "0".into()),
        ("DIRS", "0".into()),
    ])
}

/// Option 8 on a connection: the connection's identity and its jobs.
pub fn conn_jobs(c: &Conn) -> Data {
    let vars = row(&[
        ("JCTYPE", if c.tcp { "*TCP".into() } else { "*UDP".into() }),
        ("JLADR", addr_or_star(c.local.ip())),
        ("JLPORT", port_or_star(c.local.port())),
        ("JRADR", addr_or_star(c.remote.ip())),
        ("JRPORT", port_or_star(c.remote.port())),
    ]);
    let jobs = c
        .jobs
        .iter()
        .map(|(n, u, nbr, t, cur)| {
            row(&[
                ("JNAME", n.clone()),
                ("JUSER", u.clone()),
                ("JNBR", nbr.clone()),
                ("JTYPE", t.clone()),
                ("JCUR", cur.clone()),
            ])
        })
        .collect();
    (vars, vec![("JOBS".into(), jobs)])
}

/// The stack's status for option 10.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Stack {
    /// IPv4 is up.
    pub v4: bool,
    /// IPv6 is up.
    pub v6: bool,
    /// Seconds active.
    pub active_secs: u64,
    /// When last started, `MM/DD/YY  HH:MM:SS`.
    pub started: String,
    /// When last ended.
    pub ended: String,
    /// Who started it: name, user, number.
    pub starter: (String, String, String),
    /// Who ended it.
    pub ender: (String, String, String),
}

/// Option 10.
pub fn status(s: &Stack) -> Vec<(String, String)> {
    let a = |b: bool| if b { "Active".to_string() } else { "Inactive".to_string() };
    row(&[
        ("S4", a(s.v4)),
        ("S6", a(s.v6)),
        ("SLMT", "*NO".into()),
        ("SACT", s.active_secs.to_string()),
        ("SSTR", s.started.clone()),
        ("SEND", s.ended.clone()),
        ("SWSJ", s.starter.0.clone()),
        ("SWSU", s.starter.1.clone()),
        ("SWSN", s.starter.2.clone()),
        ("SWEJ", s.ender.0.clone()),
        ("SWEU", s.ender.1.clone()),
        ("SWEN", s.ender.2.clone()),
    ])
}

/// The text form of a list, for a job without a display: a heading
/// line, then a line per entry with `cols`' values.
pub fn text(title: &str, rows: &[Row], cols: &[(&str, &str, usize)]) -> Vec<String> {
    // A column is as wide as its heading at least.
    let cols: Vec<(&str, &str, usize)> = cols.iter().map(|(k, h, w)| (*k, *h, (*w).max(h.len()))).collect();
    let mut out = vec![title.to_string()];
    let head = cols.iter().fold(String::new(), |mut s, (_, h, w)| {
        let _ = write!(s, "{h:<w$}  ");
        s
    });
    out.push(head.trim_end().to_string());
    for r in rows {
        let line = cols.iter().fold(String::new(), |mut s, (k, _, w)| {
            let v = r.iter().find(|(n, _)| n == k).map_or("", |(_, v)| v.as_str());
            let _ = write!(s, "{v:<w$}  ");
            s
        });
        out.push(line.trim_end().to_string());
    }
    if rows.is_empty() {
        out.push("(none)".into());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val(r: &[(String, String)], k: &str) -> String {
        r.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone()).unwrap_or_default()
    }

    fn loopback() -> Ifc {
        Ifc {
            addr: Some("127.0.0.1".parse().unwrap()),
            prefix: 8,
            line: "*LOOPBACK".into(),
            line_type: "*NONE".into(),
            status: "Active".into(),
            alias: "LOCALHOST".into(),
            host: "LOOPBACK".into(),
            mtu: 576,
            autostart: true,
            ..Ifc::default()
        }
    }

    #[test]
    fn interfaces_as_option_1() {
        let dhcp = Ifc { special: "*IP4DHCP".into(), line: "LINETH".into(), status: "Inactive".into(), ..Ifc::default() };
        let eth = Ifc {
            addr: Some("185.113.5.134".parse().unwrap()),
            prefix: 24,
            line: "LINETH".into(),
            status: "Active".into(),
            ..Ifc::default()
        };
        let v6 = Ifc { addr: Some("::1".parse().unwrap()), prefix: 128, line: "*LOOPBACK".into(), ..Ifc::default() };
        let rows = ifcs(&[eth, loopback(), dhcp, v6], false);
        let got: Vec<(String, String)> = rows.iter().map(|r| (val(r, "IADR"), val(r, "INET"))).collect();
        assert_eq!(
            got,
            [
                ("*IP4DHCP".to_string(), "*".to_string()),
                ("127.0.0.1".into(), "127.0.0.0".into()),
                ("185.113.5.134".into(), "185.113.5.0".into())
            ]
        );
        assert_eq!(val(&rows[1], "IVLAN"), "*NONE");
        let d = ifc_detail(&loopback());
        assert_eq!(val(&d, "DMASK"), "255.0.0.0");
        assert_eq!(val(&d, "DHOSTA"), "0.0.0.1");
        assert_eq!(val(&d, "DBCAST"), "*NONE");
        assert_eq!(val(&d, "DHOST"), "LOOPBACK");
        let e = ifc_detail(&Ifc { addr: Some("10.0.2.15".parse().unwrap()), prefix: 24, ..Ifc::default() });
        assert_eq!(val(&e, "DBCAST"), "10.0.2.255");
    }

    #[test]
    fn routes_default_last() {
        let list = [
            Rte { dest: None, hop: Some("185.113.5.253".parse().unwrap()), mtu: 1500, available: true, ..Rte::default() },
            Rte { dest: Some("127.0.0.0".parse().unwrap()), prefix: 8, mtu: 576, available: true, ..Rte::default() },
            Rte { dest: Some("::1".parse().unwrap()), prefix: 128, ..Rte::default() },
        ];
        let r = rtes(&list, false);
        assert_eq!(val(&r[0], "RDEST"), "127.0.0.0");
        assert_eq!(val(&r[0], "RMASK"), "255.0.0.0");
        assert_eq!(val(&r[0], "RHOP"), "*DIRECT");
        assert_eq!(val(&r[0], "RTYPE"), "*DIRECT");
        assert_eq!(val(&r[1], "RDEST"), "*DFTROUTE");
        assert_eq!(val(&r[1], "RMASK"), "*NONE");
        assert_eq!(val(&r[1], "RTYPE"), "*DFTROUTE");
        let r6 = rtes(&list, true);
        assert_eq!(r6.len(), 1);
        assert_eq!(val(&r6[0], "RMASK"), "128");
        assert_eq!(val(&r6[0], "RSTS"), "*INACTIVE");
        assert_eq!(val(&rte_detail(&list[0]), "DORDER"), "1");
    }

    #[test]
    fn connections_cut_their_port_names() {
        let names = |p: u16, _: bool| match p {
            21 => Some("ftp-control".to_string()),
            23 => Some("telnet".to_string()),
            _ => None,
        };
        let l = |p: u16, state: &str| Conn {
            tcp: true,
            local: SocketAddr::new("0.0.0.0".parse().unwrap(), p),
            remote: "0.0.0.0:0".parse().unwrap(),
            state: state.into(),
            idle_ms: 3_723_000,
            user: "QTCP".into(),
            jobs: Vec::new(),
            counters: Counters::default(),
            passive: true,
        };
        let six = Conn { local: "[::]:23".parse().unwrap(), remote: "[::]:0".parse().unwrap(), ..l(23, "Listen") };
        let udp = Conn { tcp: false, ..l(123, "Closed") };
        let rows = conns(&[l(23, "Listen"), l(21, "Listen"), udp, six.clone()], false, &names);
        assert_eq!(rows.len(), 3, "the IPv6 telnet socket stays out: an IPv4 one has port 23");
        assert_eq!(conns(&[six], false, &names).len(), 1, "alone, it shows");
        assert_eq!(val(&rows[0], "CLPORT"), "ftp-con >");
        assert_eq!(val(&rows[0], "CRADR"), "*");
        assert_eq!(val(&rows[0], "CRPORT"), "*");
        assert_eq!(val(&rows[0], "CIDLE"), "001:02:03");
        assert_eq!(val(&rows[1], "CLPORT"), "telnet");
        assert_eq!(val(&rows[2], "CSTATE"), "*UDP");
        assert_eq!(val(&rows[2], "CLPORTN"), "123");
        let rows6 = conns(&[l(21, "Listen"), Conn { local: "[::]:21".parse().unwrap(), remote: "[::]:0".parse().unwrap(), ..l(21, "Listen") }], true, &names);
        assert_eq!(rows6.len(), 1);
        assert_eq!(val(&rows6[0], "CLPORT"), "ftp-c >");
    }

    #[test]
    fn mapped_addresses_count_as_ipv4() {
        let conn = Conn {
            tcp: true,
            local: "[::ffff:10.0.2.15]:23".parse().unwrap(),
            remote: "[::ffff:10.0.2.2]:40000".parse().unwrap(),
            state: state_name(1).into(),
            idle_ms: 0,
            user: "QTCP".into(),
            jobs: vec![("QTVTELNET".into(), "QTCP".into(), "000123".into(), "*BCH".into(), "QTCP".into())],
            counters: Counters { bytes_out: 10, bytes_in: 20, rtt_us: 1_500, ..Counters::default() },
            passive: true,
        };
        let rows = conns(std::slice::from_ref(&conn), false, &|_, _| None);
        assert_eq!(val(&rows[0], "CRADR"), "10.0.2.2");
        assert_eq!(val(&rows[0], "CSTATE"), "Established");
        let detail = conn_detail(&conn, &|_| Some("GATEWAY".into()), &|_, _| None, "10/30/26  12:00:00");
        assert_eq!(val(&detail, "DRHOST"), "GATEWAY");
        assert_eq!(val(&detail, "DRTT"), "0.001");
        assert_eq!(val(&detail, "DOPEN"), "Passive");
        let (vars, lists) = conn_jobs(&conn);
        assert_eq!(val(&vars, "JLPORT"), "23");
        assert_eq!(lists[0].1[0][0].1, "QTVTELNET");
    }

    #[test]
    fn small_formats() {
        assert_eq!(cut("as-svrmap", 9), "as-svrmap");
        assert_eq!(cut("telnet-ssl", 7), "telne >");
        assert_eq!(idle_detail(61_005), "000:01:01.005");
        assert_eq!(state_name(10), "Listen");
        let t = text("Routes", &[row(&[("A", "x".into())])], &[("A", "Dest", 4)]);
        assert_eq!(t, ["Routes", "Dest", "x"]);
    }
}
