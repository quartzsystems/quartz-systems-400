// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading the hardware clock: `RTC_RD_TIME` on `/dev/rtc0`, treated as UTC.
//! The ioctl is the only `unsafe` here.

#![allow(unsafe_code)]

use std::fs::File;

use rustix::io::Errno;
use rustix::ioctl::{Getter, Opcode, ioctl, opcode};

/// `struct rtc_time` from `<linux/rtc.h>`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct RtcTime {
    sec: i32,
    min: i32,
    hour: i32,
    mday: i32,
    mon: i32,
    year: i32,
    wday: i32,
    yday: i32,
    isdst: i32,
}

/// `_IOR('p', 0x09, struct rtc_time)`.
const RTC_RD_TIME: Opcode = opcode::read::<RtcTime>(b'p', 0x09);

/// The RTC as seconds since the Unix epoch.
pub fn read() -> Result<u64, Errno> {
    let f = File::open("/dev/rtc0").map_err(|e| Errno::from_io_error(&e).unwrap_or(Errno::IO))?;
    // SAFETY: RTC_RD_TIME fills exactly one `struct rtc_time`, which is
    // what `RtcTime` is, and writes nothing else.
    let t = unsafe { ioctl(&f, Getter::<RTC_RD_TIME, RtcTime>::new()) }?;
    Ok(civil_to_unix(
        i64::from(t.year) + 1900,
        u32::try_from(t.mon + 1).unwrap_or(1),
        u32::try_from(t.mday).unwrap_or(1),
        u32::try_from(t.hour).unwrap_or(0),
        u32::try_from(t.min).unwrap_or(0),
        u32::try_from(t.sec).unwrap_or(0),
    ))
}

/// Proleptic Gregorian civil time to Unix seconds. Dates before 1970 clamp to 0.
pub fn civil_to_unix(year: i64, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> u64 {
    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + i64::from(hour) * 3600 + i64::from(min) * 60 + i64::from(sec);
    u64::try_from(secs).unwrap_or(0)
}

/// A Unix time as `(year, month, day, hour, minute, second)`.
#[must_use]
pub fn unix_to_civil(unix: u64) -> (i64, u32, u32, u32, u32, u32) {
    let secs = i64::try_from(unix).unwrap_or(i64::MAX);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    (
        y,
        m as u32,
        d as u32,
        (rem / 3600) as u32,
        (rem % 3600 / 60) as u32,
        (rem % 60) as u32,
    )
}

/// Days since 1970-01-01 (Howard Hinnant's algorithm).
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from((month + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates() {
        assert_eq!(civil_to_unix(1970, 1, 1, 0, 0, 0), 0);
        assert_eq!(civil_to_unix(2000, 3, 1, 0, 0, 0), 951_868_800);
        assert_eq!(civil_to_unix(2026, 1, 1, 0, 0, 0), 1_767_225_600);
        assert_eq!(civil_to_unix(2024, 2, 29, 12, 34, 56), 1_709_210_096);
    }
}
