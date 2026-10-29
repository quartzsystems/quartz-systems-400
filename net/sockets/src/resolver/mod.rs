// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The stub resolver: names to addresses and back through the host
//! table and the configured name servers, in the order `HOSTSCHPTY`
//! gives, with the search list, retries and timeouts of `CHGTCPDMN`.
//! Errors are IBM's `h_errno` values ([`HostError`]) and, for
//! `getaddrinfo`, the `EAI_*` numbers.

pub mod codec;
pub mod config;
mod net;
pub mod services;

use std::fmt::Write as _;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::time::{Duration, Instant};

use codec::{Message, Rdata, rcode};
pub use config::{DnsProtocol, DomainConfig, HostConfig, HostEntry, SearchPriority, StaticHostConfig};
pub use services::{DefaultServices, Service, ServicesTable};

use crate::consts::{
    AF_INET, AF_INET6, AF_UNSPEC, AI_ALL, AI_CANONNAME, AI_MASK, AI_NUMERICHOST,
    AI_NUMERICSERV, AI_PASSIVE, AI_V4MAPPED, C_IN, EAI_AGAIN, EAI_BADEXTFLAGS, EAI_BADFLAGS, EAI_FAIL,
    EAI_FAMILY, EAI_MEMORY, EAI_NONAME, EAI_OVERFLOW, EAI_SERVICE, EAI_SOCKTYPE, EAI_SYSTEM, IPPROTO_TCP,
    IPPROTO_UDP, NI_DGRAM, NI_MASK, NI_NAMEREQD, NI_NOFQDN, NI_NUMERICHOST, NI_NUMERICSERV, RES_DEFAULT,
    RES_DEFNAMES, RES_DNSRCH, RES_RECURSE, RES_USEVC, SOCK_DGRAM, SOCK_RAW, SOCK_STREAM, T_A, T_AAAA, T_PTR,
};
use crate::layout::netdb::AddrInfo;
use crate::SockAddr;

/// `h_errno`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostError {
    /// `HOST_NOT_FOUND`.
    NotFound,
    /// `NO_DATA`: the name exists with no address of the kind asked.
    NoData,
    /// `NO_RECOVERY`: a server failed for good.
    NoRecovery,
    /// `TRY_AGAIN`: no server answered in time.
    TryAgain,
}

impl HostError {
    /// IBM's number.
    #[must_use]
    pub fn code(self) -> i32 {
        use crate::consts::{HOST_NOT_FOUND, NO_DATA, NO_RECOVERY, TRY_AGAIN};
        match self {
            HostError::NotFound => HOST_NOT_FOUND,
            HostError::NoData => NO_DATA,
            HostError::NoRecovery => NO_RECOVERY,
            HostError::TryAgain => TRY_AGAIN,
        }
    }

    /// `hstrerror` text.
    #[must_use]
    pub fn text(self) -> &'static str {
        hstrerror(self.code())
    }

    fn eai(self) -> i32 {
        match self {
            HostError::NotFound | HostError::NoData => EAI_NONAME,
            HostError::TryAgain => EAI_AGAIN,
            HostError::NoRecovery => EAI_FAIL,
        }
    }
}

/// `hstrerror()`.
#[must_use]
pub fn hstrerror(code: i32) -> &'static str {
    use crate::consts::{HOST_NOT_FOUND, NO_DATA, NO_RECOVERY, TRY_AGAIN};
    match code {
        0 => "No resolver error has been reported.",
        HOST_NOT_FOUND => "The host name is not known.",
        NO_DATA => "The name is known but has no address of the kind asked for.",
        NO_RECOVERY => "A name server failed and the lookup cannot be completed.",
        TRY_AGAIN => "No name server answered in time; the lookup may work later.",
        _ => "The resolver error is not known.",
    }
}

