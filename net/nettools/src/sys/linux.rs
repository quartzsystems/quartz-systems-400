// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The tools' sockets on Linux. Echo requests go through an ICMP
//! datagram socket (`SOCK_DGRAM`, `IPPROTO_ICMP`/`ICMPV6`), which needs
//! no privilege once `net.ipv4.ping_group_range` admits the group
//! (`slic` opens it to every group at IPL). Route probes are echo
//! requests on the same kind of socket, or UDP datagrams, with a TTL
//! and `IP_RECVERR`: a router's "time exceeded" and the destination's
//! "port unreachable" come back on the error queue with the sender's
//! address. The trace reads an `AF_PACKET` socket bound to the link.

use std::io;
use std::net::{IpAddr, SocketAddr};
use std::os::fd::OwnedFd;
use std::time::{Duration, Instant};

use super::raw;
use crate::cmsg;
use crate::icmp::{self, Echo};
use crate::ping::{Echoer, Reply};
use crate::route::{Answer, Prober};

const IPV6_UNICAST_HOPS: i32 = 16;
const IPV6_RECVHOPLIMIT: i32 = 51;
const IPV6_TCLASS: i32 = 67;

fn ms_until(deadline: Instant) -> i32 {
    let left = deadline.saturating_duration_since(Instant::now());
    i32::try_from(left.as_millis()).unwrap_or(i32::MAX)
}

fn level(v6: bool) -> i32 {
    if v6 { libc::IPPROTO_IPV6 } else { libc::IPPROTO_IP }
}

/// Set the TTL (hop limit) of what `fd` sends.
fn set_ttl(fd: &OwnedFd, v6: bool, ttl: u32) -> io::Result<()> {
    let name = if v6 { IPV6_UNICAST_HOPS } else { libc::IP_TTL };
    raw::set_int(fd, level(v6), name, i32::try_from(ttl).unwrap_or(255))
}

/// An ICMP datagram socket connected to `dest`, sourced from `local`.
fn icmp_socket(dest: IpAddr, local: Option<IpAddr>) -> io::Result<OwnedFd> {
    let v6 = dest.is_ipv6();
    let (domain, proto) = if v6 {
        (libc::AF_INET6, libc::IPPROTO_ICMPV6)
    } else {
        (libc::AF_INET, libc::IPPROTO_ICMP)
    };
    let fd = raw::socket(domain, libc::SOCK_DGRAM, proto)?;
    if let Some(l) = local {
        raw::bind(&fd, &SocketAddr::new(l, 0))?;
    }
    raw::connect(&fd, &SocketAddr::new(dest, 0))?;
    Ok(fd)
}

/// Echo requests to one address.
pub struct Pinger {
    fd: OwnedFd,
    v6: bool,
    dest: IpAddr,
}

impl Pinger {
    /// A socket to `dest` from `local`, with a TTL and type of service
    /// when given.
    pub fn open(dest: IpAddr, local: Option<IpAddr>, ttl: Option<u32>, tos: u8) -> io::Result<Self> {
        let v6 = dest.is_ipv6();
        let fd = icmp_socket(dest, local)?;
        let recv_ttl = if v6 { IPV6_RECVHOPLIMIT } else { libc::IP_RECVTTL };
        raw::set_int(&fd, level(v6), recv_ttl, 1)?;
        if let Some(t) = ttl {
            set_ttl(&fd, v6, t)?;
        }
        if tos != 0 {
            let name = if v6 { IPV6_TCLASS } else { libc::IP_TOS };
            raw::set_int(&fd, level(v6), name, i32::from(tos))?;
        }
        Ok(Self { fd, v6, dest })
    }
}

impl Echoer for Pinger {
    fn send(&mut self, seq: u16, len: usize) -> Result<(), String> {
        let p = Echo::request(self.v6, 0, seq, len).encode();
        raw::send(&self.fd, &p).map(|_| ()).map_err(|e| e.to_string())
    }

    fn recv(&mut self, deadline: Instant) -> Result<Option<Reply>, String> {
        let mut buf = vec![0u8; 65_536];
        let mut ctrl = [0u8; 128];
        loop {
            let ev = raw::poll(&self.fd, libc::POLLIN, ms_until(deadline)).map_err(|e| e.to_string())?;
            if ev == 0 {
                return Ok(None);
            }
            let r = match raw::recvmsg(&self.fd, &mut buf, &mut ctrl, libc::MSG_DONTWAIT) {
                Ok(r) => r,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => continue,
                // An ICMP error for an earlier packet: no reply to it.
                Err(e) if e.raw_os_error().is_some_and(|n| n == libc::EHOSTUNREACH || n == libc::ENETUNREACH) => {
                    continue;
                }
                Err(e) => return Err(e.to_string()),
            };
            if let Some(e) = Echo::decode(&buf[..r.len], self.v6).filter(|e| e.reply) {
                return Ok(Some(Reply {
                    seq: e.seq,
                    from: r.from.unwrap_or(self.dest),
                    ttl: cmsg::ttl(&ctrl[..r.ctrl_len]),
                    len: e.data.len(),
                }));
            }
            if Instant::now() >= deadline {
                return Ok(None);
            }
        }
    }
}

/// How the route probes go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeKind {
    /// Echo requests (`PROBEPCL(*ICMP)`).
    Icmp,
    /// UDP datagrams to `RMTPORT` and up (`PROBEPCL(*UDP)`).
    Udp {
        /// The first port.
        port: u16,
    },
}

/// Route probes to one address.
pub struct RouteProber {
    fd: OwnedFd,
    v6: bool,
    dest: IpAddr,
    local: Option<IpAddr>,
    kind: ProbeKind,
    len: usize,
}

