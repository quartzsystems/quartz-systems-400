// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Storage pools and their activity levels. A pool's activity level is
//! how many of its jobs may run at once: a job takes a level when it
//! becomes runnable and gives it back at every wait. A job that finds
//! none free is ineligible (`INEL`) until one is. The transitions are
//! counted for `WRKSYSSTS`: active to wait, wait to ineligible, active
//! to ineligible (time slice end; not modelled, always 0).
//!
//! Sizes are memory cgroups' `memory.high` (see `docs/WORKMGMT.md`);
//! this table only keeps the numbers and the gate.

use std::collections::{BTreeMap, BTreeSet};

use crate::sbs::PoolInfo;

/// The shared pools, with their system pool numbers.
pub const SHARED: &[(&str, u16)] = &[
    ("*MACHINE", 1),
    ("*BASE", 2),
    ("*INTERACT", 3),
    ("*SPOOL", 4),
];

/// The number `*SHRPOOLn` gets.
#[must_use]
pub fn shrpool_number(n: u8) -> u16 {
    4 + u16::from(n)
}

#[derive(Debug, Default)]
struct Pool {
    info: PoolInfo,
    holders: BTreeSet<u32>,
    waiting: BTreeSet<u32>,
}

/// Every pool.
#[derive(Debug, Default)]
pub struct PoolTable {
    pools: BTreeMap<String, Pool>,
    next_private: u16,
}

impl PoolTable {
    /// The shared pools: `*MACHINE` and `*BASE` from their system values
    /// (kilobytes, and `QBASACTLVL`), `*INTERACT` and `*SPOOL` with
    /// starting sizes, `*SHRPOOL1`-`60` undefined.
    #[must_use]
    pub fn new(machine_kb: u64, base_kb: u64, base_actlvl: u32) -> Self {
        let mut t = Self {
            pools: BTreeMap::new(),
            next_private: 65,
        };
        let mut add = |name: &str, number, size_kb, actlvl| {
            t.pools.insert(
                name.to_string(),
                Pool {
                    info: PoolInfo {
                        name: name.to_string(),
                        number,
                        size_kb,
                        reserved_kb: if number == 1 { size_kb / 4 } else { 0 },
                        actlvl,
                        ..PoolInfo::default()
                    },
                    ..Pool::default()
                },
            );
        };
        add("*MACHINE", 1, machine_kb, 0);
        add("*BASE", 2, base_kb, base_actlvl);
        add("*INTERACT", 3, 65_536, 20);
        add("*SPOOL", 4, 16_384, 5);
        for n in 1..=60u8 {
            add(&format!("*SHRPOOL{n}"), shrpool_number(n), 0, 0);
        }
        t
    }

    /// Define or change a pool's size and activity level
    /// (`CHGSHRPOOL`, a subsystem's private pool). A private pool new to
    /// the table gets the next free system number.
    pub fn define(&mut self, name: &str, size_kb: u64, actlvl: u32) {
        if let Some(p) = self.pools.get_mut(name) {
            p.info.size_kb = size_kb;
            p.info.actlvl = actlvl;
            return;
        }
        let number = self.next_private;
        self.next_private += 1;
        self.pools.insert(
            name.to_string(),
            Pool {
                info: PoolInfo {
                    name: name.to_string(),
                    number,
                    size_kb,
                    actlvl,
                    ..PoolInfo::default()
                },
                ..Pool::default()
            },
        );
    }

    /// A private pool is gone with its subsystem.
    pub fn drop_private(&mut self, name: &str) {
        if !name.starts_with('*') {
            self.pools.remove(name);
        }
    }

    /// A job of `pool` reports whether it is runnable; whether it holds
    /// an activity level now. A pool with level 0 has no limit; a job
    /// with no pool always runs.
    pub fn status(&mut self, pool: &str, job: u32, runnable: bool) -> bool {
        let Some(p) = self.pools.get_mut(pool) else {
            return true;
        };
        let holds = p.holders.contains(&job);
        match (runnable, holds) {
            (true, true) => true,
            (true, false) => {
                let free = p.info.actlvl == 0 || p.holders.len() < p.info.actlvl as usize;
                if free {
                    p.waiting.remove(&job);
                    p.holders.insert(job);
                    true
                } else {
                    if p.waiting.insert(job) {
                        p.info.wait_inel += 1;
                    }
                    false
                }
            }
            (false, true) => {
                p.holders.remove(&job);
                p.info.act_wait += 1;
                true
            }
            (false, false) => {
                p.waiting.remove(&job);
                true
            }
        }
    }

    /// A job ended: whatever it held or waited for goes.
    pub fn remove(&mut self, job: u32) {
        for p in self.pools.values_mut() {
            p.holders.remove(&job);
            p.waiting.remove(&job);
        }
    }

    /// Every defined pool, by system number.
    #[must_use]
    pub fn info(&self) -> Vec<PoolInfo> {
        let mut v: Vec<PoolInfo> = self
            .pools
            .values()
            .filter(|p| p.info.size_kb > 0 || p.info.number <= 2)
            .map(|p| PoolInfo {
                active: u32::try_from(p.holders.len()).unwrap_or(u32::MAX),
                ineligible: u32::try_from(p.waiting.len()).unwrap_or(u32::MAX),
                ..p.info.clone()
            })
            .collect();
        v.sort_by_key(|p| p.number);
        v
    }

    /// A pool's definition.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&PoolInfo> {
        self.pools.get(name).map(|p| &p.info)
    }

    /// Zero the transition counters (`WRKSYSSTS` F10).
    pub fn reset(&mut self) {
        for p in self.pools.values_mut() {
            p.info.act_wait = 0;
            p.info.wait_inel = 0;
            p.info.act_inel = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gate() {
        let mut t = PoolTable::new(100_000, 400_000, 2);
        assert!(t.status("*BASE", 1, true));
        assert!(t.status("*BASE", 2, true));
        assert!(!t.status("*BASE", 3, true), "third job is ineligible");
        assert!(
            !t.status("*BASE", 3, true),
            "still ineligible, counted once"
        );
        assert!(t.status("*BASE", 1, false), "job 1 waits");
        assert!(t.status("*BASE", 3, true), "job 3 gets the level");
        let b = t.get("*BASE").unwrap().clone();
        assert_eq!((b.act_wait, b.wait_inel), (1, 1));
        let info = t.info();
        let base = info.iter().find(|p| p.name == "*BASE").unwrap();
        assert_eq!((base.active, base.ineligible), (2, 0));
        t.remove(2);
        assert!(t.status("*BASE", 1, true));
        assert!(t.status("NOPOOL", 9, true));
        t.define("QBATCH/2", 8192, 1);
        assert_eq!(t.get("QBATCH/2").unwrap().number, 65);
        assert!(t.info().iter().all(|p| p.name != "*SHRPOOL7"));
        t.reset();
        assert_eq!(t.get("*BASE").unwrap().act_wait, 0);
    }
}
