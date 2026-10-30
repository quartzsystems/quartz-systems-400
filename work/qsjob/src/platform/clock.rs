// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system clock for the SNTP client: `CLOCK_REALTIME`, which the
//! IPL set from the RTC and `QDATE`/`QTIME` derive from. A step sets it
//! (`clock_settime`); a slew asks the kernel to move it gradually
//! (`adjtimex` with `ADJ_OFFSET_SINGLESHOT`, at most 500 ppm); after a
//! good sample the kernel is told the clock is synchronized, which also
//! lets it copy the time to the RTC every eleven minutes. `adjtimex` has
//! no binding for musl, so it is the raw system call: the only `unsafe`
//! here.

#![allow(unsafe_code)]

use rustix::time::{ClockId, Timespec, clock_settime};

/// The system clock, adjustable (the job runs with `CAP_SYS_TIME`).
pub struct SystemClock;

/// One `adjtimex` call with `tx`; the clock state or the error.
fn adjtimex(tx: &mut libc::timex) -> Result<i64, String> {
    // SAFETY: `tx` is a valid, exclusively borrowed `struct timex` for
    // the whole call; the kernel reads the fields `modes` selects and
    // writes the structure back, nothing else.
    let r = unsafe { libc::syscall(libc::SYS_adjtimex, std::ptr::from_mut(tx)) };
    if r < 0 {
        Err(format!("adjtimex: {}", std::io::Error::last_os_error()))
    } else {
        Ok(r)
    }
}

fn zeroed() -> libc::timex {
    // SAFETY: `struct timex` is plain integers and a `timeval`; all
    // zeros is a valid value of every field.
    unsafe { std::mem::zeroed() }
}

impl ntp::Clock for SystemClock {
    fn now(&self) -> i64 {
        ntp::clock::system_now()
    }

    fn slew(&self, offset: i64) -> Result<(), String> {
        let mut tx = zeroed();
        tx.modes = libc::ADJ_OFFSET_SINGLESHOT;
        tx.offset = offset / 1000;
        adjtimex(&mut tx).map(|_| ())
    }

    fn step(&self, offset: i64) -> Result<(), String> {
        let to = i128::from(ntp::clock::system_now()) + i128::from(offset);
        let ts = Timespec {
            tv_sec: i64::try_from(to.div_euclid(1_000_000_000)).unwrap_or(i64::MAX),
            tv_nsec: i64::try_from(to.rem_euclid(1_000_000_000)).unwrap_or(0),
        };
        clock_settime(ClockId::Realtime, ts).map_err(|e| format!("clock_settime: {e}"))
    }

    fn synced(&self, error: i64) {
        let mut tx = zeroed();
        tx.modes = libc::ADJ_STATUS | libc::ADJ_MAXERROR | libc::ADJ_ESTERROR;
        tx.status = 0;
        tx.maxerror = (error / 1000).max(1);
        tx.esterror = (error / 2000).max(1);
        if let Err(e) = adjtimex(&mut tx) {
            eprintln!("qsjob: QTOTNTP: {e}");
        }
    }
}
