// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SNTP client's exchange (RFC 4330 section 5): send a request
//! stamped T1, take the answer's T2 and T3, note T4 on arrival; the
//! clock's offset is ((T2 - T1) + (T3 - T4)) / 2 and the round trip
//! (T4 - T1) - (T3 - T2). An answer is used only if it is a server's,
//! echoes T1, is synchronized and is not a kiss-o'-death.

use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use thiserror::Error;

use crate::clock::Clock;
use crate::packet::{Leap, Mode, Packet, Timestamp};

/// Why an exchange gave no sample.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum SntpError {
    /// The socket failed.
    #[error("{0}")]
    Io(String),
    /// Nothing came back in time.
    #[error("no answer from {0}")]
    Timeout(String),
    /// The server sent a kiss-o'-death.
    #[error("the server refused service ({0})")]
    Kiss(String),
    /// The server's own clock is not synchronized.
    #[error("the server is not synchronized")]
    Unsynchronized,
    /// The answer is not one to use.
    #[error("unusable answer: {0}")]
    Bogus(String),
}

/// One exchange's result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    /// The server asked.
    pub server: String,
    /// What to add to the local clock, nanoseconds.
    pub offset: i64,
    /// The round trip, nanoseconds.
    pub delay: i64,
    /// The server's stratum.
    pub stratum: u8,
    /// The server's reference identifier.
    pub reference_id: [u8; 4],
}

/// The sample an answer gives, or why it is unusable. `t1` is the
/// request's transmit time, `t4` the local clock when the answer came.
pub fn evaluate(server: &str, t1: Timestamp, reply: &Packet, t4: i64) -> Result<Sample, SntpError> {
    if reply.mode != Mode::Server && reply.mode != Mode::Broadcast {
        return Err(SntpError::Bogus(format!("mode {:?}", reply.mode)));
    }
    if let Some(code) = reply.kiss_code() {
        return Err(SntpError::Kiss(code));
    }
    if reply.origin != t1 {
        return Err(SntpError::Bogus("the answer is not to this request".into()));
    }
    if reply.transmit.is_zero() {
        return Err(SntpError::Bogus("no transmit time".into()));
    }
    if reply.leap == Leap::Unknown || reply.stratum > 15 {
        return Err(SntpError::Unsynchronized);
    }
    let t1 = i128::from(t1.to_unix_nanos());
    let t2 = i128::from(reply.receive.to_unix_nanos());
    let t3 = i128::from(reply.transmit.to_unix_nanos());
    let t4 = i128::from(t4);
    let offset = i128::midpoint(t2 - t1, t3 - t4);
    let delay = ((t4 - t1) - (t3 - t2)).max(0);
    Ok(Sample {
        server: server.to_string(),
        offset: i64::try_from(offset).unwrap_or(i64::MAX),
        delay: i64::try_from(delay).unwrap_or(i64::MAX),
        stratum: reply.stratum,
        reference_id: reply.reference_id,
    })
}

/// One exchange with `addr` (named `server` in the sample).
pub fn query(server: &str, addr: SocketAddr, clock: &dyn Clock, timeout: Duration) -> Result<Sample, SntpError> {
    let bind: SocketAddr = if addr.is_ipv4() {
        ([0, 0, 0, 0], 0).into()
    } else {
        (std::net::Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let io = |e: std::io::Error| SntpError::Io(e.to_string());
    let sock = UdpSocket::bind(bind).map_err(io)?;
    sock.connect(addr).map_err(io)?;
    sock.set_read_timeout(Some(timeout)).map_err(io)?;
    let t1 = Timestamp::from_unix_nanos(clock.now());
    sock.send(&Packet::request(t1).encode()).map_err(io)?;
    let mut buf = [0u8; 512];
    loop {
        let n = match sock.recv(&mut buf) {
            Ok(n) => n,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {
                return Err(SntpError::Timeout(server.to_string()));
            }
            Err(e) => return Err(io(e)),
        };
        let t4 = clock.now();
        let Ok(p) = Packet::decode(&buf[..n]) else {
            continue;
        };
        // A late answer to an earlier request is skipped.
        if p.origin != t1 && p.mode == Mode::Server && p.stratum != 0 {
            continue;
        }
        return evaluate(server, t1, &p, t4);
    }
}

/// The best of up to `tries` exchanges with `addr`: the shortest round
/// trip's (RFC 5905's clock filter in its simplest form).
pub fn best_of(
    server: &str,
    addr: SocketAddr,
    clock: &dyn Clock,
    timeout: Duration,
    tries: u32,
) -> Result<Sample, SntpError> {
    let mut best: Option<Sample> = None;
    let mut last = None;
    for _ in 0..tries.max(1) {
        match query(server, addr, clock, timeout) {
            Ok(s) => {
                if best.as_ref().is_none_or(|b| s.delay < b.delay) {
                    best = Some(s);
                }
            }
            // A refusal ends the exchanges with this server.
            Err(e @ SntpError::Kiss(_)) => return Err(e),
            Err(e) => last = Some(e),
        }
    }
    best.ok_or_else(|| last.unwrap_or_else(|| SntpError::Timeout(server.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: i64 = 1_000_000_000;

    fn answer(t1: Timestamp, t2: i64, t3: i64) -> Packet {
        Packet {
            mode: Mode::Server,
            stratum: 2,
            reference_id: [10, 0, 0, 1],
            origin: t1,
            receive: Timestamp::from_unix_nanos(t2),
            transmit: Timestamp::from_unix_nanos(t3),
            ..Packet::default()
        }
    }

    #[test]
    fn offset_and_delay() {
        // Local clock 5 s behind; 100 ms each way; 10 ms in the server.
        let base = 1_800_000_000 * S;
        let t1 = Timestamp::from_unix_nanos(base);
        let p = answer(t1, base + 5 * S + 100_000_000, base + 5 * S + 110_000_000);
        let s = evaluate("h", t1, &p, base + 210_000_000).unwrap();
        assert!((s.offset - 5 * S).abs() < 2, "{}", s.offset);
        assert!((s.delay - 200_000_000).abs() < 2, "{}", s.delay);
        assert_eq!(s.stratum, 2);
    }

    #[test]
    fn unusable_answers() {
        let t1 = Timestamp::from_unix_nanos(S);
        let mut p = answer(t1, S, S);
        p.origin = Timestamp(7);
        assert!(matches!(evaluate("h", t1, &p, S), Err(SntpError::Bogus(_))));
        let mut p = answer(t1, S, S);
        p.leap = Leap::Unknown;
        assert_eq!(evaluate("h", t1, &p, S), Err(SntpError::Unsynchronized));
        let mut p = answer(t1, S, S);
        p.stratum = 0;
        p.reference_id = *b"DENY";
        assert_eq!(evaluate("h", t1, &p, S), Err(SntpError::Kiss("DENY".into())));
        let mut p = answer(t1, S, S);
        p.mode = Mode::Client;
        assert!(evaluate("h", t1, &p, S).is_err());
    }
}
