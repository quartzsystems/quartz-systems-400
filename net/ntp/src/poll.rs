// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One poll of the client: the servers in `RMTSYS` order, the first
//! that answers usefully is the sample; its offset is decided under the
//! policy and applied to the clock; the server's upstream follows.

use std::net::SocketAddr;
use std::time::Duration;

use crate::adjust::{Decision, Policy};
use crate::client::{self, Sample, SntpError};
use crate::clock::Clock;
use crate::server::{Shared, Upstream};

/// What a poll did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Polled {
    /// A server answered; what was decided and whether applying it
    /// worked.
    Sample {
        /// The sample.
        sample: Sample,
        /// The decision.
        decision: Decision,
        /// The clock's answer to the adjustment.
        applied: Result<(), String>,
    },
    /// No server answered usefully: each one's reason.
    Failed(Vec<(String, SntpError)>),
}

/// The reference identifier for an upstream address.
fn reference(addr: SocketAddr) -> [u8; 4] {
    match addr {
        SocketAddr::V4(a) => a.ip().octets(),
        // RFC 5905: the first four octets of the address's MD5; any
        // stable four octets serve a client here.
        SocketAddr::V6(a) => {
            let o = a.ip().octets();
            [o[12], o[13], o[14], o[15]]
        }
    }
}

/// Poll once. `servers` are names with an address each (resolved by
/// the caller; a name that did not resolve is left out).
pub fn poll(
    servers: &[(String, SocketAddr)],
    clock: &dyn Clock,
    policy: &Policy,
    timeout: Duration,
    up: Option<&Shared>,
) -> Polled {
    let mut failed = Vec::new();
    for (name, addr) in servers {
        let sample = match client::best_of(name, *addr, clock, timeout, 3) {
            Ok(s) => s,
            Err(e) => {
                failed.push((name.clone(), e));
                continue;
            }
        };
        let decision = policy.decide(sample.offset);
        let applied = match decision {
            Decision::Keep => Ok(()),
            Decision::Slew(n) => clock.slew(n),
            Decision::Step(n) => clock.step(n),
            Decision::TooFar(_) => Err("beyond the maximum adjustment".into()),
        };
        if applied.is_ok() {
            clock.synced(sample.delay / 2);
            if let Some(up) = up {
                *up.lock().expect("upstream") = Some(Upstream {
                    stratum: sample.stratum,
                    reference_id: reference(*addr),
                    when: clock.now(),
                    delay: sample.delay,
                });
            }
        }
        return Polled::Sample {
            sample,
            decision,
            applied,
        };
    }
    Polled::Failed(failed)
}

#[cfg(test)]
mod tests {
    use std::net::UdpSocket;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::clock::MemClock;
    use crate::server;

    const S: i64 = 1_000_000_000;

    fn time_server(at: i64) -> server::Running {
        let clock: Arc<dyn Clock> = Arc::new(MemClock::at(at));
        server::serve(UdpSocket::bind("127.0.0.1:0").unwrap(), clock, Arc::new(Mutex::new(None)), false, None).unwrap()
    }

    #[test]
    fn slews_steps_and_refuses() {
        let base = 1_900_000_000 * S;
        let policy = Policy::of(Some(20), 20, Some(60));
        let dead = UdpSocket::bind("127.0.0.1:0").unwrap();
        let quiet = ("quiet".to_string(), dead.local_addr().unwrap());

        // 2 s off: slewed, after a server that does not answer.
        let ts = time_server(base + 2 * S);
        let mine = MemClock::at(base);
        let up: Shared = Arc::new(Mutex::new(None));
        let p = poll(&[quiet.clone(), ("good".into(), ts.addr())], &mine, &policy, Duration::from_millis(200), Some(&up));
        let Polled::Sample { sample, decision, applied } = p else {
            panic!("{p:?}");
        };
        assert_eq!((sample.server.as_str(), decision, applied), ("good", Decision::Slew(2 * S), Ok(())));
        assert_eq!(mine.slews(), [2 * S]);
        let u = up.lock().unwrap().unwrap();
        assert_eq!((u.stratum, u.reference_id), (server::LOCAL_STRATUM, [127, 0, 0, 1]));

        // 5 minutes off: stepped.
        let ts = time_server(base + 300 * S);
        let mine = MemClock::at(base);
        let p = poll(&[("t".into(), ts.addr())], &mine, &policy, Duration::from_secs(2), None);
        assert!(matches!(p, Polled::Sample { decision: Decision::Step(_), .. }), "{p:?}");
        assert_eq!(mine.steps(), [300 * S]);

        // An hour off: left alone.
        let ts = time_server(base + 3600 * S);
        let mine = MemClock::at(base);
        let up: Shared = Arc::new(Mutex::new(None));
        let p = poll(&[("t".into(), ts.addr())], &mine, &policy, Duration::from_secs(2), Some(&up));
        assert!(matches!(p, Polled::Sample { decision: Decision::TooFar(_), applied: Err(_), .. }), "{p:?}");
        assert!(mine.steps().is_empty() && mine.slews().is_empty());
        assert!(up.lock().unwrap().is_none());

        // Nobody answers.
        let p = poll(&[quiet], &mine, &policy, Duration::from_millis(100), None);
        assert!(matches!(p, Polled::Failed(ref f) if f.len() == 1), "{p:?}");
    }
}
