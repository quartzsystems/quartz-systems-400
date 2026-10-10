// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! C1001040: set the system clock from the RTC. The kernel does not do it
//! (`RTC_HCTOSYS=n`), so until here the clock reads near the epoch.

use rustix::time::{ClockId, Timespec, clock_settime};

use crate::ipl::system::System;
use crate::platform::rtc;

/// Set `CLOCK_REALTIME` to a Unix time (the IPL options screen).
pub fn set(unix: u64) -> Result<(), String> {
    let ts = Timespec {
        tv_sec: i64::try_from(unix).unwrap_or(i64::MAX),
        tv_nsec: 0,
    };
    clock_settime(ClockId::Realtime, ts).map_err(|e| format!("clock_settime: {e}"))
}

/// Set `CLOCK_REALTIME` from `/dev/rtc0` and re-stamp the IPL start time,
/// which was taken before the clock was right. Returns the Unix time.
pub fn set_from_rtc(system: &mut System) -> Result<u64, String> {
    let now = rtc::read().map_err(|e| format!("read /dev/rtc0: {e}"))?;
    let ts = Timespec {
        tv_sec: i64::try_from(now).unwrap_or(i64::MAX),
        tv_nsec: 0,
    };
    clock_settime(ClockId::Realtime, ts).map_err(|e| format!("clock_settime: {e}"))?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let uptime = system.log.uptime().round() as u64;
    system.start_time = now.saturating_sub(uptime);
    Ok(now)
}
