// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The small structures: `fd_set`, `pollfd`, `timeval`, `linger`,
//! `iovec` and `msghdr` as their bytes in job storage. Pointer fields
//! are left to the caller, who knows what a pointer is; this module
//! gives their offsets.

use std::time::Duration;

use super::{i16_at, i32_at, put_i16, put_i32, u32_at};
use crate::consts::{FD_SET_WORDS, FD_SETSIZE};

/// `fd_set`: 200 bits in seven big-endian `unsigned int`s, bit
/// `fd % 32` of word `fd / 32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FdSet {
    words: [u32; FD_SET_WORDS],
}

impl FdSet {
    /// Empty.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// From its 28 bytes (fewer read as zero).
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Self {
        let mut s = Self::default();
        for (i, w) in s.words.iter_mut().enumerate() {
            *w = u32_at(b, i * 4);
        }
        s
    }

    /// Its 28 bytes.
    #[must_use]
    pub fn to_bytes(self) -> [u8; FD_SET_WORDS * 4] {
        let mut b = [0u8; FD_SET_WORDS * 4];
        for (i, w) in self.words.iter().enumerate() {
            b[i * 4..i * 4 + 4].copy_from_slice(&w.to_be_bytes());
        }
        b
    }

    /// `FD_ISSET`.
    #[must_use]
    pub fn contains(self, fd: i32) -> bool {
        match usize::try_from(fd) {
            Ok(n) if n < FD_SETSIZE => self.words[n / 32] & (1 << (n % 32)) != 0,
            _ => false,
        }
    }

    /// `FD_SET`.
    pub fn insert(&mut self, fd: i32) {
        if let Ok(n) = usize::try_from(fd)
            && n < FD_SETSIZE
        {
            self.words[n / 32] |= 1 << (n % 32);
        }
    }

    /// `FD_CLR`.
    pub fn remove(&mut self, fd: i32) {
        if let Ok(n) = usize::try_from(fd)
            && n < FD_SETSIZE
        {
            self.words[n / 32] &= !(1 << (n % 32));
        }
    }

    /// The members below `nfds`, ascending.
    #[must_use]
    pub fn members(self, nfds: i32) -> Vec<i32> {
        let top = usize::try_from(nfds).unwrap_or(0).min(FD_SETSIZE);
        (0..top)
            .filter(|&n| self.words[n / 32] & (1 << (n % 32)) != 0)
            .map(|n| i32::try_from(n).expect("small"))
            .collect()
    }
}

/// One `pollfd`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PollFd {
    /// Descriptor.
    pub fd: i32,
    /// Requested events.
    pub events: i16,
    /// Returned events.
    pub revents: i16,
}

impl PollFd {
    /// From its 8 bytes.
    #[must_use]
    pub fn from_bytes(b: &[u8]) -> Self {
        Self {
            fd: i32_at(b, 0),
            events: i16_at(b, 4),
            revents: i16_at(b, 6),
        }
    }

    /// Into 8 bytes.
    pub fn write(self, b: &mut [u8]) {
        put_i32(b, 0, self.fd);
        put_i16(b, 4, self.events);
        put_i16(b, 6, self.revents);
    }
}

/// `struct timeval` from its 8 bytes: seconds and microseconds, both
/// `long` (4 bytes). A negative value is `EINVAL` to `select`.
#[must_use]
pub fn timeval(b: &[u8]) -> Option<Duration> {
    let sec = i32_at(b, 0);
    let usec = i32_at(b, 4);
    if sec < 0 || !(0..1_000_000).contains(&usec) {
        return None;
    }
    Some(Duration::new(
        u64::try_from(sec).expect("non-negative"),
        u32::try_from(usec).expect("in range") * 1000,
    ))
}

/// Write a `struct timeval`.
pub fn put_timeval(b: &mut [u8], d: Duration) {
    put_i32(b, 0, i32::try_from(d.as_secs()).unwrap_or(i32::MAX));
    put_i32(b, 4, i32::try_from(d.subsec_micros()).unwrap_or(0));
}

/// `struct linger`: `(l_onoff != 0, l_linger seconds)`.
#[must_use]
pub fn linger(b: &[u8]) -> (bool, i32) {
    (i32_at(b, 0) != 0, i32_at(b, 4))
}

/// Write a `struct linger`.
pub fn put_linger(b: &mut [u8], on: bool, secs: i32) {
    put_i32(b, 0, i32::from(on));
    put_i32(b, 4, secs);
}

/// Offsets inside `struct iovec`.
pub mod iovec {
    /// `iov_base`, a pointer.
    pub const BASE: usize = 0;
    /// `iov_len`, a `size_t`.
    pub const LEN: usize = 16;
    /// `sizeof`.
    pub const SIZE: usize = 32;
}

/// Offsets inside `struct msghdr`.
pub mod msghdr {
    /// `msg_name`, a pointer.
    pub const NAME: usize = 0;
    /// `msg_namelen`.
    pub const NAMELEN: usize = 16;
    /// `msg_iov`, a pointer to `iovec`s.
    pub const IOV: usize = 32;
    /// `msg_iovlen`.
    pub const IOVLEN: usize = 48;
    /// `msg_control` (BSD 4.4) or `msg_accrights` (4.3), a pointer.
    pub const CONTROL: usize = 64;
    /// `msg_controllen` or `msg_accrightslen`.
    pub const CONTROLLEN: usize = 80;
    /// `msg_flags` (BSD 4.4 only).
    pub const FLAGS: usize = 84;
    /// `sizeof`.
    pub const SIZE: usize = 96;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fd_set_bits_are_ibm_s() {
        let mut s = FdSet::new();
        s.insert(0);
        s.insert(5);
        s.insert(33);
        s.insert(199);
        s.insert(200);
        s.insert(-1);
        let b = s.to_bytes();
        assert_eq!(&b[..4], &[0, 0, 0, 0x21]);
        assert_eq!(&b[4..8], &[0, 0, 0, 2]);
        assert_eq!(&b[24..28], &[0, 0, 0, 0x80], "bit 7 of word 6");
        assert!(s.contains(33));
        assert!(!s.contains(200));
        assert_eq!(FdSet::from_bytes(&b), s);
        assert_eq!(s.members(34), vec![0, 5, 33]);
        s.remove(5);
        assert_eq!(s.members(300), vec![0, 33, 199]);
    }

    #[test]
    fn pollfd_timeval_linger() {
        let mut b = [0u8; 8];
        PollFd {
            fd: 3,
            events: 1,
            revents: -1,
        }
        .write(&mut b);
        assert_eq!(b, [0, 0, 0, 3, 0, 1, 0xff, 0xff]);
        assert_eq!(PollFd::from_bytes(&b).revents, -1);
        put_timeval(&mut b, Duration::from_millis(1500));
        assert_eq!(b, [0, 0, 0, 1, 0, 0x07, 0xa1, 0x20]);
        assert_eq!(timeval(&b), Some(Duration::from_millis(1500)));
        put_i32(&mut b, 4, 1_000_000);
        assert_eq!(timeval(&b), None);
        put_linger(&mut b, true, 9);
        assert_eq!(linger(&b), (true, 9));
    }
}
