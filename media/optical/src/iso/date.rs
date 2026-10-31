// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! ISO 9660 dates: the 17-byte descriptor form and the 7-byte record
//! form, both UTC, from and to Unix seconds.

/// A civil UTC time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub struct Timestamp {
    /// Year.
    pub year: i32,
    /// 1 to 12.
    pub month: u8,
    /// 1 to 31.
    pub day: u8,
    /// 0 to 23.
    pub hour: u8,
    /// 0 to 59.
    pub minute: u8,
    /// 0 to 59.
    pub second: u8,
}

impl Timestamp {
    /// From Unix seconds.
    #[must_use]
    pub fn from_unix(epoch: u64) -> Self {
        let secs_total = i64::try_from(epoch).unwrap_or(0);
        let days = secs_total.div_euclid(86_400);
        let secs = secs_total.rem_euclid(86_400);
        let shifted = days + 719_468;
        let era = shifted.div_euclid(146_097);
        let doe = shifted.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        let b = |v: i64| u8::try_from(v).unwrap_or(0);
        Self {
            year: i32::try_from(year).unwrap_or(1970),
            month: b(month),
            day: b(day),
            hour: b(secs / 3600),
            minute: b((secs % 3600) / 60),
            second: b(secs % 60),
        }
    }

    /// To Unix seconds (0 before 1970).
    #[must_use]
    pub fn to_unix(&self) -> u64 {
        let y = i64::from(self.year) - i64::from(self.month <= 2);
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let m = i64::from(self.month);
        let mp = if m > 2 { m - 3 } else { m + 9 };
        let doy = (153 * mp + 2) / 5 + i64::from(self.day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        let secs = days * 86_400
            + i64::from(self.hour) * 3600
            + i64::from(self.minute) * 60
            + i64::from(self.second);
        u64::try_from(secs).unwrap_or(0)
    }

    /// The 17-byte descriptor form: `YYYYMMDDHHMMSScc` and a zero zone.
    #[must_use]
    pub fn long_form(&self) -> [u8; 17] {
        let text = format!(
            "{:04}{:02}{:02}{:02}{:02}{:02}00",
            self.year.clamp(0, 9999),
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second
        );
        let mut out = [0u8; 17];
        out[..16].copy_from_slice(&text.as_bytes()[..16]);
        out
    }

    /// The 7-byte record form.
    #[must_use]
    pub fn record_form(&self) -> [u8; 7] {
        [
            u8::try_from(self.year - 1900).unwrap_or(0),
            self.month,
            self.day,
            self.hour,
            self.minute,
            self.second,
            0,
        ]
    }

    /// From the record form (the zone byte is ignored).
    #[must_use]
    pub fn from_record(b: &[u8]) -> Self {
        let g = |i: usize| b.get(i).copied().unwrap_or(0);
        Self {
            year: 1900 + i32::from(g(0)),
            month: g(1),
            day: g(2),
            hour: g(3),
            minute: g(4),
            second: g(5),
        }
    }

    /// From the descriptor form; `None` when unset or not digits.
    #[must_use]
    pub fn from_long(b: &[u8]) -> Option<Self> {
        let digits = b.get(..14)?;
        if !digits.iter().all(u8::is_ascii_digit) || digits.iter().all(|&d| d == b'0') {
            return None;
        }
        let n = |r: std::ops::Range<usize>| {
            digits[r]
                .iter()
                .fold(0u32, |a, d| a * 10 + u32::from(d - b'0'))
        };
        let b8 = |v: u32| u8::try_from(v).unwrap_or(0);
        Some(Self {
            year: i32::try_from(n(0..4)).unwrap_or(0),
            month: b8(n(4..6)),
            day: b8(n(6..8)),
            hour: b8(n(8..10)),
            minute: b8(n(10..12)),
            second: b8(n(12..14)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for e in [0u64, 1_767_225_600, 951_782_400, 4_102_444_799] {
            let t = Timestamp::from_unix(e);
            assert_eq!(t.to_unix(), e);
            assert_eq!(Timestamp::from_record(&t.record_form()), t);
            assert_eq!(Timestamp::from_long(&t.long_form()), Some(t));
        }
        assert_eq!(
            &Timestamp::from_unix(1_767_225_600).long_form()[..16],
            b"2026010100000000"
        );
    }
}