/// `gai_strerror()`.
#[must_use]
pub fn gai_strerror(code: i32) -> &'static str {
    match code {
        0 => "The address lookup succeeded.",
        EAI_AGAIN => "The name could not be resolved now; try again later.",
        EAI_BADFLAGS => "The flags are not valid.",
        EAI_FAIL => "The name could not be resolved.",
        EAI_FAMILY => "The address family is not supported.",
        EAI_MEMORY => "Storage could not be allocated for the result.",
        EAI_NONAME => "The node or service name is not known.",
        EAI_SERVICE => "The service is not known for the socket type.",
        EAI_SOCKTYPE => "The socket type is not supported.",
        EAI_SYSTEM => "A system error happened; errno has the detail.",
        EAI_OVERFLOW => "A buffer for the result is too small.",
        EAI_BADEXTFLAGS => "The extended flags are not valid.",
        _ => "The address lookup error is not known.",
    }
}

/// A host: what `gethostbyname` and `gethostbyaddr` return.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    /// The official name.
    pub name: String,
    /// Other names.
    pub aliases: Vec<String>,
    /// `AF_INET` or `AF_INET6`.
    pub family: i32,
    /// Addresses, all of `family`.
    pub addrs: Vec<IpAddr>,
}

impl Host {
    /// The addresses as `h_addr_list` holds them.
    #[must_use]
    pub fn addr_bytes(&self) -> Vec<Vec<u8>> {
        self.addrs
            .iter()
            .map(|a| match a {
                IpAddr::V4(v) => v.octets().to_vec(),
                IpAddr::V6(v) => v.octets().to_vec(),
            })
            .collect()
    }
}

/// `getaddrinfo` hints.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hints {
    /// `ai_flags`.
    pub flags: i32,
    /// `ai_family`.
    pub family: i32,
    /// `ai_socktype`.
    pub socktype: i32,
    /// `ai_protocol`.
    pub protocol: i32,
}

/// The resolver.
pub struct Resolver {
    cfg: Box<dyn HostConfig>,
    services: Box<dyn ServicesTable>,
    domain: DomainConfig,
    hosts: Vec<HostEntry>,
    options: i32,
    next_id: u16,
}

impl std::fmt::Debug for Resolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolver")
            .field("domain", &self.domain)
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new(Box::new(StaticHostConfig::default()), Box::new(DefaultServices))
    }
}

fn parse_v4(s: &str) -> Option<Ipv4Addr> {
    s.parse().ok()
}

fn reverse_name(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(v) => {
            let o = v.octets();
            format!("{}.{}.{}.{}.in-addr.arpa", o[3], o[2], o[1], o[0])
        }
        IpAddr::V6(v) => {
            let mut s = String::new();
            for b in v.octets().iter().rev() {
                let _ = write!(s, "{:x}.{:x}.", b & 0x0f, b >> 4);
            }
            s.push_str("ip6.arpa");
            s
        }
    }
}

fn same(a: &str, b: &str) -> bool {
    a.trim_end_matches('.').eq_ignore_ascii_case(b.trim_end_matches('.'))
}

impl Resolver {
    /// A resolver over this configuration, initialized.
    #[must_use]
    pub fn new(cfg: Box<dyn HostConfig>, services: Box<dyn ServicesTable>) -> Self {
        let mut r = Self {
            cfg,
            services,
            domain: DomainConfig::default(),
            hosts: Vec::new(),
            options: RES_DEFAULT,
            next_id: 0x5153,
        };
        r.init();
        r
    }

    /// `res_init()`: read the configuration again.
    pub fn init(&mut self) {
        self.domain = self.cfg.domain();
        self.hosts = self.cfg.hosts();
    }

    /// The configuration in effect.
    #[must_use]
    pub fn domain(&self) -> &DomainConfig {
        &self.domain
    }

    /// `_res.options`.
    #[must_use]
    pub fn options(&self) -> i32 {
        self.options
    }

    /// Set `_res.options`.
    pub fn set_options(&mut self, options: i32) {
        self.options = options;
    }

    /// The services table.
    #[must_use]
    pub fn services(&self) -> &dyn ServicesTable {
        &*self.services
    }

