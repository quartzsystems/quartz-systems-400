// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SNTP server: a client's request (mode 3) is answered from the
//! system clock (RFC 4330 section 6). Synchronized by the client, the
//! answer carries the upstream server's stratum plus one; otherwise
//! the local clock answers at stratum 10 as `LOCL`, unless `SYNCRQD`
//! says to stay silent until the client has synchronized.

use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use crate::clock::Clock;
use crate::packet::{Leap, Mode, Packet, Timestamp, nanos_to_short};

/// The stratum the local clock answers at when nothing synchronized it.
pub const LOCAL_STRATUM: u8 = 10;
/// The precision announced: about a microsecond.
pub const PRECISION: i8 = -20;

/// What the client last synchronized to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Upstream {
    /// Its stratum.
    pub stratum: u8,
    /// Its address as a reference identifier (IPv4) or a hash.
    pub reference_id: [u8; 4],
    /// When the clock was last set from it, Unix nanoseconds.
    pub when: i64,
    /// The round trip to it, nanoseconds.
    pub delay: i64,
}

/// The upstream, shared between the client and the server.
pub type Shared = Arc<Mutex<Option<Upstream>>>;

/// The answer to `req`, received at `t2` and sent at `t3` (Unix
/// nanoseconds); `None` when there is none to give.
#[must_use]
pub fn answer(req: &Packet, t2: i64, t3: i64, up: Option<Upstream>, sync_required: bool) -> Option<Packet> {
    if req.mode != Mode::Client {
        return None;
    }
    if up.is_none() && sync_required {
        return None;
    }
    let (stratum, reference_id, reference, delay) = match up {
        Some(u) => (u.stratum.saturating_add(1).min(15), u.reference_id, u.when, u.delay),
        None => (LOCAL_STRATUM, *b"LOCL", t2, 0),
    };
    Some(Packet {
        leap: Leap::None,
        version: req.version.clamp(3, 4),
        mode: Mode::Server,
        stratum,
        poll: req.poll,
        precision: PRECISION,
        root_delay: nanos_to_short(delay),
        root_dispersion: nanos_to_short(1_000_000),
        reference_id,
        reference: Timestamp::from_unix_nanos(reference),
        origin: req.transmit,
        receive: Timestamp::from_unix_nanos(t2),
        transmit: Timestamp::from_unix_nanos(t3),
    })
}

/// A running server loop.
pub struct Running {
    stop: Arc<AtomicBool>,
    addr: SocketAddr,
    thread: Option<JoinHandle<()>>,
}

impl Running {
    /// Where it listens.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stop answering.
    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.halt();
    }
}

/// What the loop reports of each request: the peer and whether it was
/// answered.
pub type Log = Box<dyn Fn(SocketAddr, bool) + Send>;

/// Answer requests on `sock` until stopped.
pub fn serve(
    sock: UdpSocket,
    clock: Arc<dyn Clock>,
    up: Shared,
    sync_required: bool,
    log: Option<Log>,
) -> std::io::Result<Running> {
    sock.set_read_timeout(Some(Duration::from_millis(100)))?;
    let addr = sock.local_addr()?;
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let thread = std::thread::spawn(move || {
        let mut buf = [0u8; 512];
        while !flag.load(Ordering::Relaxed) {
            let Ok((n, peer)) = sock.recv_from(&mut buf) else {
                continue;
            };
            let t2 = clock.now();
            let reply = Packet::decode(&buf[..n]).ok().and_then(|req| {
                let current = *up.lock().expect("upstream");
                answer(&req, t2, clock.now(), current, sync_required)
            });
            let sent = reply.is_some_and(|p| sock.send_to(&p.encode(), peer).is_ok());
            if let Some(l) = &log {
                l(peer, sent);
            }
        }
    });
    Ok(Running {
        stop,
        addr,
        thread: Some(thread),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client;
    use crate::clock::MemClock;

    const S: i64 = 1_000_000_000;

    #[test]
    fn answers_a_request() {
        let req = Packet::request(Timestamp::from_unix_nanos(100 * S));
        let p = answer(&req, 105 * S, 105 * S + 1000, None, false).unwrap();
        assert_eq!((p.mode, p.stratum, &p.reference_id), (Mode::Server, LOCAL_STRATUM, b"LOCL"));
        assert_eq!(p.origin, req.transmit);
        let up = Upstream {
            stratum: 2,
            reference_id: [192, 0, 2, 1],
            when: 90 * S,
            delay: 3_000_000,
        };
        let p = answer(&req, 105 * S, 105 * S, Some(up), true).unwrap();
        assert_eq!(p.stratum, 3);
        assert_eq!(p.reference_id, [192, 0, 2, 1]);
        // SYNCRQD(*YES) without an upstream: silence; not a client: silence.
        assert!(answer(&req, 0, 0, None, true).is_none());
        let mut other = req;
        other.mode = Mode::Server;
        assert!(answer(&other, 0, 0, None, false).is_none());
    }

    #[test]
    fn client_and_server_over_loopback() {
        // The server's clock is 3 s ahead of the client's.
        let server_clock: Arc<dyn Clock> = Arc::new(MemClock::at(1_900_000_003 * S));
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let running = serve(sock, server_clock, Arc::new(Mutex::new(None)), false, None).unwrap();
        let mine = MemClock::at(1_900_000_000 * S);
        let s = client::best_of("lo", running.addr(), &mine, Duration::from_secs(2), 2).unwrap();
        assert_eq!(s.offset, 3 * S);
        assert_eq!(s.stratum, LOCAL_STRATUM);
        running.stop();
    }
}
