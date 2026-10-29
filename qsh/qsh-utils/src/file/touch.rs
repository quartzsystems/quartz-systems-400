// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `touch [-acm] [-r file | -t [[CC]YY]MMDDhhmm[.SS]] file...`: set
//! modification times, creating files that are missing.

use qsh::sys::{Errno, OpenFlags, UtilCtx};

use crate::common::args::getopt;
use crate::common::time::{days_in_month, to_epoch, utc};
use crate::common::{complain_sys, usage};

/// `[[CC]YY]MMDDhhmm[.SS]` to seconds since the epoch (UTC).
fn parse_time(s: &str, now: i64) -> Option<i64> {
    let (main, sec) = match s.split_once('.') {
        Some((m, s)) => (m, s.parse::<u32>().ok().filter(|v| *v <= 60)?),
        None => (s, 0),
    };
    if !main.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let num = |s: &str| s.parse::<u32>().ok();
    let (year, rest) = match main.len() {
        8 => (utc(now).year, main),
        10 => {
            let yy = num(&main[..2])?;
            let year = if yy >= 69 { 1900 + yy } else { 2000 + yy };
            (i64::from(year), &main[2..])
        }
        12 => (i64::from(num(&main[..4])?), &main[4..]),
        _ => return None,
    };
    let mo = num(&rest[..2])?;
    let d = num(&rest[2..4])?;
    let h = num(&rest[4..6])?;
    let mi = num(&rest[6..8])?;
    if !(1..=12).contains(&mo) || d == 0 || d > days_in_month(year, mo) || h > 23 || mi > 59 {
        return None;
    }
    Some(to_epoch(year, mo, d, h, mi, sec))
}

/// Run `touch`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "touch";
    let p = match getopt(&cx.argv[1..], "acmr:t:") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    if p.operands.is_empty() {
        return usage(cx, name, "A file name is needed.");
    }
    let now = i64::try_from(cx.sys.now().as_secs()).unwrap_or(0);
    let when = if let Some(r) = p.value('r') {
        match cx.sys.stat(&cx.path(r)) {
            Ok(st) => st.mtime,
            Err(e) => {
                complain_sys(cx, name, r, e);
                return 1;
            }
        }
    } else if let Some(t) = p.value('t') {
        match parse_time(t, now) {
            Some(v) => v,
            None => return usage(cx, name, &format!("The time {t} is not valid.")),
        }
    } else {
        now
    };
    let no_create = p.has('c');
    let mut status = 0;
    for f in &p.operands {
        let abs = cx.path(f);
        if cx.sys.stat(&abs).is_err() {
            if no_create {
                continue;
            }
            let flags = OpenFlags {
                write: true,
                create: true,
                ..OpenFlags::default()
            };
            match cx.sys.open(&abs, flags, 0o666 & !cx.umask) {
                Ok(_) => {}
                Err(e) => {
                    complain_sys(cx, name, f, e);
                    status = 1;
                    continue;
                }
            }
        }
        if let Err(e) = cx.sys.utimes(&abs, when)
            && e.errno != Errno::ENOSYS {
                complain_sys(cx, name, f, e);
                status = 1;
            }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times() {
        assert_eq!(parse_time("202610291230", 0), Some(to_epoch(2026, 10, 29, 12, 30, 0)));
        assert_eq!(parse_time("2610291230.15", 0), Some(to_epoch(2026, 10, 29, 12, 30, 15)));
        assert_eq!(parse_time("7001010000", 0), Some(0));
        let now = to_epoch(2026, 5, 1, 0, 0, 0);
        assert_eq!(parse_time("10291230", now), Some(to_epoch(2026, 10, 29, 12, 30, 0)));
        assert_eq!(parse_time("13011200", now), None);
        assert_eq!(parse_time("abc", now), None);
    }
}
