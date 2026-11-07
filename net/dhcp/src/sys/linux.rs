// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The socket on Linux, through socket2 (no `unsafe` here).

use std::io::{self, Read as _};
use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::Duration;

use socket2::{Domain, Protocol, SockAddr, Socket as Raw, Type};

use crate::client::Dest;
use crate::packet::{CLIENT_PORT, SERVER_PORT};

/// UDP 68 on one link.
#[derive(Debug)]
pub struct Socket {
    raw: Raw,
}

impl Socket {
    /// Open it on `link` (`eth0`).
    pub fn open(link: &str) -> io::Result<Self> {
        let raw = Raw::new(Domain::IPV4, Type::DGRAM.cloexec(), Some(Protocol::UDP))?;
        raw.set_reuse_address(true)?;
        raw.set_broadcast(true)?;
        raw.bind_device(Some(link.as_bytes()))?;
        raw.bind(&SockAddr::from(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            CLIENT_PORT,
        )))?;
        Ok(Self { raw })
    }

    /// Send `bytes` to port 67.
    pub fn send(&self, to: Dest, bytes: &[u8]) -> io::Result<()> {
        let ip = match to {
            Dest::Broadcast => Ipv4Addr::BROADCAST,
            Dest::Unicast(a) => a,
        };
        self.raw
            .send_to(bytes, &SockAddr::from(SocketAddrV4::new(ip, SERVER_PORT)))?;
        Ok(())
    }

    /// One datagram, waiting at most `wait`; `None` when none came.
    pub fn recv(&self, wait: Duration) -> io::Result<Option<Vec<u8>>> {
        self.raw
            .set_read_timeout(Some(wait.max(Duration::from_millis(1))))?;
        let mut buf = vec![0u8; 1600];
        match (&self.raw).read(&mut buf) {
            Ok(n) => {
                buf.truncate(n);
                Ok(Some(buf))
            }
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock
                        | io::ErrorKind::TimedOut
                        | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}