impl RouteProber {
    /// Probes to `dest` with `len` bytes of data each.
    pub fn open(dest: IpAddr, local: Option<IpAddr>, kind: ProbeKind, len: usize) -> io::Result<Self> {
        let v6 = dest.is_ipv6();
        let fd = Self::socket(dest, local, kind, 0)?;
        Ok(Self { fd, v6, dest, local, kind, len })
    }

    fn socket(dest: IpAddr, local: Option<IpAddr>, kind: ProbeKind, n: u32) -> io::Result<OwnedFd> {
        let v6 = dest.is_ipv6();
        let fd = match kind {
            ProbeKind::Icmp => icmp_socket(dest, local)?,
            ProbeKind::Udp { port } => {
                let domain = if v6 { libc::AF_INET6 } else { libc::AF_INET };
                let fd = raw::socket(domain, libc::SOCK_DGRAM, libc::IPPROTO_UDP)?;
                if let Some(l) = local {
                    raw::bind(&fd, &SocketAddr::new(l, 0))?;
                }
                let p = u16::try_from((u32::from(port) + n) % 65_536).unwrap_or(port);
                raw::connect(&fd, &SocketAddr::new(dest, p))?;
                fd
            }
        };
        let recverr = if v6 { cmsg::IPV6_RECVERR } else { cmsg::IP_RECVERR };
        raw::set_int(&fd, level(v6), recverr, 1)?;
        Ok(fd)
    }

    fn answer(&self, ctrl: &[u8], rtt: Duration) -> Option<Answer> {
        let e = cmsg::extended_err(ctrl)?;
        if e.origin != cmsg::ORIGIN_ICMP && e.origin != cmsg::ORIGIN_ICMP6 {
            return None;
        }
        let addr = e.offender.unwrap_or(self.dest);
        Some(match icmp::error_of(self.v6, e.kind, e.code) {
            icmp::Error::TimeExceeded => Answer::From { addr, rtt, done: false, mark: None },
            icmp::Error::PortUnreachable => Answer::From { addr, rtt, done: true, mark: None },
            icmp::Error::Unreachable(c) => Answer::From { addr, rtt, done: false, mark: Some(c) },
            icmp::Error::Other => return None,
        })
    }
}

impl Prober for RouteProber {
    fn probe(&mut self, ttl: u8, n: u32, wait: Duration) -> Result<Answer, String> {
        if let ProbeKind::Udp { .. } = self.kind {
            // A port per probe, as traceroute does, so answers match.
            self.fd = Self::socket(self.dest, self.local, self.kind, n).map_err(|e| e.to_string())?;
        }
        set_ttl(&self.fd, self.v6, u32::from(ttl)).map_err(|e| e.to_string())?;
        let seq = u16::try_from(n % 65_536).unwrap_or(0);
        let data = match self.kind {
            ProbeKind::Icmp => Echo::request(self.v6, 0, seq, self.len).encode(),
            ProbeKind::Udp { .. } => vec![0u8; self.len],
        };
        let start = Instant::now();
        let deadline = start + wait;
        raw::send(&self.fd, &data).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 65_536];
        let mut ctrl = [0u8; 512];
        loop {
            let ev = raw::poll(&self.fd, libc::POLLIN | libc::POLLERR, ms_until(deadline)).map_err(|e| e.to_string())?;
            if ev == 0 {
                return Ok(Answer::Timeout);
            }
            if ev & libc::POLLERR != 0 {
                if let Ok(r) = raw::recvmsg(&self.fd, &mut buf, &mut ctrl, libc::MSG_ERRQUEUE | libc::MSG_DONTWAIT)
                    && let Some(a) = self.answer(&ctrl[..r.ctrl_len], start.elapsed())
                {
                    return Ok(a);
                }
                continue;
            }
            match raw::recvmsg(&self.fd, &mut buf, &mut ctrl, libc::MSG_DONTWAIT) {
                Ok(r) => {
                    if let ProbeKind::Icmp = self.kind
                        && let Some(e) = Echo::decode(&buf[..r.len], self.v6)
                        && e.reply
                        && e.seq == seq
                    {
                        return Ok(Answer::From { addr: r.from.unwrap_or(self.dest), rtt: start.elapsed(), done: true, mark: None });
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {}
                // The error queue holds the detail; read it next turn.
                Err(_) => {}
            }
            if Instant::now() >= deadline {
                return Ok(Answer::Timeout);
            }
        }
    }
}

/// The index of a kernel link.
pub fn ifindex(name: &str) -> io::Result<i32> {
    let s = std::fs::read_to_string(format!("/sys/class/net/{name}/ifindex"))?;
    s.trim()
        .parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("ifindex of {name}: {s:?}")))
}

/// Frames of one link.
pub struct Capture {
    fd: OwnedFd,
}

impl Capture {
    /// Open link `ifname` for capture (needs `CAP_NET_RAW`).
    pub fn open(ifname: &str) -> io::Result<Self> {
        Ok(Self { fd: raw::packet_socket(ifindex(ifname)?)? })
    }

    /// The next frame within `wait`: its length on the wire and whether
    /// it was sent, `None` when nothing came.
    pub fn next(&mut self, buf: &mut [u8], wait: Duration) -> io::Result<Option<(usize, bool)>> {
        let ms = i32::try_from(wait.as_millis()).unwrap_or(i32::MAX);
        if raw::poll(&self.fd, libc::POLLIN, ms)? == 0 {
            return Ok(None);
        }
        raw::packet_recv(&self.fd, buf).map(Some)
    }
}
