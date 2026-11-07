// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QPFRADJ`: the shared pools' sizes and activity levels adjusted from
//! the collector's figures. Each pool has a fault guideline (a minimum,
//! a part per active thread, a maximum, as `WRKSHRPOOL` F11 shows): a
//! pool faulting above its guideline is short of storage and takes it
//! from the pool furthest under its own, within each pool's minimum
//! and maximum size; `*BASE` gives first. A pool whose jobs waited for
//! an activity level gets more levels; one with many idle levels gives
//! some back, never below its defined level. `*MACHINE` is not changed.

/// One pool as the adjuster sees it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pool {
    /// `*BASE`, `*INTERACT`, `*SHRPOOL1`...
    pub name: String,
    /// Size, kilobytes.
    pub size_kb: u64,
    /// Activity level.
    pub actlvl: u32,
    /// The activity level the pool was defined with: levels given back
    /// stop there.
    pub defined_actlvl: u32,
    /// Faults per second in the last interval, database and other.
    pub faults: f64,
    /// Threads active in the pool.
    pub threads: u32,
    /// Jobs that went ineligible (wait to ineligible and active to
    /// ineligible) in the interval.
    pub ineligible: u64,
    /// Jobs holding an activity level at the end.
    pub active: u32,
    /// Tuning priority, 1 (first served) to 14.
    pub priority: u8,
    /// Minimum and maximum size, percent of main storage.
    pub min_pct: f64,
    /// Maximum size, percent.
    pub max_pct: f64,
    /// Fault guideline: minimum, per thread, maximum (per second).
    pub fault_min: f64,
    /// Per active thread.
    pub fault_thread: f64,
    /// Maximum.
    pub fault_max: f64,
}

impl Pool {
    /// Faults per second the pool may have at its activity.
    #[must_use]
    pub fn guideline(&self) -> f64 {
        (self.fault_min + self.fault_thread * f64::from(self.threads)).min(self.fault_max)
    }

    /// How far over its guideline (positive) or under it (negative).
    fn pressure(&self) -> f64 {
        self.faults - self.guideline()
    }
}

/// The tuning defaults of a shared pool, as IBM ships them.
#[must_use]
pub fn defaults(name: &str) -> (u8, f64, f64, f64, f64, f64) {
    // priority, min %, max %, fault min, per thread, max
    match name {
        "*MACHINE" => (1, 4.27, 100.0, 10.0, 0.0, 10.0),
        "*BASE" => (1, 4.99, 100.0, 12.0, 1.0, 200.0),
        "*INTERACT" => (1, 10.0, 100.0, 12.0, 1.0, 200.0),
        "*SPOOL" => (2, 1.0, 100.0, 5.0, 1.0, 100.0),
        _ => (2, 1.0, 100.0, 10.0, 2.0, 100.0),
    }
}

/// A change the adjuster makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The pool.
    pub name: String,
    /// New size, kilobytes.
    pub size_kb: u64,
    /// New activity level.
    pub actlvl: u32,
}

/// Storage moved in one step: a tenth of what the giver can spare, at
/// least 256 KB.
const STEP_MIN_KB: u64 = 256;

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)] // sizes far below 2^52
fn kb_of(pct: f64, main_kb: u64) -> u64 {
    (main_kb as f64 * pct / 100.0).max(0.0) as u64
}

