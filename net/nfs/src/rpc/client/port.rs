// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sockets for the client, bound to a reserved port (1023 down to 512)
//! when they can be, else to an ephemeral one without a word.

use std::io;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

use socket2::{Domain, Protocol, Socket, Type};

/// The lowest reserved port tried.
const LOW: u16 = 512;

fn any(for_addr: &SocketAddr) -> IpAddr {
    if for_addr.is_ipv4() {
        IpAddr::V4(Ipv4Addr::UNSPECIFIED)
    } else {
        IpAddr::V6(Ipv6Addr::UNSPECIFIED)
    }
}

/// Binds `s` to a free reserved port; false when none can be had.
fn bind_reserved(s: &Socket, ip: IpAddr) -> bool {
    for port in (LOW..=1023).rev() {
        match s.bind(&SocketAddr::new(ip, port).into()) {
            Ok(()) => return true,
            // Not allowed to: no other port will be either.
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return false,
            Err(_) => {}
        }
    }
    false
}

/// A TCP connection to `addr` within `timeout`.
pub fn tcp(addr: SocketAddr, reserved: bool, timeout: Duration) -> io::Result<TcpStream> {
    let s = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    if reserved {
        // A reserved port is reused quickly across reconnections (on
        // Windows the option would take ports from other sockets).
        #[cfg(not(windows))]
        let _ = s.set_reuse_address(true);
        if !bind_reserved(&s, any(&addr)) {
            tracing::debug!("no reserved port for {addr}: an ephemeral one is used");
        }
    }
    let t = timeout.max(Duration::from_millis(1));
    match s.connect_timeout(&addr.into(), t) {
        Ok(()) => {}
        // The reserved port is still tied to this server: take any.
        Err(e)
            if reserved
                && matches!(
                    e.kind(),
                    io::ErrorKind::AddrInUse | io::ErrorKind::AddrNotAvailable
                ) =>
        {
            return tcp(addr, false, timeout);
        }
        Err(e) => return Err(e),
    }
    let t: TcpStream = s.into();
    t.set_nodelay(true)?;
    Ok(t)
}

/// A UDP socket connected to `addr`.
pub fn udp(addr: SocketAddr, reserved: bool) -> io::Result<UdpSocket> {
    let s = Socket::new(Domain::for_address(addr), Type::DGRAM, Some(Protocol::UDP))?;
    if !(reserved && bind_reserved(&s, any(&addr))) {
        s.bind(&SocketAddr::new(any(&addr), 0).into())?;
    }
    let u: UdpSocket = s.into();
    u.connect(addr)?;
    Ok(u)
}
