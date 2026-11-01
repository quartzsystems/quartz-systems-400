// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Performance: `WRKSYSACT`'s jobs, `WRKSYSSTS`'s pools and
//! `WRKDSKSTS`'s I/O rates, each over the user's elapsed interval.

/// One job of `WRKSYSACT`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobActivity {
    /// `123501`.
    pub number: String,
    /// The user profile.
    pub user: String,
    /// The job name.
    pub name: String,
    /// `BCH`, `INT`, `PJ`, `SBS`, `SYS`.
    pub kind: String,
    /// CPU % in the interval, tenths.
    pub cpu: u32,
    /// Disk I/O in the interval.
    pub io: u64,
    /// Transactions in the interval.
    pub transactions: u64,
}

impl JobActivity {
    /// `number/user/name`.
    pub fn qualified(&self) -> String {
        format!("{}/{}/{}", self.number, self.user, self.name)
    }
}

/// A storage pool as `WRKSYSSTS` shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PoolActivity {
    /// The system pool number.
    pub number: u32,
    /// `*MACHINE`, `*BASE`, a subsystem's.
    pub name: String,
    /// Size, megabytes.
    pub size_mb: u64,
    /// Reserved, megabytes.
    pub reserved_mb: u64,
    /// The activity level; `None` for the machine pool.
    pub activity: Option<u32>,
    /// Database faults a second, tenths.
    pub db_faults: u32,
    /// Other faults a second, tenths.
    pub other_faults: u32,
    /// Active to wait transitions.
    pub act_wait: u64,
    /// Wait to ineligible.
    pub wait_inel: u64,
    /// Active to ineligible.
    pub act_inel: u64,
}

/// `WRKSYSACT` and `WRKSYSSTS` over the interval.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SysActivity {
    /// The elapsed interval, seconds.
    pub elapsed: u32,
    /// CPU % total, tenths.
    pub cpu: u32,
    /// CPU % for database work, tenths.
    pub cpu_db: u32,
    /// CPU % for interactive work, tenths.
    pub interactive: u32,
    /// CPU % the system's own overhead, tenths.
    pub overhead: u32,
    /// The busiest jobs, busiest first.
    pub jobs: Vec<JobActivity>,
    /// The pools.
    pub pools: Vec<PoolActivity>,
}

/// One disk unit's I/O over the interval.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnitIo {
    /// The unit number.
    pub unit: u32,
    /// `DD001`.
    pub resource: String,
    /// The ASP.
    pub asp: u32,
    /// % busy, tenths.
    pub busy: u32,
    /// Read requests a second, tenths.
    pub reads: u32,
    /// Write requests a second, tenths.
    pub writes: u32,
    /// KB read a second.
    pub read_kb: u64,
    /// KB written a second.
    pub write_kb: u64,
    /// Average service time, tenths of a millisecond.
    pub service: u32,
    /// Average wait time, tenths of a millisecond.
    pub wait: u32,
    /// Requests queued.
    pub queue: u32,
    /// The job doing the most I/O on it (`number/user/name`), blank if
    /// none.
    pub top_job: String,
}

/// `WRKDSKSTS`'s I/O rates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiskActivity {
    /// The elapsed interval, seconds.
    pub elapsed: u32,
    /// The configured units.
    pub units: Vec<UnitIo>,
}

/// What the performance pages need. Each call ends the user's interval
/// and starts the next, as the 5250 displays do on Refresh.
pub trait Perf: Send + Sync {
    /// `WRKSYSACT` and the pools of `WRKSYSSTS`.
    fn system(&self, user: &str) -> SysActivity;
    /// `WRKDSKSTS`'s I/O rates.
    fn disks(&self, user: &str) -> DiskActivity;
}