/// The changes for one interval over `pools` in `main_kb` of storage.
/// Pools not changed are left out.
#[must_use]
pub fn adjust(pools: &[Pool], main_kb: u64) -> Vec<Change> {
    let mut next: Vec<(u64, u32)> = pools.iter().map(|p| (p.size_kb, p.actlvl)).collect();
    let adjustable = |p: &Pool| p.name != "*MACHINE" && p.size_kb > 0;
    // Storage: the neediest pool (by priority, then pressure) from the
    // pool with the most room under its guideline.
    let mut needy: Vec<usize> = (0..pools.len())
        .filter(|&i| adjustable(&pools[i]) && pools[i].pressure() > 0.0)
        .collect();
    needy.sort_by(|&a, &b| {
        pools[a]
            .priority
            .cmp(&pools[b].priority)
            .then(pools[b].pressure().total_cmp(&pools[a].pressure()))
    });
    for &n in &needy {
        let max = kb_of(pools[n].max_pct, main_kb);
        let giver = (0..pools.len())
            .filter(|&g| g != n && adjustable(&pools[g]) && pools[g].pressure() < 0.0)
            .filter(|&g| next[g].0 > kb_of(pools[g].min_pct, main_kb))
            // *BASE first, then the least pressed.
            .min_by(|&a, &b| {
                (pools[b].name == "*BASE")
                    .cmp(&(pools[a].name == "*BASE"))
                    .then(pools[a].pressure().total_cmp(&pools[b].pressure()))
            });
        let Some(g) = giver else { continue };
        let spare = next[g].0 - kb_of(pools[g].min_pct, main_kb);
        let room = max.saturating_sub(next[n].0);
        let step = (spare / 10).max(STEP_MIN_KB).min(spare).min(room);
        if step == 0 {
            continue;
        }
        next[g].0 -= step;
        next[n].0 += step;
    }
    // Activity levels.
    for (i, p) in pools.iter().enumerate() {
        if !adjustable(p) || p.actlvl == 0 {
            continue;
        }
        if p.ineligible > 0 {
            next[i].1 = p.actlvl + (p.actlvl / 10).max(1);
        } else if p.actlvl > 2 * p.active.max(5) && p.actlvl > p.defined_actlvl {
            // Idle levels cost nothing to keep but hide a pool's need;
            // give a tenth back, never below twice the active jobs.
            next[i].1 = (p.actlvl - (p.actlvl / 10).max(1))
                .max(2 * p.active.max(5))
                .max(p.defined_actlvl);
        }
    }
    pools
        .iter()
        .zip(next)
        .filter(|(p, (s, a))| *s != p.size_kb || *a != p.actlvl)
        .map(|(p, (size_kb, actlvl))| Change {
            name: p.name.clone(),
            size_kb,
            actlvl,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pool(name: &str, size_kb: u64, actlvl: u32, faults: f64) -> Pool {
        let (priority, min_pct, max_pct, fault_min, fault_thread, fault_max) = defaults(name);
        Pool {
            name: name.into(),
            size_kb,
            actlvl,
            faults,
            priority,
            min_pct,
            max_pct,
            fault_min,
            fault_thread,
            fault_max,
            ..Pool::default()
        }
    }

    #[test]
    fn a_faulting_pool_takes_from_base() {
        let main = 1_048_576;
        let pools = [
            pool("*MACHINE", 100_000, 0, 50.0),
            pool("*BASE", 600_000, 50, 0.0),
            pool("*INTERACT", 200_000, 40, 80.0),
            pool("*SPOOL", 50_000, 5, 0.0),
        ];
        let c = adjust(&pools, main);
        let base = c.iter().find(|c| c.name == "*BASE").unwrap();
        let int = c.iter().find(|c| c.name == "*INTERACT").unwrap();
        assert!(base.size_kb < 600_000);
        assert_eq!(
            600_000 - base.size_kb,
            int.size_kb - 200_000,
            "storage moves, none lost"
        );
        assert!(c.iter().all(|c| c.name != "*MACHINE"));
    }

    #[test]
    fn levels_follow_waits() {
        let mut p = pool("*BASE", 600_000, 20, 0.0);
        p.ineligible = 3;
        p.active = 20;
        assert_eq!(adjust(&[p.clone()], 1_048_576)[0].actlvl, 22);
        p.ineligible = 0;
        p.active = 1;
        p.actlvl = 100;
        p.defined_actlvl = 20;
        assert_eq!(adjust(&[p.clone()], 1_048_576)[0].actlvl, 90);
        p.actlvl = 21;
        assert_eq!(
            adjust(&[p.clone()], 1_048_576)[0].actlvl,
            20,
            "not below the defined level"
        );
        p.actlvl = 20;
        assert!(adjust(&[p], 1_048_576).is_empty(), "at the defined level");
    }
}
