// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The name service's socket loop over [`Datagrams`], and [`Udp`], the
//! datagrams of UDP 137.

use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::packet::MAX_PACKET;
use super::service::{Dest, NameService, Out};

/// The name service port.
pub const PORT: u16 = 137;
/// The longest the loop waits before it looks at its stop flag.
const POLL: Duration = Duration::from_millis(200);

/// Datagrams to and from the name service's port.
pub trait Datagrams {
    /// A datagram and its sender, or `None` when `wait` passed without one.
    ///
    /// # Errors
    ///
    /// The socket's.
    fn recv(&mut self, wait: Duration) -> io::Result<Option<(Vec<u8>, SocketAddrV4)>>;
    /// Sends `bytes` to `to`.
    ///
    /// # Errors
    ///
    /// The socket's.
    fn send(&mut self, bytes: &[u8], to: SocketAddrV4) -> io::Result<()>;
    /// Where a broadcast goes.
    fn broadcast(&self) -> SocketAddrV4;
    /// This node's address towards `peer`, if it has a route there.
    fn local_ip(&self, peer: Ipv4Addr) -> Option<Ipv4Addr>;
}

/// UDP 137 on every interface, broadcasting to 255.255.255.255.
pub struct Udp {
    sock: UdpSocket,
}

impl Udp {
    /// Binds `0.0.0.0:port` with broadcasts allowed.
    ///
    /// # Errors
    ///
    /// The bind's (137 needs the privilege a job's process has).
    pub fn bind(port: u16) -> io::Result<Self> {
        let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, port))?;
        sock.set_broadcast(true)?;
        Ok(Self { sock })
    }
}

impl Datagrams for Udp {
    fn recv(&mut self, wait: Duration) -> io::Result<Option<(Vec<u8>, SocketAddrV4)>> {
        self.sock
            .set_read_timeout(Some(wait.max(Duration::from_millis(1))))?;
        let mut buf = vec![0u8; MAX_PACKET + 1];
        match self.sock.recv_from(&mut buf) {
            Ok((n, SocketAddr::V4(from))) => {
                buf.truncate(n);
                Ok(Some((buf, from)))
            }
            Ok(_) => Ok(None),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    fn send(&mut self, bytes: &[u8], to: SocketAddrV4) -> io::Result<()> {
        self.sock.send_to(bytes, to).map(|_| ())
    }

    fn broadcast(&self) -> SocketAddrV4 {
        SocketAddrV4::new(Ipv4Addr::BROADCAST, PORT)
    }

    fn local_ip(&self, peer: Ipv4Addr) -> Option<Ipv4Addr> {
        // The source address the kernel would choose: a connected socket's.
        let s = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
        s.set_broadcast(true).ok()?;
        s.connect((peer, PORT)).ok()?;
        match s.local_addr().ok()? {
            SocketAddr::V4(a) if !a.ip().is_unspecified() => Some(*a.ip()),
            _ => None,
        }
    }
}

fn send_all(d: &mut dyn Datagrams, out: Vec<Out>) {
    for o in out {
        let to = match o.dest {
            Dest::Broadcast => d.broadcast(),
            Dest::To(a) => a,
        };
        if let Err(e) = d.send(&o.packet.encode(), to) {
            tracing::debug!("nbns: send to {to}: {e}");
        }
    }
}

/// Runs `svc` on `d` until `stop`: registers the names once this node has
/// an address towards the broadcast, answers, refreshes, and releases
/// them at the end.
pub fn serve(d: &mut dyn Datagrams, svc: &mut NameService, stop: &AtomicBool) {
    let mut me = None;
    let mut next_try = Instant::now();
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        if me.is_none() && now >= next_try {
            me = d.local_ip(*d.broadcast().ip());
            match me {
                Some(ip) => send_all(d, svc.start(now, ip)),
                None => next_try = now + Duration::from_secs(1),
            }
        }
        if let Some(ip) = me {
            send_all(d, svc.tick(now, ip));
        }
        let wait = svc.next_wake().map_or(POLL, |t| {
            t.saturating_duration_since(Instant::now()).min(POLL)
        });
        match d.recv(wait) {
            Ok(Some((bytes, from))) => {
                if let Some(ip) = d.local_ip(*from.ip()).or(me) {
                    let out = svc.handle(from, &bytes, ip);
                    send_all(d, out);
                }
            }
            Ok(None) => {}
            Err(e) => {
                tracing::debug!("nbns: receive: {e}");
                std::thread::sleep(POLL);
            }
        }
    }
    if let Some(ip) = me {
        send_all(d, svc.stop(ip));
    }
}
