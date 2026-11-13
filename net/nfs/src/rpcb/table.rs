// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The registration table and the counts `RPCBPROC_GETSTAT` reports.
//!
//! A lookup by version finds that version, or, as rpcbind does for
//! portmap's `GETPORT` and rpcbind's `GETADDR`, another version of the
//! program on the same netid; `GETVERSADDR` takes only the version.

use crate::types::rpcb::{RPCBSTAT_HIGHPROC, RPCBVERS_STAT, rpcb};

/// The registrations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Table {
    entries: Vec<rpcb>,
}

/// The owner a superuser registration names.
pub const SUPERUSER: &[u8] = b"0";

impl Table {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// The registrations, in the order made.
    pub fn entries(&self) -> &[rpcb] {
        &self.entries
    }

    /// Registers `r`; false when its program, version and netid are taken.
    pub fn set(&mut self, r: rpcb) -> bool {
        let taken = self.entries.iter().any(|e| {
            e.r_prog == r.r_prog
                && e.r_vers == r.r_vers
                && e.r_netid.eq_ignore_ascii_case(&r.r_netid)
        });
        if taken || r.r_netid.is_empty() || r.r_addr.is_empty() {
            return false;
        }
        self.entries.push(r);
        true
    }

    /// Removes the registrations of `prog` and `vers` on `netid` (every
    /// netid when empty) that `owner` made (any, for the superuser);
    /// false when none was removed.
    pub fn unset(&mut self, prog: u32, vers: u32, netid: &[u8], owner: &[u8]) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| {
            let hit = e.r_prog == prog
                && e.r_vers == vers
                && (netid.is_empty() || e.r_netid.eq_ignore_ascii_case(netid))
                && (owner == SUPERUSER || e.r_owner == owner);
            !hit
        });
        self.entries.len() != before
    }

    /// The registration of `prog` on `netid`: `vers` if there is one,
    /// else (unless `exact`) another version.
    pub fn find(&self, prog: u32, vers: u32, netid: &[u8], exact: bool) -> Option<&rpcb> {
        let mut hit = None;
        for e in self
            .entries
            .iter()
            .filter(|e| e.r_prog == prog && e.r_netid.eq_ignore_ascii_case(netid))
        {
            if e.r_vers == vers {
                return Some(e);
            }
            hit = Some(e);
        }
        if exact { None } else { hit }
    }
}

/// Counts for one binder version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersStats {
    /// Calls of each procedure.
    pub info: [i32; RPCBSTAT_HIGHPROC as usize],
    /// Registrations made.
    pub setinfo: i32,
    /// Registrations removed.
    pub unsetinfo: i32,
    /// Lookups: (program, version, netid, found, not found).
    pub addrinfo: Vec<(u32, u32, Vec<u8>, i32, i32)>,
}

impl Default for VersStats {
    fn default() -> Self {
        Self {
            info: [0; RPCBSTAT_HIGHPROC as usize],
            setinfo: 0,
            unsetinfo: 0,
            addrinfo: Vec::new(),
        }
    }
}

/// Counts for versions 2, 3 and 4.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    /// By version, 2 first.
    pub by_vers: [VersStats; RPCBVERS_STAT as usize],
}

impl Stats {
    fn of(&mut self, vers: u32) -> Option<&mut VersStats> {
        self.by_vers
            .get_mut(usize::try_from(vers.checked_sub(2)?).ok()?)
    }

    /// A call of `procedure` in `vers`.
    pub fn call(&mut self, vers: u32, procedure: u32) {
        if let Some(s) = self.of(vers)
            && let Some(n) = s.info.get_mut(procedure as usize)
        {
            *n = n.saturating_add(1);
        }
    }

    /// A registration made (`set`) or removed.
    pub fn changed(&mut self, vers: u32, set: bool) {
        if let Some(s) = self.of(vers) {
            let n = if set {
                &mut s.setinfo
            } else {
                &mut s.unsetinfo
            };
            *n = n.saturating_add(1);
        }
    }

    /// A lookup and whether it found an address. At most 64 distinct
    /// lookups are counted per version.
    pub fn lookup(&mut self, vers: u32, prog: u32, pvers: u32, netid: &[u8], found: bool) {
        let Some(s) = self.of(vers) else { return };
        let i = match s
            .addrinfo
            .iter()
            .position(|a| a.0 == prog && a.1 == pvers && a.2 == netid)
        {
            Some(i) => i,
            None if s.addrinfo.len() < 64 => {
                s.addrinfo.push((prog, pvers, netid.to_vec(), 0, 0));
                s.addrinfo.len() - 1
            }
            None => return,
        };
        if let Some(a) = s.addrinfo.get_mut(i) {
            let n = if found { &mut a.3 } else { &mut a.4 };
            *n = n.saturating_add(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg(vers: u32, netid: &str, owner: &str) -> rpcb {
        rpcb {
            r_prog: 100_003,
            r_vers: vers,
            r_netid: netid.into(),
            r_addr: b"0.0.0.0.8.1".to_vec(),
            r_owner: owner.into(),
        }
    }

    #[test]
    fn set_find_unset() {
        let mut t = Table::new();
        assert!(t.set(reg(3, "tcp", "4294967295")));
        assert!(!t.set(reg(3, "tcp", "4294967295")));
        assert!(t.set(reg(4, "tcp", "4294967295")));
        assert!(t.set(reg(3, "udp", "4294967295")));
        assert_eq!(t.find(100_003, 4, b"tcp", true).map(|r| r.r_vers), Some(4));
        assert_eq!(t.find(100_003, 2, b"tcp", true), None);
        assert!(t.find(100_003, 2, b"tcp", false).is_some());
        assert!(!t.unset(100_003, 3, b"", b"someone"));
        assert!(t.unset(100_003, 3, b"", b"4294967295"));
        assert_eq!(t.entries().len(), 1);
        assert!(t.unset(100_003, 4, b"tcp", SUPERUSER));
        assert!(t.entries().is_empty());
    }

    #[test]
    fn stats() {
        let mut s = Stats::default();
        s.call(4, 3);
        s.call(9, 3);
        s.call(4, 99);
        s.lookup(4, 100_003, 3, b"tcp", true);
        s.lookup(4, 100_003, 3, b"tcp", false);
        assert_eq!(s.by_vers[2].info[3], 1);
        assert_eq!(
            s.by_vers[2].addrinfo,
            vec![(100_003, 3, b"tcp".to_vec(), 1, 1)]
        );
    }
}
