// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The limits before authentication, as a QS/VS host has them: at most
//! [`MAX_SESSIONS`] sessions and [`MAX_HANDSHAKES`] handshakes at once,
//! a handshake within [`HANDSHAKE`], and an address that failed
//! [`MAX_FAILURES`] handshakes or pairings within [`FAILURE_WINDOW`]
//! turned away for as long.

use std::collections::{BTreeMap, BTreeSet};
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// The most sessions at once: two consoles, each reconnecting while its
/// old session times out, and pairing.
pub const MAX_SESSIONS: usize = 6;
/// The most handshakes under way at once.
pub const MAX_HANDSHAKES: usize = 4;
/// How long a connection has for its handshake.
pub const HANDSHAKE: Duration = Duration::from_secs(10);
/// Failed handshakes and pairings from one address before it is turned
/// away: more than one password's five attempts.
pub const MAX_FAILURES: u32 = 10;
/// How long failures are counted, and an address turned away.
pub const FAILURE_WINDOW: Duration = Duration::from_secs(60);

/// Why a connection was turned away.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turned {
    /// As many sessions as there may be.
    Full,
    /// As many handshakes under way as there may be.
    Busy,
    /// Its address failed too often of late.
    Failing,
}

impl Turned {
    /// In words, for the job log.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::Full => "as many sessions as there may be",
            Self::Busy => "as many handshakes under way as there may be",
            Self::Failing => "its address failed too often in the last minute",
        }
    }
}

/// The counts every connection is admitted by.
#[derive(Debug, Default)]
pub struct Gate {
    sessions: usize,
    handshakes: usize,
    failures: BTreeMap<IpAddr, (u32, Instant)>,
    said: BTreeSet<IpAddr>,
}

impl Gate {
    /// Admit a connection from `ip` to its handshake.
    ///
    /// # Errors
    /// Why it is turned away.
    pub fn admit(&mut self, ip: IpAddr, now: Instant) -> Result<(), Turned> {
        self.failures
            .retain(|_, (_, since)| now.duration_since(*since) < FAILURE_WINDOW);
        let failures = &self.failures;
        self.said.retain(|ip| failures.contains_key(ip));
        if self
            .failures
            .get(&ip)
            .is_some_and(|(n, _)| *n >= MAX_FAILURES)
        {
            return Err(Turned::Failing);
        }
        if self.sessions + self.handshakes >= MAX_SESSIONS {
            return Err(Turned::Full);
        }
        if self.handshakes >= MAX_HANDSHAKES {
            return Err(Turned::Busy);
        }
        self.handshakes += 1;
        Ok(())
    }

    /// Whether `ip`, turned away for its failures, is to be said: the
    /// first time in its window.
    pub fn say_failing(&mut self, ip: IpAddr) -> bool {
        self.said.insert(ip)
    }

    /// The handshake failed.
    pub fn failed(&mut self, ip: IpAddr, now: Instant) {
        self.handshakes = self.handshakes.saturating_sub(1);
        self.failed_pairing(ip, now);
    }

    /// A pairing attempt from `ip` failed.
    pub fn failed_pairing(&mut self, ip: IpAddr, now: Instant) {
        let entry = self.failures.entry(ip).or_insert((0, now));
        entry.0 += 1;
    }

    /// The handshake is done: a session.
    pub fn opened(&mut self) {
        self.handshakes = self.handshakes.saturating_sub(1);
        self.sessions += 1;
    }

    /// A session ended.
    pub fn closed(&mut self) {
        self.sessions = self.sessions.saturating_sub(1);
    }

    /// Sessions open now.
    #[must_use]
    pub fn sessions(&self) -> usize {
        self.sessions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_that_fails_is_turned_away_for_a_while() {
        let mut g = Gate::default();
        let ip: IpAddr = "192.0.2.7".parse().unwrap();
        let other: IpAddr = "192.0.2.8".parse().unwrap();
        let t = Instant::now();
        for _ in 0..MAX_FAILURES {
            g.admit(ip, t).unwrap();
            g.failed(ip, t);
        }
        assert_eq!(g.admit(ip, t), Err(Turned::Failing));
        assert!(g.say_failing(ip));
        assert!(!g.say_failing(ip));
        assert_eq!(g.admit(other, t), Ok(()));
        assert_eq!(g.admit(ip, t + FAILURE_WINDOW), Ok(()));
    }

    #[test]
    fn sessions_and_handshakes_are_bounded() {
        let mut g = Gate::default();
        let ip: IpAddr = "192.0.2.7".parse().unwrap();
        let t = Instant::now();
        for _ in 0..MAX_HANDSHAKES {
            g.admit(ip, t).unwrap();
        }
        assert_eq!(g.admit(ip, t), Err(Turned::Busy));
        for _ in 0..MAX_HANDSHAKES {
            g.opened();
        }
        for _ in MAX_HANDSHAKES..MAX_SESSIONS {
            g.admit(ip, t).unwrap();
            g.opened();
        }
        assert_eq!(g.admit(ip, t), Err(Turned::Full));
        g.closed();
        assert_eq!(g.admit(ip, t), Ok(()));
        assert_eq!(g.sessions(), MAX_SESSIONS - 1);
    }

    #[test]
    fn pairings_that_fail_count_as_handshakes_do() {
        let mut g = Gate::default();
        let ip: IpAddr = "192.0.2.9".parse().unwrap();
        let t = Instant::now();
        for _ in 0..MAX_FAILURES {
            g.failed_pairing(ip, t);
        }
        assert_eq!(g.admit(ip, t), Err(Turned::Failing));
    }
}
