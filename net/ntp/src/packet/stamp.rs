// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! NTP timestamps: seconds since 1900 and a binary fraction, 32 bits
//! each. Era 0 ends in 2036; a timestamp whose top bit is clear is
//! taken as era 1 (RFC 4330 section 3), so times from 1968 to 2104
//! convert.

/// Seconds from 1900-01-01 to 1970-01-01.
pub const UNIX_OFFSET: i64 = 2_208_988_800;

const NANOS: i128 = 1_000_000_000;

/// A 64-bit NTP timestamp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Zero: "not known".
    pub const ZERO: Self = Self(0);

    /// From Unix nanoseconds.
    #[must_use]
    pub fn from_unix_nanos(ns: i64) -> Self {
        let ns = i128::from(ns) + i128::from(UNIX_OFFSET) * NANOS;
        let secs = ns.div_euclid(NANOS);
        let frac = ns.rem_euclid(NANOS);
        // Wraps into the 32-bit era on purpose.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let secs = secs as u32;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frac = ((frac << 32) / NANOS) as u32;
        Self((u64::from(secs) << 32) | u64::from(frac))
    }

    /// As Unix nanoseconds (era 1 when the top bit is clear).
    #[must_use]
    pub fn to_unix_nanos(self) -> i64 {
        let secs = i128::from(self.0 >> 32);
        let secs = if secs & 0x8000_0000 == 0 { secs + (1 << 32) } else { secs };
        let frac = i128::from(self.0 & 0xFFFF_FFFF);
        let ns = (secs - i128::from(UNIX_OFFSET)) * NANOS + ((frac * NANOS) >> 32);
        i64::try_from(ns).unwrap_or(i64::MAX)
    }

    /// Whole seconds of the timestamp.
    #[must_use]
    pub fn seconds(self) -> u32 {
        u32::try_from(self.0 >> 32).unwrap_or(0)
    }

    /// Whether it is zero.
    #[must_use]
    pub fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// A 32-bit "short format" value (root delay, root dispersion):
/// 16.16 fixed point seconds, as nanoseconds.
#[must_use]
pub fn short_to_nanos(v: u32) -> i64 {
    (i64::from(v) * 1_000_000_000) >> 16
}

/// Nanoseconds as the short format, clamped.
#[must_use]
pub fn nanos_to_short(ns: i64) -> u32 {
    let v = (i128::from(ns.max(0)) << 16) / NANOS;
    u32::try_from(v).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_round_trip() {
        for ns in [0i64, 1_000_000_000, 1_793_000_000_123_456_789, 2_085_978_496_000_000_000 + 5] {
            let t = Timestamp::from_unix_nanos(ns);
            let back = t.to_unix_nanos();
            assert!((back - ns).abs() <= 1, "{ns} came back {back}");
        }
        // The epoch of Unix is 2208988800 NTP seconds.
        assert_eq!(Timestamp::from_unix_nanos(0).seconds(), 2_208_988_800);
        // Half a second is 0x8000_0000 of fraction.
        assert_eq!(Timestamp::from_unix_nanos(500_000_000).0 & 0xFFFF_FFFF, 0x8000_0000);
    }

    #[test]
    fn era_one_after_2036() {
        // 2040-01-01T00:00:00Z.
        let ns = 2_208_988_800i64 * 1_000_000_000;
        let t = Timestamp::from_unix_nanos(ns);
        assert_eq!(t.0 >> 63, 0, "wrapped into era 1");
        assert_eq!(t.to_unix_nanos(), ns);
    }

    #[test]
    fn short_format() {
        assert_eq!(short_to_nanos(0x0001_0000), 1_000_000_000);
        assert_eq!(nanos_to_short(500_000_000), 0x0000_8000);
        assert_eq!(nanos_to_short(-5), 0);
    }
}
