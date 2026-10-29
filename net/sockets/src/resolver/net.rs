// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One query to one name server: UDP, and TCP when the answer comes
//! back truncated or the configuration asks for TCP. Unix only; off
//! Unix there is no network path and every exchange fails.

use std::net::SocketAddr;
use std::time::Duration;

use super::config::DnsProtocol;

/// Why an exchange produced no answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(unix), allow(dead_code))]
pub enum Failure {
    /// No answer in time.
    Timeout,
    /// The server could not be reached.
    Unreachable,
}

#[cfg(unix)]
mod imp {
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, UdpSocket};
    use std::time::{Duration, Instant};

    use super::Failure;

    fn fail(e: &std::io::Error) -> Failure {
        match e.kind() {
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => Failure::Timeout,
            _ => Failure::Unreachable,
        }
    }

    fn answers(id: u16, b: &[u8]) -> bool {
        b.len() >= 12 && u16::from_be_bytes([b[0], b[1]]) == id && b[2] & 0x80 != 0
    }

    pub(super) fn udp(server: SocketAddr, query: &[u8], id: u16, wait: Duration) -> Result<Vec<u8>, Failure> {
        let local: SocketAddr = if server.is_ipv4() {
            (Ipv4Addr::UNSPECIFIED, 0).into()
        } else {
            (Ipv6Addr::UNSPECIFIED, 0).into()
        };
        let s = UdpSocket::bind(local).map_err(|e| fail(&e))?;
        s.connect(server).map_err(|e| fail(&e))?;
        s.send(query).map_err(|e| fail(&e))?;
        let end = Instant::now() + wait;
        let mut buf = vec![0u8; 4096];
        loop {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Failure::Timeout);
            }
            s.set_read_timeout(Some(left)).map_err(|e| fail(&e))?;
            let n = s.recv(&mut buf).map_err(|e| fail(&e))?;
            // Anything that is not the answer to this query is ignored.
            if answers(id, &buf[..n]) {
                buf.truncate(n);
                return Ok(buf);
            }
        }
    }

    pub(super) fn tcp(server: SocketAddr, query: &[u8], id: u16, wait: Duration) -> Result<Vec<u8>, Failure> {
        let mut s = TcpStream::connect_timeout(&server, wait).map_err(|e| fail(&e))?;
        s.set_read_timeout(Some(wait)).map_err(|e| fail(&e))?;
        s.set_write_timeout(Some(wait)).map_err(|e| fail(&e))?;
        let len = u16::try_from(query.len()).map_err(|_| Failure::Unreachable)?;
        let mut out = len.to_be_bytes().to_vec();
        out.extend_from_slice(query);
        s.write_all(&out).map_err(|e| fail(&e))?;
        let mut l = [0u8; 2];
        s.read_exact(&mut l).map_err(|e| fail(&e))?;
        let mut buf = vec![0u8; usize::from(u16::from_be_bytes(l))];
        s.read_exact(&mut buf).map_err(|e| fail(&e))?;
        if answers(id, &buf) { Ok(buf) } else { Err(Failure::Unreachable) }
    }
}

/// Send `query` (whose identifier is `id`) and wait up to `wait` for
/// the answer.
#[cfg(unix)]
pub fn exchange(server: SocketAddr, query: &[u8], id: u16, proto: DnsProtocol, wait: Duration) -> Result<Vec<u8>, Failure> {
    if proto == DnsProtocol::Tcp {
        return imp::tcp(server, query, id, wait);
    }
    let a = imp::udp(server, query, id, wait)?;
    // TC: the whole answer only comes over TCP.
    if a[2] & 0x02 != 0 {
        return imp::tcp(server, query, id, wait);
    }
    Ok(a)
}

/// No network path off Unix.
#[cfg(not(unix))]
pub fn exchange(_server: SocketAddr, _query: &[u8], _id: u16, _proto: DnsProtocol, _wait: Duration) -> Result<Vec<u8>, Failure> {
    Err(Failure::Unreachable)
}
