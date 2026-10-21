// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Active subsystems and storage pools as the storage service keeps
//! them, in the shapes they travel in.

use ipc::wire_struct;

/// An active subsystem.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SbsInfo {
    /// Name.
    pub name: String,
    /// Library of its description.
    pub lib: String,
    /// The monitor job's number.
    pub monitor: u32,
    /// `ACTIVE` or `ENDING`.
    pub status: String,
    /// Active jobs in it, the monitor not counted.
    pub jobs: u32,
    /// `MAXJOBS`; 0 is `*NOMAX`.
    pub maxjobs: u32,
    /// The storage pools it uses, by subsystem pool number.
    pub pools: Vec<(u8, String)>,
}

wire_struct!(SbsInfo {
    name,
    lib,
    monitor,
    status,
    jobs,
    maxjobs,
    pools
});

/// What a monitor learns each time it asks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SbsState {
    /// The subsystem is to end: `*IMMED` and the delay in seconds
    /// before a controlled end becomes immediate.
    pub ending: Option<(bool, u32)>,
    /// Its jobs `ENDJOB` asked to end: number, and `*IMMED`.
    pub end_jobs: Vec<(u32, bool)>,
}

wire_struct!(SbsState { ending, end_jobs });

/// A storage pool.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PoolInfo {
    /// `*MACHINE`, `*BASE`, `*INTERACT`, `*SPOOL`, `*SHRPOOLn`, or
    /// `SBS/n` for a subsystem's private pool.
    pub name: String,
    /// The system pool number, 1 for `*MACHINE`, 2 for `*BASE`.
    pub number: u16,
    /// Size in kilobytes; 0 is not defined.
    pub size_kb: u64,
    /// Kilobytes reserved for the system.
    pub reserved_kb: u64,
    /// Maximum active jobs.
    pub actlvl: u32,
    /// Jobs holding an activity level now.
    pub active: u32,
    /// Jobs waiting for one (`INEL`).
    pub ineligible: u32,
    /// Transitions active to waiting since the counters were reset.
    pub act_wait: u64,
    /// Waiting to ineligible.
    pub wait_inel: u64,
    /// Active to ineligible.
    pub act_inel: u64,
}

wire_struct!(PoolInfo {
    name,
    number,
    size_kb,
    reserved_kb,
    actlvl,
    active,
    ineligible,
    act_wait,
    wait_inel,
    act_inel
});
