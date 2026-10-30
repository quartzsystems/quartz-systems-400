// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Paths made absolute against the session's directory, IFS `stat` as
//! SFTP attributes, and the `ls -l` line `READDIR` returns (clients
//! show it as is).

use ifs::ops::Stat;
use ifs::ops::flags::{S_IFDIR, S_IFLNK, S_IFREG};

use super::proto::Attrs;

/// `p` made absolute against `cwd`, `.` and `..` folded.
#[must_use]
pub fn absolute(cwd: &str, p: &str) -> String {
    let joined = if p.starts_with('/') {
        p.to_string()
    } else if p.is_empty() || p == "." {
        cwd.to_string()
    } else {
        format!("{}/{p}", cwd.trim_end_matches('/'))
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    format!("/{}", parts.join("/"))
}

/// The POSIX type bits for an IFS mode (objects other than files,
/// directories and links show as regular files).
#[must_use]
pub fn posix_mode(mode: u32) -> u32 {
    let t = mode & 0o170_000;
    let t = if t == 0 { S_IFREG } else { t };
    t | (mode & 0o7777)
}

fn secs(ns: i64) -> u32 {
    u32::try_from(ns.div_euclid(1_000_000_000)).unwrap_or(0)
}

/// SFTP attributes for a `stat`.
#[must_use]
pub fn attrs(st: &Stat) -> Attrs {
    Attrs {
        size: Some(st.size),
        uid_gid: Some((st.uid, st.gid)),
        permissions: Some(posix_mode(st.mode)),
        times: Some((secs(st.atime), secs(st.mtime))),
    }
}

fn perm_string(mode: u32) -> String {
    let t = match mode & 0o170_000 {
        S_IFDIR => 'd',
        S_IFLNK => 'l',
        0o010_000 => 'p',
        0o140_000 => 's',
        0o020_000 => 'c',
        0o060_000 => 'b',
        _ => '-',
    };
    let mut s = String::from(t);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 7;
        s.push(if bits & 4 != 0 { 'r' } else { '-' });
        s.push(if bits & 2 != 0 { 'w' } else { '-' });
        s.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    s
}

/// Year, month (1-12) and day of a day count since 1970-01-01.
#[must_use]
pub fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1);
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1);
    (if m <= 2 { y + 1 } else { y }, m, d)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The `ls -l` line for `name`; `now` in Unix seconds picks the time or
/// the year, as `ls` does for files older than six months.
#[must_use]
pub fn longname(name: &str, st: &Stat, now: i64) -> String {
    let t = st.mtime.div_euclid(1_000_000_000);
    let (y, m, d) = civil(t.div_euclid(86_400));
    let sod = t.rem_euclid(86_400);
    let when = if (now - t).abs() < 182 * 86_400 {
        format!("{:02}:{:02}", sod / 3600, (sod % 3600) / 60)
    } else {
        format!(" {y}")
    };
    let month = MONTHS[(m as usize).saturating_sub(1) % 12];
    format!(
        "{} {:>4} {:<8} {:<8} {:>8} {month} {d:>2} {when:>5} {name}",
        perm_string(posix_mode(st.mode)),
        st.nlink,
        st.owner,
        st.group.as_deref().unwrap_or("*NONE"),
        st.size,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_dates() {
        assert_eq!(absolute("/home/QSTEST", "a/../b/./c"), "/home/QSTEST/b/c");
        assert_eq!(absolute("/home/QSTEST", "."), "/home/QSTEST");
        assert_eq!(absolute("/home/QSTEST", "/QSYS.LIB/../tmp"), "/tmp");
        assert_eq!(absolute("/", ".."), "/");
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_755), (2026, 10, 29));
        assert_eq!(perm_string(0o040_755), "drwxr-xr-x");
        assert_eq!(posix_mode(0o200_000 | 0o644), 0o100_644);
    }
}