    fn family_ok(family: i32, a: &IpAddr) -> bool {
        match family {
            AF_INET => a.is_ipv4(),
            AF_INET6 => a.is_ipv6(),
            _ => true,
        }
    }

    fn table_by_name(&self, name: &str, family: i32) -> Option<Host> {
        let dom = &self.domain.domain_name;
        let matches = |n: &str| {
            same(n, name)
                || (!dom.is_empty()
                    && (same(&format!("{n}.{dom}"), name) || same(n, &format!("{name}.{dom}"))))
        };
        let hits: Vec<&HostEntry> = self
            .hosts
            .iter()
            .filter(|e| Self::family_ok(family, &e.addr) && e.names.iter().any(|n| matches(n)))
            .collect();
        let first = hits.first()?;
        Some(Host {
            name: first.names[0].clone(),
            aliases: first.names[1..].to_vec(),
            family: if first.addr.is_ipv4() { AF_INET } else { AF_INET6 },
            addrs: hits
                .iter()
                .filter(|e| e.addr.is_ipv4() == first.addr.is_ipv4())
                .map(|e| e.addr)
                .collect(),
        })
    }

    fn table_by_addr(&self, ip: IpAddr) -> Option<Host> {
        let e = self.hosts.iter().find(|e| e.addr == ip)?;
        Some(Host {
            name: e.names[0].clone(),
            aliases: e.names[1..].to_vec(),
            family: if ip.is_ipv4() { AF_INET } else { AF_INET6 },
            addrs: vec![ip],
        })
    }

    /// The names an unqualified or qualified name is tried as, in order.
    fn candidates(&self, name: &str) -> Vec<String> {
        if let Some(n) = name.strip_suffix('.') {
            return vec![n.to_string()];
        }
        let doms = if self.options & RES_DNSRCH != 0 {
            self.domain.domains()
        } else if self.options & RES_DEFNAMES != 0 && !self.domain.domain_name.is_empty() {
            vec![self.domain.domain_name.clone()]
        } else {
            Vec::new()
        };
        let with: Vec<String> = doms.iter().map(|d| format!("{name}.{d}")).collect();
        if name.contains('.') {
            std::iter::once(name.to_string()).chain(with).collect()
        } else {
            with.into_iter().chain(std::iter::once(name.to_string())).collect()
        }
    }

    /// One question to the name servers: the decoded answer and its
    /// bytes, retried across the servers.
    fn ask(&mut self, name: &str, class: u16, qtype: u16) -> Result<(Message, Vec<u8>), HostError> {
        if self.domain.servers.is_empty() {
            return Err(HostError::NotFound);
        }
        self.next_id = self.next_id.wrapping_add(0x2f1b);
        let id = self.next_id;
        let recurse = self.options & RES_RECURSE != 0;
        let query = Message::query(id, name, qtype, class, recurse)
            .encode()
            .map_err(|_| HostError::NoRecovery)?;
        let proto = if self.options & RES_USEVC != 0 {
            DnsProtocol::Tcp
        } else {
            self.domain.protocol
        };
        let start = Instant::now();
        let mut timed_out = false;
        for attempt in 0..=self.domain.retries {
            let wait = self
                .domain
                .initial_query_interval
                .saturating_mul(1 << attempt.min(16));
            for server in self.domain.servers.clone() {
                let left = self.domain.query_timeout.saturating_sub(start.elapsed());
                if left.is_zero() {
                    return Err(HostError::TryAgain);
                }
                let to = SocketAddr::new(server, self.domain.port);
                match net::exchange(to, &query, id, proto, wait.min(left).max(Duration::from_millis(1))) {
                    Ok(bytes) => {
                        let Ok(m) = Message::decode(&bytes) else {
                            continue;
                        };
                        match m.header.rcode {
                            rcode::NOERROR => return Ok((m, bytes)),
                            rcode::NXDOMAIN => return Err(HostError::NotFound),
                            rcode::SERVFAIL => timed_out = true,
                            _ => {}
                        }
                    }
                    Err(net::Failure::Timeout) => timed_out = true,
                    Err(net::Failure::Unreachable) => {}
                }
            }
        }
        Err(if timed_out { HostError::TryAgain } else { HostError::NoRecovery })
    }

