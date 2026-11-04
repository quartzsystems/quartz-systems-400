// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One interval's data as the collector records it: what happened
//! between two samples of the machine, per category. Counts are for the
//! interval; sizes and states are at its end.

use ipc::wire_struct;

/// The machine (`*SYSLVL`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SysData {
    /// CPU used in the interval, milliseconds over every processor.
    pub cpu_used_ms: u64,
    /// CPU available in the interval: processors times the interval.
    pub cpu_avail_ms: u64,
    /// Processors.
    pub cpus: u32,
    /// Main storage, kilobytes.
    pub main_kb: u64,
    /// The system ASP's capacity and what is free, bytes.
    pub asp_bytes: u64,
    /// Free bytes of the system ASP.
    pub asp_free: u64,
    /// Jobs in the system.
    pub jobs: u32,
    /// Active threads.
    pub threads: u32,
    /// Full opens of database files.
    pub opens: u64,
    /// Database reads.
    pub db_reads: u64,
    /// Database writes.
    pub db_writes: u64,
    /// Interactive transactions.
    pub transactions: u64,
    /// Their response time, milliseconds.
    pub response_ms: u64,
}

wire_struct!(SysData {
    cpu_used_ms,
    cpu_avail_ms,
    cpus,
    main_kb,
    asp_bytes,
    asp_free,
    jobs,
    threads,
    opens,
    db_reads,
    db_writes,
    transactions,
    response_ms
});

/// One job (`*JOBMI`, `*JOBOS`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JobData {
    /// Name.
    pub name: String,
    /// User.
    pub user: String,
    /// Number.
    pub number: u32,
    /// `WRKACTJOB`'s type letter: `B`, `I`, `M`, `S`, `W`, `R`, `A`.
    pub kind: String,
    /// Subtype letter, blank for none.
    pub sub_type: String,
    /// Subsystem and its library; blank outside one.
    pub subsystem: String,
    /// The subsystem's library.
    pub sbs_lib: String,
    /// System pool number.
    pub pool: u16,
    /// Run priority.
    pub priority: u8,
    /// CPU used in the interval, milliseconds.
    pub cpu_ms: u64,
    /// CPU used since the job started, milliseconds.
    pub cpu_total_ms: u64,
    /// Threads.
    pub threads: u32,
    /// Temporary storage, megabytes.
    pub tmp_mb: u64,
    /// Database reads.
    pub db_reads: u64,
    /// Database writes.
    pub db_writes: u64,
    /// Other storage requests.
    pub other_io: u64,
    /// Full opens.
    pub opens: u64,
    /// Interactive transactions.
    pub transactions: u64,
    /// Their response time, milliseconds.
    pub response_ms: u64,
    /// Activity level transitions: active to wait, wait to ineligible,
    /// active to ineligible.
    pub aw: u64,
    /// Wait to ineligible.
    pub wi: u64,
    /// Active to ineligible.
    pub ai: u64,
}

wire_struct!(JobData {
    name,
    user,
    number,
    kind,
    sub_type,
    subsystem,
    sbs_lib,
    pool,
    priority,
    cpu_ms,
    cpu_total_ms,
    threads,
    tmp_mb,
    db_reads,
    db_writes,
    other_io,
    opens,
    transactions,
    response_ms,
    aw,
    wi,
    ai
});

impl JobData {
    /// Every storage request of the interval.
    #[must_use]
    pub fn io(&self) -> u64 {
        self.db_reads + self.db_writes + self.other_io
    }
}

