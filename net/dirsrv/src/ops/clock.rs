// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the operations ask of the world: the time and random bytes
//! ([`Env`]), and Generalized Time as the server writes it.

use std::time::{SystemTime, UNIX_EPOCH};

/// The clock and the random source. The service passes [`SystemEnv`];
/// tests pass a fixed clock.
pub trait Env {
    /// Microseconds since the Unix epoch.
    fn now_micros(&self) -> u64;
    /// Fill `buf` with random bytes.
    fn random(&self, buf: &mut [u8]);
}

/// The system clock and `getrandom`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemEnv;

impl Env for SystemEnv {
    fn now_micros(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
    }

    fn random(&self, buf: &mut [u8]) {
        if getrandom::fill(buf).is_err() {
            // No entropy source: fall back on the clock, never on zeros.
            let mut x = self.now_micros() | 1;
            for b in buf.iter_mut() {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                *b = x.to_le_bytes()[0];
            }
        }
    }
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// `YYYYMMDDHHMMSS.ffffffZ`, as IBM's server writes timestamps.
#[must_use]
pub fn gentime(micros: u64) -> String {
    let secs = i64::try_from(micros / 1_000_000).unwrap_or(i64::MAX);
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let sod = secs.rem_euclid(86_400);
    format!(
        "{y:04}{m:02}{d:02}{:02}{:02}{:02}.{:06}Z",
        sod / 3600,
        (sod / 60) % 60,
        sod % 60,
        micros % 1_000_000
    )
}

/// The Unix microseconds of a Generalized Time value (`Z` or an offset,
/// minutes and seconds optional, a fraction of the last field given);
/// `None` when it is not one.
#[must_use]
pub fn parse_gentime(v: &[u8]) -> Option<u64> {
    let s = std::str::from_utf8(v).ok()?.trim();
    let num = |a: usize, n: usize| -> Option<i64> {
        let t = s.get(a..a + n)?;
        t.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| t.parse().ok())?
    };
    let y = num(0, 4)?;
    let mo = num(4, 2)?;
    let d = num(6, 2)?;
    let h = num(8, 2)?;
    let mut at = 10;
    let mi = num(at, 2).inspect(|_| at += 2).unwrap_or(0);
    let se = if at == 12 {
        num(at, 2).inspect(|_| at += 2).unwrap_or(0)
    } else {
        0
    };
    let mut frac = 0i64;
    if matches!(s.as_bytes().get(at), Some(b'.' | b',')) {
        at += 1;
        let start = at;
        while s.as_bytes().get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        let digits = &s[start..at];
        let mut f: i64 = 0;
        for (i, c) in digits.bytes().take(6).enumerate() {
            f += i64::from(c - b'0') * 10i64.pow(5 - u32::try_from(i).ok()?);
        }
        frac = f;
    }
    let off = match s.as_bytes().get(at) {
        Some(b'Z') | None => 0,
        Some(c @ (b'+' | b'-')) => {
            let oh = num(at + 1, 2)?;
            let om = num(at + 3, 2).unwrap_or(0);
            let o = oh * 3600 + om * 60;
            if *c == b'+' { -o } else { o }
        }
        _ => return None,
    };
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 60 {
        return None;
    }
    let secs = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + se + off;
    u64::try_from(secs * 1_000_000 + frac).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let t = 1_789_387_085_748_334;
        let g = gentime(t);
        assert_eq!(g, "20260914115805.748334Z");
        assert_eq!(parse_gentime(g.as_bytes()), Some(t));
        assert_eq!(parse_gentime(b"19700101000000Z"), Some(0));
        assert_eq!(parse_gentime(b"197001010100+0100"), Some(0));
        assert_eq!(parse_gentime(b"junk"), None);
    }
}
