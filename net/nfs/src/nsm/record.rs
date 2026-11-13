// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the status monitor keeps across starts, and where
//! ([`StatdStore`]; the job keeps them in the IFS):
//!
//! - `/etc/STATD`: the state number as EBCDIC digits and a new line
//!   (`X'15'`), as the oracle's file is (`X'F115'`); [`encode_state`].
//! - `/QIBM/UserData/OS400/NFS/QNFSNSMD.MON`: the hosts monitored, a
//!   format of this release ([`encode_monitored`]): the magic `QSMN`,
//!   the version (u16, 1), two bytes of zero, the count (u32), then per
//!   host its name, the local name to call back, (u16 length and the
//!   bytes, at most 1024), the program, version and procedure to call
//!   (three u32) and the 16 private bytes. Big-endian.

use std::io;
use std::sync::Mutex;

/// The monitored hosts file's magic.
pub const MAGIC: &[u8; 4] = b"QSMN";

/// Its version.
pub const VERSION: u16 = 1;

/// The longest name.
pub const MAX_NAME: usize = 1024;

/// One monitored host and whom to tell when it restarts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonRecord {
    /// The host monitored.
    pub mon_name: Vec<u8>,
    /// The local host to call back.
    pub my_name: Vec<u8>,
    /// The program to call back.
    pub my_prog: u32,
    /// Its version.
    pub my_vers: u32,
    /// Its procedure.
    pub my_proc: u32,
    /// What the callback carries back.
    pub private: [u8; 16],
}

/// `/etc/STATD`'s bytes for `state`.
#[must_use]
pub fn encode_state(state: u32) -> Vec<u8> {
    let mut v: Vec<u8> = state.to_string().bytes().map(|d| d - b'0' + 0xF0).collect();
    v.push(0x15);
    v
}

/// The state in `/etc/STATD`'s bytes (EBCDIC digits, then `X'15'` or
/// blanks), or `None` when it is not one.
#[must_use]
pub fn decode_state(b: &[u8]) -> Option<u32> {
    let digits: Vec<u8> = b
        .iter()
        .take_while(|c| (0xF0..=0xF9).contains(*c))
        .map(|c| c - 0xF0 + b'0')
        .collect();
    let rest = &b[digits.len()..];
    if digits.is_empty() || !rest.iter().all(|c| matches!(c, 0x15 | 0x40 | 0x25 | 0)) {
        return None;
    }
    std::str::from_utf8(&digits).ok()?.parse().ok()
}

fn put_name(out: &mut Vec<u8>, n: &[u8]) {
    let n = &n[..n.len().min(MAX_NAME)];
    out.extend_from_slice(&u16::try_from(n.len()).unwrap_or(0).to_be_bytes());
    out.extend_from_slice(n);
}

/// The monitored hosts file's bytes.
#[must_use]
pub fn encode_monitored(list: &[MonRecord]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_be_bytes());
    out.extend_from_slice(&[0, 0]);
    out.extend_from_slice(&u32::try_from(list.len()).unwrap_or(0).to_be_bytes());
    for r in list {
        put_name(&mut out, &r.mon_name);
        put_name(&mut out, &r.my_name);
        for n in [r.my_prog, r.my_vers, r.my_proc] {
            out.extend_from_slice(&n.to_be_bytes());
        }
        out.extend_from_slice(&r.private);
    }
    out
}

struct Cur<'a>(&'a [u8]);

impl Cur<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if self.0.len() < n {
            return None;
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Some(a)
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_be_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_be_bytes(self.take(4)?.try_into().ok()?))
    }
    fn name(&mut self) -> Option<Vec<u8>> {
        let n = usize::from(self.u16()?);
        if n > MAX_NAME {
            return None;
        }
        Some(self.take(n)?.to_vec())
    }
}

