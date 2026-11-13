// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What one start of a server has: its write verifier (NFSv3 `WRITE`
//! and `COMMIT`, NFSv4's boot verifier), which differs at every start so
//! a client sends again what it wrote unstable before; and the clock
//! its grace period runs on ([`Clock`], a [`ManualClock`] for tests).

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A clock.
pub trait Clock: Send + Sync {
    /// Now.
    fn now(&self) -> Instant;
}

/// The system's clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// A clock moved by hand.
#[derive(Debug)]
pub struct ManualClock {
    base: Instant,
    off: Mutex<Duration>,
}

impl Default for ManualClock {
    fn default() -> Self {
        Self {
            base: Instant::now(),
            off: Mutex::new(Duration::ZERO),
        }
    }
}

impl ManualClock {
    /// Moves it on by `d`.
    pub fn advance(&self, d: Duration) {
        let mut o = self.off.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        *o += d;
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.base + *self.off.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// One start of a server.
#[derive(Debug)]
pub struct Instance {
    verf: [u8; 8],
    started: SystemTime,
}

static STARTS: AtomicU64 = AtomicU64::new(0);

impl Instance {
    /// A new start: a verifier from the time, the process and a count,
    /// so two starts never share one.
    #[must_use]
    pub fn new() -> Self {
        let now = SystemTime::now();
        let t = now.duration_since(UNIX_EPOCH).unwrap_or_default();
        let n = STARTS.fetch_add(1, Ordering::Relaxed);
        let secs = u32::try_from(t.as_secs() & 0xffff_ffff).unwrap_or(0);
        let low = t.subsec_nanos() ^ std::process::id().rotate_left(16)
            ^ u32::try_from(n & 0xffff_ffff).unwrap_or(0).rotate_left(8);
        let mut verf = [0u8; 8];
        verf[..4].copy_from_slice(&secs.to_be_bytes());
        verf[4..].copy_from_slice(&low.wrapping_add(u32::try_from(n & 0xff).unwrap_or(0)).to_be_bytes());
        Self { verf, started: now }
    }

    /// The write verifier.
    #[must_use]
    pub fn verf(&self) -> [u8; 8] {
        self.verf
    }

    /// When it started.
    #[must_use]
    pub fn started(&self) -> SystemTime {
        self.started
    }
}

impl Default for Instance {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_differ() {
        assert_ne!(Instance::new().verf(), Instance::new().verf());
        let c = ManualClock::default();
        let a = c.now();
        c.advance(Duration::from_secs(5));
        assert_eq!(c.now() - a, Duration::from_secs(5));
    }
}