    /// Addresses of one fully qualified name: (canonical name, aliases,
    /// addresses).
    fn dns_one(&mut self, name: &str, qtype: u16) -> Result<(String, Vec<String>, Vec<IpAddr>), HostError> {
        let (m, _) = self.ask(name, C_IN, qtype)?;
        let mut current = name.to_string();
        let mut aliases = Vec::new();
        // Follow the CNAME chain the server returned.
        for _ in 0..16 {
            let next = m.answers.iter().find_map(|r| match &r.data {
                Rdata::Cname(t) if same(&r.name, &current) => Some(t.clone()),
                _ => None,
            });
            match next {
                Some(t) => {
                    aliases.push(std::mem::replace(&mut current, t));
                }
                None => break,
            }
        }
        let addrs: Vec<IpAddr> = m
            .answers
            .iter()
            .filter(|r| same(&r.name, &current))
            .filter_map(|r| match r.data {
                Rdata::A(a) if qtype == T_A => Some(IpAddr::V4(a)),
                Rdata::Aaaa(a) if qtype == T_AAAA => Some(IpAddr::V6(a)),
                _ => None,
            })
            .collect();
        if addrs.is_empty() {
            return Err(HostError::NoData);
        }
        Ok((current, aliases, addrs))
    }

    fn dns_by_name(&mut self, name: &str, family: i32) -> Result<Host, HostError> {
        let qtype = if family == AF_INET6 { T_AAAA } else { T_A };
        let mut last = HostError::NotFound;
        for c in self.candidates(name) {
            match self.dns_one(&c, qtype) {
                Ok((canon, aliases, addrs)) => {
                    return Ok(Host {
                        name: canon,
                        aliases,
                        family: if family == AF_INET6 { AF_INET6 } else { AF_INET },
                        addrs,
                    });
                }
                Err(e @ (HostError::NotFound | HostError::NoData)) => {
                    if e == HostError::NoData || last != HostError::NoData {
                        last = e;
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    /// `gethostbyname()` for one family (`AF_INET` or `AF_INET6`).
    pub fn by_name(&mut self, name: &str, family: i32) -> Result<Host, HostError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(HostError::NotFound);
        }
        let literal = match family {
            AF_INET6 => name.parse::<Ipv6Addr>().ok().map(IpAddr::V6),
            _ => parse_v4(name).map(IpAddr::V4),
        };
        if let Some(a) = literal {
            return Ok(Host {
                name: name.to_string(),
                aliases: Vec::new(),
                family: if a.is_ipv4() { AF_INET } else { AF_INET6 },
                addrs: vec![a],
            });
        }
        match self.domain.priority {
            SearchPriority::Local => match self.table_by_name(name, family) {
                Some(h) => Ok(h),
                None => self.dns_by_name(name, family),
            },
            SearchPriority::Remote => match self.dns_by_name(name, family) {
                Ok(h) => Ok(h),
                Err(e) => self.table_by_name(name, family).ok_or(e),
            },
        }
    }

    fn dns_by_addr(&mut self, ip: IpAddr) -> Result<Host, HostError> {
        let (m, _) = self.ask(&reverse_name(ip), C_IN, T_PTR)?;
        let name = m
            .answers
            .iter()
            .find_map(|r| match &r.data {
                Rdata::Ptr(n) => Some(n.clone()),
                _ => None,
            })
            .ok_or(HostError::NoData)?;
        Ok(Host {
            name,
            aliases: Vec::new(),
            family: if ip.is_ipv4() { AF_INET } else { AF_INET6 },
            addrs: vec![ip],
        })
    }

    /// `gethostbyaddr()`.
    pub fn by_addr(&mut self, ip: IpAddr) -> Result<Host, HostError> {
        match self.domain.priority {
            SearchPriority::Local => match self.table_by_addr(ip) {
                Some(h) => Ok(h),
                None => self.dns_by_addr(ip),
            },
            SearchPriority::Remote => match self.dns_by_addr(ip) {
                Ok(h) => Ok(h),
                Err(e) => self.table_by_addr(ip).ok_or(e),
            },
        }
    }

    /// `res_query()`: the answer message's bytes for one name as given.
    pub fn query(&mut self, name: &str, class: u16, qtype: u16) -> Result<Vec<u8>, HostError> {
        let (m, bytes) = self.ask(name.trim_end_matches('.'), class, qtype)?;
        if m.answers.is_empty() {
            return Err(HostError::NoData);
        }
        Ok(bytes)
    }

    /// `res_search()`: [`Resolver::query`] over the search list.
    pub fn search(&mut self, name: &str, class: u16, qtype: u16) -> Result<Vec<u8>, HostError> {
        let mut last = HostError::NotFound;
        for c in self.candidates(name) {
            match self.query(&c, class, qtype) {
                Ok(b) => return Ok(b),
                Err(e @ (HostError::NotFound | HostError::NoData)) => last = e,
                Err(e) => return Err(e),
            }
        }
        Err(last)
    }

    fn port_of(&self, service: &str, hints: &Hints) -> Result<Vec<(i32, i32, u16)>, i32> {
        let kinds: Vec<(i32, i32)> = match hints.socktype {
            SOCK_STREAM => vec![(SOCK_STREAM, IPPROTO_TCP)],
            SOCK_DGRAM => vec![(SOCK_DGRAM, IPPROTO_UDP)],
            SOCK_RAW => vec![(SOCK_RAW, hints.protocol)],
            _ => match hints.protocol {
                IPPROTO_TCP => vec![(SOCK_STREAM, IPPROTO_TCP)],
                IPPROTO_UDP => vec![(SOCK_DGRAM, IPPROTO_UDP)],
                _ => vec![(SOCK_STREAM, IPPROTO_TCP), (SOCK_DGRAM, IPPROTO_UDP)],
            },
        };
        if service.is_empty() {
            return Ok(kinds.into_iter().map(|(t, p)| (t, p, 0)).collect());
        }
        if let Ok(n) = service.parse::<u16>() {
            return Ok(kinds.into_iter().map(|(t, p)| (t, p, n)).collect());
        }
        if hints.flags & AI_NUMERICSERV != 0 {
            return Err(EAI_NONAME);
        }
        if hints.socktype == SOCK_RAW {
            return Err(EAI_SERVICE);
        }
        let out: Vec<(i32, i32, u16)> = kinds
            .into_iter()
            .filter_map(|(t, p)| {
                let proto = if p == IPPROTO_UDP { "udp" } else { "tcp" };
                self.services.by_name(service, Some(proto)).map(|s| (t, p, s.port))
            })
            .collect();
        if out.is_empty() { Err(EAI_SERVICE) } else { Ok(out) }
    }

    /// `getaddrinfo()`: the list, or an `EAI_*` number.
    pub fn getaddrinfo(&mut self, node: Option<&str>, service: Option<&str>, hints: &Hints) -> Result<Vec<AddrInfo>, i32> {
        if hints.flags & !(AI_MASK) != 0 {
            return Err(EAI_BADFLAGS);
        }
        if !matches!(hints.family, AF_UNSPEC | AF_INET | AF_INET6) {
            return Err(EAI_FAMILY);
        }
        if !matches!(hints.socktype, 0 | SOCK_STREAM | SOCK_DGRAM | SOCK_RAW) {
            return Err(EAI_SOCKTYPE);
        }
        let node = node.map(str::trim).filter(|n| !n.is_empty());
        let service = service.map(str::trim).filter(|s| !s.is_empty());
        if node.is_none() && service.is_none() {
            return Err(EAI_NONAME);
        }
        let ports = self.port_of(service.unwrap_or(""), hints)?;
        let mut canon = None;
        let addrs: Vec<IpAddr> = match node {
            None => {
                let passive = hints.flags & AI_PASSIVE != 0;
                let v4 = if passive { Ipv4Addr::UNSPECIFIED } else { Ipv4Addr::LOCALHOST };
                let v6 = if passive { Ipv6Addr::UNSPECIFIED } else { Ipv6Addr::LOCALHOST };
                match hints.family {
                    AF_INET => vec![v4.into()],
                    AF_INET6 => vec![v6.into()],
                    _ => vec![v4.into(), v6.into()],
                }
            }
            Some(n) => {
                if let Some(a) = parse_v4(n) {
                    if hints.family == AF_INET6 {
                        if hints.flags & AI_V4MAPPED == 0 {
                            return Err(EAI_FAMILY);
                        }
                        vec![IpAddr::V6(a.to_ipv6_mapped())]
                    } else {
                        vec![a.into()]
                    }
                } else if let Ok(a) = n.parse::<Ipv6Addr>() {
                    if hints.family == AF_INET {
                        return Err(EAI_FAMILY);
                    }
                    vec![a.into()]
                } else if hints.flags & AI_NUMERICHOST != 0 {
                    return Err(EAI_NONAME);
                } else {
                    let (list, name) = self.addrs_for(n, hints)?;
                    canon = Some(name);
                    list
                }
            }
        };
        let mut out = Vec::new();
        for a in addrs {
            for (socktype, protocol, port) in &ports {
                let addr = match a {
                    IpAddr::V4(v) => SockAddr::V4(SocketAddrV4::new(v, *port)),
                    IpAddr::V6(v) => SockAddr::V6(SocketAddrV6::new(v, *port, 0, 0)),
                };
                out.push(AddrInfo {
                    flags: hints.flags,
                    socktype: *socktype,
                    protocol: *protocol,
                    addr,
                    canonname: None,
                });
            }
        }
        if hints.flags & AI_CANONNAME != 0
            && let Some(first) = out.first_mut()
        {
            first.canonname = Some(canon.unwrap_or_else(|| node.unwrap_or("").to_string()));
        }
        Ok(out)
    }

    fn addrs_for(&mut self, n: &str, hints: &Hints) -> Result<(Vec<IpAddr>, String), i32> {
        let mapped = hints.flags & AI_V4MAPPED != 0;
        match hints.family {
            AF_INET => self.by_name(n, AF_INET).map(|h| (h.addrs, h.name)).map_err(HostError::eai),
            AF_INET6 => {
                let v6 = self.by_name(n, AF_INET6);
                let all = mapped && hints.flags & AI_ALL != 0;
                let v4 = if mapped && (all || v6.is_err()) {
                    self.by_name(n, AF_INET).ok()
                } else {
                    None
                };
                let mut list: Vec<IpAddr> = Vec::new();
                let mut name = None;
                if let Ok(h) = &v6 {
                    list.extend(&h.addrs);
                    name = Some(h.name.clone());
                }
                if let Some(h) = &v4 {
                    list.extend(h.addrs.iter().filter_map(|a| match a {
                        IpAddr::V4(v) => Some(IpAddr::V6(v.to_ipv6_mapped())),
                        IpAddr::V6(_) => None,
                    }));
                    name = name.or_else(|| Some(h.name.clone()));
                }
                match (list.is_empty(), v6) {
                    (false, _) => Ok((list, name.unwrap_or_default())),
                    (true, Err(e)) => Err(e.eai()),
                    (true, Ok(_)) => Err(EAI_NONAME),
                }
            }
            _ => {
                let v4 = self.by_name(n, AF_INET);
                let v6 = self.by_name(n, AF_INET6);
                let mut list = Vec::new();
                let mut name = None;
                for h in [&v4, &v6].into_iter().flatten() {
                    list.extend(&h.addrs);
                    name = name.or_else(|| Some(h.name.clone()));
                }
                if list.is_empty() {
                    let e = v4.err().or(v6.err()).unwrap_or(HostError::NotFound);
                    return Err(e.eai());
                }
                Ok((list, name.unwrap_or_default()))
            }
        }
    }

    /// `getnameinfo()`: (host, service), or an `EAI_*` number.
    pub fn getnameinfo(&mut self, addr: &SockAddr, flags: i32) -> Result<(String, String), i32> {
        if flags & !NI_MASK != 0 {
            return Err(EAI_BADFLAGS);
        }
        let (ip, port) = match addr {
            SockAddr::V4(a) => (IpAddr::V4(*a.ip()), a.port()),
            SockAddr::V6(a) => (IpAddr::V6(*a.ip()), a.port()),
            _ => return Err(EAI_FAMILY),
        };
        let host = if flags & NI_NUMERICHOST != 0 {
            ip.to_string()
        } else {
            match self.by_addr(ip) {
                Ok(h) => {
                    let mut n = h.name;
                    if flags & NI_NOFQDN != 0 {
                        let dom = &self.domain.domain_name;
                        if !dom.is_empty() {
                            let suffix = format!(".{}", dom.to_ascii_lowercase());
                            if n.to_ascii_lowercase().ends_with(&suffix) {
                                n.truncate(n.len() - suffix.len());
                            }
                        }
                    }
                    n
                }
                Err(HostError::TryAgain) if flags & NI_NAMEREQD != 0 => return Err(EAI_AGAIN),
                Err(_) if flags & NI_NAMEREQD != 0 => return Err(EAI_NONAME),
                Err(_) => ip.to_string(),
            }
        };
        let service = if flags & NI_NUMERICSERV != 0 {
            port.to_string()
        } else {
            let proto = if flags & NI_DGRAM != 0 { "udp" } else { "tcp" };
            self.services
                .by_port(port, Some(proto))
                .map_or_else(|| port.to_string(), |s| s.name)
        };
        Ok((host, service))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver(hosts: Vec<HostEntry>, domain: &str) -> Resolver {
        Resolver::new(
            Box::new(StaticHostConfig {
                domain: DomainConfig {
                    domain_name: domain.into(),
                    ..DomainConfig::default()
                },
                hosts,
            }),
            Box::new(DefaultServices),
        )
    }

    fn entry(a: &str, names: &[&str]) -> HostEntry {
        HostEntry {
            addr: a.parse().unwrap(),
            names: names.iter().map(|n| (*n).to_string()).collect(),
        }
    }

    #[test]
    fn host_table_lookups() {
        let mut r = resolver(
            vec![
                entry("10.0.0.5", &["AS400.EXAMPLE.COM", "AS400"]),
                entry("10.0.0.6", &["AS400.EXAMPLE.COM"]),
                entry("fd00::5", &["AS400.EXAMPLE.COM"]),
            ],
            "example.com",
        );
        let h = r.by_name("as400", AF_INET).unwrap();
        assert_eq!(h.name, "AS400.EXAMPLE.COM");
        assert_eq!(h.aliases, vec!["AS400".to_string()]);
        assert_eq!(h.addrs.len(), 2, "every entry of the name");
        assert_eq!(h.addr_bytes()[0], vec![10, 0, 0, 5]);
        assert_eq!(r.by_name("as400.example.com", AF_INET6).unwrap().addrs, vec!["fd00::5".parse::<IpAddr>().unwrap()]);
        assert_eq!(r.by_name("nosuch", AF_INET), Err(HostError::NotFound), "no servers");
        let h = r.by_name("10.1.2.3", AF_INET).unwrap();
        assert_eq!(h.addrs, vec![IpAddr::V4(Ipv4Addr::new(10, 1, 2, 3))]);
        assert_eq!(r.by_addr("10.0.0.6".parse().unwrap()).unwrap().name, "AS400.EXAMPLE.COM");
        let d = Resolver::default().by_name("localhost", AF_INET).unwrap();
        assert_eq!(d.addrs, vec![IpAddr::V4(Ipv4Addr::LOCALHOST)]);
    }

    #[test]
    fn search_candidates() {
        let mut r = resolver(Vec::new(), "example.com");
        assert_eq!(r.candidates("host"), vec!["host.example.com", "host"]);
        assert_eq!(r.candidates("a.b"), vec!["a.b", "a.b.example.com"]);
        assert_eq!(r.candidates("abs."), vec!["abs"]);
        r.set_options(RES_RECURSE);
        assert_eq!(r.candidates("host"), vec!["host"]);
        assert_eq!(reverse_name("192.0.2.1".parse().unwrap()), "1.2.0.192.in-addr.arpa");
        assert!(reverse_name("::1".parse().unwrap()).starts_with("1.0.0.0."));
        assert!(reverse_name("::1".parse().unwrap()).ends_with(".0.ip6.arpa"));
    }

    #[test]
    fn getaddrinfo_rules() {
        let mut r = Resolver::default();
        let h = Hints {
            family: AF_INET,
            socktype: SOCK_STREAM,
            flags: AI_CANONNAME,
            ..Hints::default()
        };
        let l = r.getaddrinfo(Some("localhost"), Some("http"), &h).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].addr, SockAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 80)));
        assert_eq!(l[0].protocol, IPPROTO_TCP);
        assert_eq!(l[0].canonname.as_deref(), Some("LOOPBACK"));
        let any = r.getaddrinfo(None, Some("53"), &Hints { flags: AI_PASSIVE, ..Hints::default() }).unwrap();
        assert_eq!(any.len(), 4, "two families, two socket types");
        assert_eq!(any[0].addr.ip(), Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
        assert_eq!(r.getaddrinfo(None, None, &Hints::default()), Err(EAI_NONAME));
        assert_eq!(r.getaddrinfo(Some("x"), None, &Hints { family: 7, ..Hints::default() }), Err(EAI_FAMILY));
        assert_eq!(r.getaddrinfo(Some("x"), None, &Hints { socktype: 9, ..Hints::default() }), Err(EAI_SOCKTYPE));
        assert_eq!(r.getaddrinfo(Some("x"), None, &Hints { flags: 0x100, ..Hints::default() }), Err(EAI_BADFLAGS));
        assert_eq!(r.getaddrinfo(Some("x"), Some("nosuchsvc"), &Hints::default()), Err(EAI_SERVICE));
        assert_eq!(r.getaddrinfo(Some("x"), Some("http"), &Hints { flags: AI_NUMERICSERV, ..Hints::default() }), Err(EAI_NONAME));
        assert_eq!(r.getaddrinfo(Some("nosuch"), None, &Hints::default()), Err(EAI_NONAME));
        assert_eq!(r.getaddrinfo(Some("nosuch"), None, &Hints { flags: AI_NUMERICHOST, ..Hints::default() }), Err(EAI_NONAME));
        let m = r
            .getaddrinfo(Some("127.0.0.1"), Some("7"), &Hints { family: AF_INET6, flags: AI_V4MAPPED, socktype: SOCK_DGRAM, ..Hints::default() })
            .unwrap();
        assert_eq!(m[0].addr.ip(), Some("::ffff:127.0.0.1".parse().unwrap()));
        assert!(gai_strerror(EAI_NONAME).contains("not known"));
    }

    #[test]
    fn getnameinfo_rules() {
        let mut r = Resolver::default();
        let a = SockAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 23));
        assert_eq!(r.getnameinfo(&a, 0).unwrap(), ("LOOPBACK".into(), "telnet".into()));
        assert_eq!(r.getnameinfo(&a, NI_NUMERICHOST | NI_NUMERICSERV).unwrap(), ("127.0.0.1".into(), "23".into()));
        let b = SockAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 0, 2, 9), 7));
        assert_eq!(r.getnameinfo(&b, NI_DGRAM).unwrap(), ("192.0.2.9".into(), "echo".into()));
        assert_eq!(r.getnameinfo(&b, NI_NAMEREQD), Err(EAI_NONAME));
        assert_eq!(r.getnameinfo(&SockAddr::Unix("/x".into()), 0), Err(EAI_FAMILY));
        assert_eq!(HostError::TryAgain.code(), 20);
        assert!(!hstrerror(5).is_empty());
    }
}