/// The monitored hosts in the file's bytes.
///
/// # Errors
///
/// `InvalidData` for another magic or version, or bytes that end early
/// or are left over.
pub fn decode_monitored(b: &[u8]) -> io::Result<Vec<MonRecord>> {
    let bad = || io::Error::new(io::ErrorKind::InvalidData, "not a monitored hosts file");
    let mut c = Cur(b);
    if c.take(4) != Some(MAGIC.as_slice()) || c.u16() != Some(VERSION) || c.u16() != Some(0) {
        return Err(bad());
    }
    let n = c.u32().ok_or_else(bad)?;
    let mut out = Vec::new();
    for _ in 0..n {
        let r = (|| {
            Some(MonRecord {
                mon_name: c.name()?,
                my_name: c.name()?,
                my_prog: c.u32()?,
                my_vers: c.u32()?,
                my_proc: c.u32()?,
                private: c.take(16)?.try_into().ok()?,
            })
        })()
        .ok_or_else(bad)?;
        out.push(r);
    }
    if !c.0.is_empty() {
        return Err(bad());
    }
    Ok(out)
}

/// Where the monitor's state and hosts are kept.
pub trait StatdStore: Send + Sync {
    /// `/etc/STATD`'s bytes, `None` when there is no file.
    ///
    /// # Errors
    ///
    /// The file system's.
    fn read_state(&self) -> io::Result<Option<Vec<u8>>>;
    /// Writes `/etc/STATD`.
    ///
    /// # Errors
    ///
    /// The file system's.
    fn write_state(&self, bytes: &[u8]) -> io::Result<()>;
    /// The monitored hosts file's bytes, `None` when there is none.
    ///
    /// # Errors
    ///
    /// The file system's.
    fn read_monitored(&self) -> io::Result<Option<Vec<u8>>>;
    /// Writes the monitored hosts file.
    ///
    /// # Errors
    ///
    /// The file system's.
    fn write_monitored(&self, bytes: &[u8]) -> io::Result<()>;
}

/// Both files in memory.
#[derive(Debug, Default)]
pub struct MemStatdStore {
    /// `/etc/STATD`.
    pub state: Mutex<Option<Vec<u8>>>,
    /// The monitored hosts.
    pub monitored: Mutex<Option<Vec<u8>>>,
}

impl StatdStore for MemStatdStore {
    fn read_state(&self) -> io::Result<Option<Vec<u8>>> {
        Ok(self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone())
    }
    fn write_state(&self, bytes: &[u8]) -> io::Result<()> {
        *self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(bytes.to_vec());
        Ok(())
    }
    fn read_monitored(&self) -> io::Result<Option<Vec<u8>>> {
        Ok(self.monitored.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone())
    }
    fn write_monitored(&self, bytes: &[u8]) -> io::Result<()> {
        *self.monitored.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(bytes.to_vec());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statd_is_the_oracles_form() {
        assert_eq!(encode_state(1), [0xF1, 0x15]);
        assert_eq!(decode_state(&[0xF1, 0x15]), Some(1));
        assert_eq!(decode_state(&encode_state(4_294_967_295)), Some(4_294_967_295));
        assert_eq!(decode_state(&[0xF1, 0xC1]), None);
        assert_eq!(decode_state(&[]), None);
    }

    #[test]
    fn monitored_round_trip() {
        let l = vec![MonRecord {
            mon_name: b"client1".to_vec(),
            my_name: b"localhost".to_vec(),
            my_prog: 100_021,
            my_vers: 2,
            my_proc: 1,
            private: [7; 16],
        }];
        let b = encode_monitored(&l);
        assert_eq!(&b[..4], MAGIC);
        assert_eq!(decode_monitored(&b).unwrap(), l);
        assert!(decode_monitored(&b[..b.len() - 1]).is_err());
        let mut more = b.clone();
        more.push(0);
        assert!(decode_monitored(&more).is_err());
        assert_eq!(decode_monitored(&encode_monitored(&[])).unwrap(), vec![]);
    }
}
