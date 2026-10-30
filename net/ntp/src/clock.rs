// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The clock the client reads and adjusts. The job process gives the
//! system's (`CLOCK_REALTIME`, slewed with `adjtimex`); tests give
//! [`MemClock`].

use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// A clock: read, slewed, stepped. Times are Unix nanoseconds, UTC.
pub trait Clock: Send + Sync {
    /// Now.
    fn now(&self) -> i64;

    /// Move the clock by `offset` nanoseconds gradually.
    fn slew(&self, offset: i64) -> Result<(), String>;

    /// Move the clock by `offset` nanoseconds at once.
    fn step(&self, offset: i64) -> Result<(), String>;

    /// The clock was just checked against a server and is right within
    /// `error` nanoseconds.
    fn synced(&self, error: i64) {
        let _ = error;
    }
}

/// The system clock read only: slewing and stepping are refused. The
/// server uses it; the job process supplies the adjusting one.
#[derive(Clone, Copy, Debug, Default)]
pub struct ReadOnly;

/// `SystemTime::now()` as Unix nanoseconds.
#[must_use]
pub fn system_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_nanos()).unwrap_or(i64::MAX),
        Err(e) => -i64::try_from(e.duration().as_nanos()).unwrap_or(i64::MAX),
    }
}

impl Clock for ReadOnly {
    fn now(&self) -> i64 {
        system_now()
    }

    fn slew(&self, _: i64) -> Result<(), String> {
        Err("this clock cannot be adjusted".into())
    }

    fn step(&self, _: i64) -> Result<(), String> {
        Err("this clock cannot be set".into())
    }
}

/// A clock in memory: a fixed time plus the adjustments made, which
/// are recorded.
#[derive(Debug, Default)]
pub struct MemClock {
    state: Mutex<MemState>,
}

#[derive(Debug, Default)]
struct MemState {
    now: i64,
    slews: Vec<i64>,
    steps: Vec<i64>,
}

impl MemClock {
    /// A clock reading `now`.
    #[must_use]
    pub fn at(now: i64) -> Self {
        Self {
            state: Mutex::new(MemState {
                now,
                ..MemState::default()
            }),
        }
    }

    /// Move time on by `ns`.
    pub fn advance(&self, ns: i64) {
        self.state.lock().expect("clock").now += ns;
    }

    /// The slews asked for.
    #[must_use]
    pub fn slews(&self) -> Vec<i64> {
        self.state.lock().expect("clock").slews.clone()
    }

    /// The steps asked for.
    #[must_use]
    pub fn steps(&self) -> Vec<i64> {
        self.state.lock().expect("clock").steps.clone()
    }
}

impl Clock for MemClock {
    fn now(&self) -> i64 {
        self.state.lock().expect("clock").now
    }

    fn slew(&self, offset: i64) -> Result<(), String> {
        let mut s = self.state.lock().expect("clock");
        s.slews.push(offset);
        // A slew completes in time; the test clock takes it at once.
        s.now += offset;
        Ok(())
    }

    fn step(&self, offset: i64) -> Result<(), String> {
        let mut s = self.state.lock().expect("clock");
        s.steps.push(offset);
        s.now += offset;
        Ok(())
    }
}
