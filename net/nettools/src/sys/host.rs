// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Stand-ins off Linux: every open fails with `Unsupported`.

use std::io;
use std::net::IpAddr;
use std::time::{Duration, Instant};

use crate::ping::{Echoer, Reply};
use crate::route::{Answer, Prober};

fn unsupported<T>() -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "network tools need Linux"))
}

/// Echo requests to one address.
pub struct Pinger;

impl Pinger {
    /// Fails off Linux.
    pub fn open(_dest: IpAddr, _local: Option<IpAddr>, _ttl: Option<u32>, _tos: u8) -> io::Result<Self> {
        unsupported()
    }
}

impl Echoer for Pinger {
    fn send(&mut self, _seq: u16, _len: usize) -> Result<(), String> {
        Err("network tools need Linux".into())
    }

    fn recv(&mut self, _deadline: Instant) -> Result<Option<Reply>, String> {
        Ok(None)
    }
}

/// How the route probes go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeKind {
    /// Echo requests.
    Icmp,
    /// UDP datagrams.
    Udp {
        /// The first port.
        port: u16,
    },
}

/// Route probes to one address.
pub struct RouteProber;

impl RouteProber {
    /// Fails off Linux.
    pub fn open(_dest: IpAddr, _local: Option<IpAddr>, _kind: ProbeKind, _len: usize) -> io::Result<Self> {
        unsupported()
    }
}

impl Prober for RouteProber {
    fn probe(&mut self, _ttl: u8, _n: u32, _wait: Duration) -> Result<Answer, String> {
        Err("network tools need Linux".into())
    }
}

/// The index of a kernel link.
pub fn ifindex(_name: &str) -> io::Result<i32> {
    unsupported()
}

/// Frames of one link.
pub struct Capture;

impl Capture {
    /// Fails off Linux.
    pub fn open(_ifname: &str) -> io::Result<Self> {
        unsupported()
    }

    /// Nothing ever comes.
    pub fn next(&mut self, _buf: &mut [u8], _wait: Duration) -> io::Result<Option<(usize, bool)>> {
        Ok(None)
    }
}
