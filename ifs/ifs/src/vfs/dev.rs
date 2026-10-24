// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Character special files (`*CHRSF`) in `/dev` and `/QOpenSys/dev`,
//! emulated: nothing reaches a Linux device. `/dev` also holds one
//! directory per ASP (`QASP01`) listing its user-defined file systems.

use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::errno::{Errno, Result};

/// A device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DevKind {
    /// Reads end at once, writes vanish.
    Null,
    /// Reads give zeros, writes vanish.
    Zero,
    /// The job's terminal: a server job has none (`ENXIO`).
    Tty,
    /// Random bytes.
    Random,
    /// Random bytes.
    Urandom,
    /// Reads give zeros, writes fail `ENOSPC`.
    Full,
}

/// Every device with its name, in listing order.
pub const DEVICES: [(&str, DevKind); 6] = [
    ("full", DevKind::Full),
    ("null", DevKind::Null),
    ("random", DevKind::Random),
    ("tty", DevKind::Tty),
    ("urandom", DevKind::Urandom),
    ("zero", DevKind::Zero),
];

/// The system ASP's directory in `/dev`.
pub const SYSTEM_ASP: &str = "QASP01";

impl DevKind {
    /// By name.
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        DEVICES.iter().find(|(n, _)| *n == name).map(|(_, d)| *d)
    }

    /// The name.
    #[must_use]
    pub fn name(self) -> &'static str {
        DEVICES
            .iter()
            .find(|(_, d)| *d == self)
            .map(|(n, _)| *n)
            .expect("listed")
    }

    /// A stable file number.
    #[must_use]
    pub fn ino(self) -> u64 {
        DEVICES
            .iter()
            .position(|(_, d)| *d == self)
            .expect("listed") as u64
            + 2
    }

    /// `st_rdev`: Linux's major and minor numbers for the same device.
    #[must_use]
    pub fn rdev(self) -> u64 {
        let (maj, min) = match self {
            Self::Null => (1, 3),
            Self::Zero => (1, 5),
            Self::Full => (1, 7),
            Self::Random => (1, 8),
            Self::Urandom => (1, 9),
            Self::Tty => (5, 0),
        };
        (maj << 8) | min
    }

    /// Read into `buf`.
    pub fn read(self, buf: &mut [u8]) -> Result<usize> {
        match self {
            Self::Null => Ok(0),
            Self::Zero | Self::Full => {
                buf.fill(0);
                Ok(buf.len())
            }
            Self::Random | Self::Urandom => {
                fill_random(buf);
                Ok(buf.len())
            }
            Self::Tty => Err(Errno::ENXIO),
        }
    }

    /// Write `buf`.
    pub fn write(self, buf: &[u8]) -> Result<usize> {
        match self {
            Self::Null | Self::Zero | Self::Random | Self::Urandom => Ok(buf.len()),
            Self::Full => Err(Errno::ENOSPC),
            Self::Tty => Err(Errno::ENXIO),
        }
    }
}

/// Random bytes: a keyed hash (`SipHash`, keyed from the operating
/// system's randomness once per process) of a counter.
pub fn fill_random(buf: &mut [u8]) {
    static KEY: OnceLock<RandomState> = OnceLock::new();
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let key = KEY.get_or_init(RandomState::new);
    for chunk in buf.chunks_mut(8) {
        let mut h = key.build_hasher();
        h.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
        chunk.copy_from_slice(&h.finish().to_le_bytes()[..chunk.len()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn devices() {
        let mut b = [7u8; 16];
        assert_eq!(DevKind::Null.read(&mut b).unwrap(), 0);
        assert_eq!(DevKind::Zero.read(&mut b).unwrap(), 16);
        assert_eq!(b, [0; 16]);
        assert_eq!(DevKind::Full.write(b"x"), Err(Errno::ENOSPC));
        assert_eq!(DevKind::Null.write(b"xyz").unwrap(), 3);
        assert_eq!(DevKind::Tty.read(&mut b), Err(Errno::ENXIO));
        let mut r1 = [0u8; 13];
        let mut r2 = [0u8; 13];
        DevKind::Urandom.read(&mut r1).unwrap();
        DevKind::Random.read(&mut r2).unwrap();
        assert_ne!(r1, r2);
        assert_eq!(DevKind::by_name("zero"), Some(DevKind::Zero));
        assert_eq!(DevKind::Zero.name(), "zero");
        assert_eq!(DevKind::Null.rdev(), 0x103);
    }
}