/// One storage pool (`*POOL`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PoolData {
    /// System pool number.
    pub number: u16,
    /// Name: `*MACHINE`, `*BASE`, a shared pool or `SBS/n`.
    pub name: String,
    /// Size, kilobytes.
    pub size_kb: u64,
    /// Reserved size, kilobytes.
    pub reserved_kb: u64,
    /// Activity level, 0 for none (`+++++`).
    pub actlvl: u32,
    /// Database faults and pages read in the interval.
    pub db_faults: u64,
    /// Database pages.
    pub db_pages: u64,
    /// Other faults.
    pub ndb_faults: u64,
    /// Other pages.
    pub ndb_pages: u64,
    /// Active to wait transitions.
    pub aw: u64,
    /// Wait to ineligible.
    pub wi: u64,
    /// Active to ineligible.
    pub ai: u64,
    /// Jobs active in the pool at the end.
    pub active: u32,
    /// Jobs waiting for an activity level at the end.
    pub ineligible: u32,
}

wire_struct!(PoolData {
    number,
    name,
    size_kb,
    reserved_kb,
    actlvl,
    db_faults,
    db_pages,
    ndb_faults,
    ndb_pages,
    aw,
    wi,
    ai,
    active,
    ineligible
});

/// One disk unit (`*DISK`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiskData {
    /// Unit number, from 1.
    pub unit: u32,
    /// Type, `6B22`.
    pub type_code: String,
    /// Model.
    pub model: String,
    /// Resource name, `DD001`.
    pub resource: String,
    /// Serial number.
    pub serial: String,
    /// ASP.
    pub asp: u16,
    /// Capacity, bytes.
    pub capacity: u64,
    /// Free, bytes.
    pub available: u64,
    /// Read requests in the interval.
    pub reads: u64,
    /// Write requests.
    pub writes: u64,
    /// 512-byte blocks read.
    pub read_blocks: u64,
    /// 512-byte blocks written.
    pub write_blocks: u64,
    /// Milliseconds busy.
    pub busy_ms: u64,
}

wire_struct!(DiskData {
    unit,
    type_code,
    model,
    resource,
    serial,
    asp,
    capacity,
    available,
    reads,
    writes,
    read_blocks,
    write_blocks,
    busy_ms
});

/// One Ethernet line (`*CMNBASE`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineData {
    /// Line description.
    pub line: String,
    /// Resource name, `CMN01`.
    pub resource: String,
    /// Speed, bits per second.
    pub speed: u64,
    /// Bytes received in the interval.
    pub rx_bytes: u64,
    /// Bytes sent.
    pub tx_bytes: u64,
    /// Frames received.
    pub rx_frames: u64,
    /// Frames sent.
    pub tx_frames: u64,
    /// Frames received in error.
    pub rx_errors: u64,
    /// Frames not sent for errors.
    pub tx_errors: u64,
}

wire_struct!(LineData {
    line,
    resource,
    speed,
    rx_bytes,
    tx_bytes,
    rx_frames,
    tx_frames,
    rx_errors,
    tx_errors
});

/// One interval.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Interval {
    /// From 1.
    pub number: u32,
    /// When it ended, Unix seconds, local time.
    pub end: u64,
    /// Its length, seconds.
    pub secs: u32,
    /// `*SYSLVL`.
    pub system: SysData,
    /// `*JOBMI` and `*JOBOS`.
    pub jobs: Vec<JobData>,
    /// `*POOL`.
    pub pools: Vec<PoolData>,
    /// `*DISK`.
    pub disks: Vec<DiskData>,
    /// `*CMNBASE`.
    pub lines: Vec<LineData>,
}

wire_struct!(Interval {
    number,
    end,
    secs,
    system,
    jobs,
    pools,
    disks,
    lines
});

impl Interval {
    /// The interval with only the categories of `mask` kept.
    #[must_use]
    pub fn keep(mut self, mask: u32) -> Self {
        use crate::profile::Category as C;
        let has = |c: C| mask & c.bit() != 0;
        if !has(C::SysLvl) {
            self.system = SysData::default();
        }
        if !has(C::JobMi) && !has(C::JobOs) {
            self.jobs.clear();
        }
        if !has(C::Pool) {
            self.pools.clear();
        }
        if !has(C::Disk) {
            self.disks.clear();
        }
        if !has(C::CmnBase) {
            self.lines.clear();
        }
        self
    }
}
