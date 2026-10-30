// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Listing lines. `LISTFMT 0` is IBM's own layout (clients parse it):
//! owner in ten, size right in eleven, `yy-mm-dd hh:mm:ss`, the type in
//! ten, the name; a member line leaves size and dates blank. `LISTFMT
//! 1` is the Unix `ls -l` layout: mode, links in three, owner and group
//! in eight, size right in seven, the date as `Mon dd hh:mm` within
//! half a year and `Mon dd  yyyy` before.

use ifs::ops::Stat;
use ifs::ops::flags::{S_IFDIR, S_IFMT};

/// Seconds in half a year, the `ls` rule for showing a time.
const HALF_YEAR: i64 = 183 * 86_400;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A calendar time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Civil {
    /// Year.
    pub year: i64,
    /// Month, 1 to 12.
    pub month: u32,
    /// Day, 1 to 31.
    pub day: u32,
    /// Hour.
    pub hour: u32,
    /// Minute.
    pub minute: u32,
    /// Second.
    pub second: u32,
}

/// Unix seconds as a calendar time (proleptic Gregorian).
pub fn civil(secs: i64) -> Civil {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let as_u32 = |v: i64| u32::try_from(v).unwrap_or(0);
    Civil {
        year: if m <= 2 { y + 1 } else { y },
        month: as_u32(m),
        day: as_u32(d),
        hour: as_u32(rem / 3600),
        minute: as_u32(rem % 3600 / 60),
        second: as_u32(rem % 60),
    }
}

/// Seconds of a `Stat` time.
pub fn secs(nanos: ifs::backend::Nanos) -> i64 {
    nanos.div_euclid(1_000_000_000)
}

/// `yy-mm-dd hh:mm:ss`.
pub fn date0(local: i64) -> String {
    let c = civil(local);
    format!(
        "{:02}-{:02}-{:02} {:02}:{:02}:{:02}",
        c.year.rem_euclid(100),
        c.month,
        c.day,
        c.hour,
        c.minute,
        c.second
    )
}

/// `Mon dd hh:mm` or `Mon dd yyyy ` (`ls`), against `now`.
pub fn date1(local: i64, now: i64) -> String {
    let c = civil(local);
    let mon = MONTHS[(c.month as usize).saturating_sub(1) % 12];
    if (now - local).abs() < HALF_YEAR {
        format!("{mon} {:02} {:02}:{:02}", c.day, c.hour, c.minute)
    } else {
        format!("{mon} {:02} {} ", c.day, c.year)
    }
}

/// One `LISTFMT 0` line of an object with size and time.
pub fn line0(owner: &str, size: u64, local: i64, objtype: &str, name: &str) -> String {
    format!("{owner:<10}{size:>11} {} {objtype:<10} {name}", date0(local))
}

/// One `LISTFMT 0` member line: no size, no time.
pub fn member0(owner: &str, name: &str) -> String {
    format!("{owner:<10}{:30}{:<10} {name}", "", "*MEM")
}

/// The `ls` mode string; `dir` forces a `d`.
pub fn mode_string(mode: u32, dir: bool) -> String {
    let t = if dir || mode & S_IFMT == S_IFDIR { 'd' } else { '-' };
    let mut s = String::with_capacity(10);
    s.push(t);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 7;
        s.push(if bits & 4 != 0 { 'r' } else { '-' });
        s.push(if bits & 2 != 0 { 'w' } else { '-' });
        s.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    s
}

/// One `LISTFMT 1` line.
#[allow(clippy::too_many_arguments)]
pub fn line1(mode: &str, nlink: u32, owner: &str, group: &str, size: u64, local: i64, now: i64, name: &str) -> String {
    format!(
        "{mode} {nlink:>3} {owner:<8} {group:<8} {size:>7} {} {name}",
        date1(local, now)
    )
}

/// The `LISTFMT 1` line of something `stat` describes.
pub fn stat_line1(st: &Stat, dir: bool, nlink: u32, offset: i64, now: i64, name: &str) -> String {
    line1(
        &mode_string(st.mode, dir),
        nlink,
        &st.owner,
        "0",
        st.size,
        secs(st.mtime) + offset,
        now + offset,
        name,
    )
}

/// The `LISTFMT 0` line of something `stat` describes.
pub fn stat_line0(st: &Stat, objtype: &str, offset: i64, name: &str) -> String {
    line0(&st.owner, st.size, secs(st.mtime) + offset, objtype, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar() {
        // 2026-10-29 21:33:00 UTC.
        let t = 1_793_309_580;
        let c = civil(t);
        assert_eq!((c.year, c.month, c.day, c.hour, c.minute), (2026, 10, 29, 21, 33));
        assert_eq!(date0(t), "26-10-29 21:33:00");
        assert_eq!(date1(t, t + 10), "Oct 29 21:33");
        assert_eq!(date1(t - 400 * 86_400, t), "Sep 24 2025 ");
    }

    #[test]
    fn layouts_match_the_oracles() {
        let t = civil_secs(2026, 10, 10, 0, 10, 31);
        assert_eq!(
            line0("QSYS", 143_360, t, "*FILE", "ARPA"),
            "QSYS           143360 26-10-10 00:10:31 *FILE      ARPA"
        );
        assert_eq!(
            member0("QSYS", "ARPA.INET"),
            "QSYS                                    *MEM       ARPA.INET"
        );
        let now = civil_secs(2026, 10, 29, 21, 0, 0);
        assert_eq!(
            line1("drwx---r-x", 1, "QSYS", "0", 143_360, t, now, "ARPA"),
            "drwx---r-x   1 QSYS     0         143360 Oct 10 00:10 ARPA"
        );
        assert_eq!(
            line1("drwx---r-x", 1, "QSYS", "0", 39_321_600, t, now, "H"),
            "drwx---r-x   1 QSYS     0        39321600 Oct 10 00:10 H"
        );
        let old = civil_secs(2021, 10, 3, 1, 0, 0);
        assert_eq!(
            line1("-rwx---r-x", 1, "QSYS", "0", 14022, old, now, "ARPA.INET"),
            "-rwx---r-x   1 QSYS     0          14022 Oct 03 2021  ARPA.INET"
        );
        assert_eq!(mode_string(0o100_770, false), "-rwxrwx---");
        assert_eq!(mode_string(0o705, true), "drwx---r-x");
    }

    fn civil_secs(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64) -> i64 {
        // Days from civil (Howard Hinnant's algorithm).
        let y = if mo <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (mo + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        days * 86_400 + h * 3600 + mi * 60 + s
    }
}
