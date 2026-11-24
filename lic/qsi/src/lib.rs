// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSI: the protocol a management console speaks to what it manages, a
//! virtual server host or a standalone system (`docs/QSI.md`). This
//! crate is its messages: the [`Hello`] each side says first, the
//! families of operations with their versions and capability bits
//! ([`family`], [`caps`]), and every family's requests, replies and
//! events ([`base`], [`pair`], [`service`], [`update`], [`host`]). The
//! session that carries
//! them is `qsisess`, the trust rules `qsitrust`. Its partition scope,
//! the channel between a partition and its hypervisor, is `qsipart`,
//! whose frames QSI's session keeps.
//!
//! Either side may be the older: each says its version and
//! capabilities in its hello, both use the lower version and only the
//! capabilities both have ([`Hello::agree`]). A family's later version
//! has a capability bit of its own. A request of an operation the
//! receiver does not know is refused with [`msgid::UNKNOWN`]; a body
//! with fields after the ones a receiver knows is read and the rest
//! passed over; a value a later version adds to a one-byte enumeration
//! is kept as `Other`.

#![forbid(unsafe_code)]

pub mod any;
pub mod base;
mod error;
mod hello;
pub mod host;
pub mod message;
pub mod msgid;
pub mod pair;
pub mod samples;
pub mod service;
pub mod update;

pub use error::Error;
pub use hello::{Agreed, Hello, PeerKind};
pub use message::{Empty, Family, Message};

/// The protocol's version: the highest this crate speaks.
pub const VERSION: u16 = 1;

/// The families' numbers.
pub mod family {
    /// What any managed system answers: identify, state, power, codes,
    /// events, time.
    pub const BASE: u16 = 0x0001;
    /// Pairing: the one thing a peer not yet pinned may do.
    pub const PAIR: u16 = 0x0002;
    /// A managed system's problems.
    pub const SERVICE: u16 = 0x0003;
    /// Code update of a managed system whose code is in two slots.
    pub const UPDATE: u16 = 0x0004;
    /// A virtual server host: processors, memory, pools, hardware.
    pub const HOST: u16 = 0x0010;
    /// The host's partitions, read.
    pub const PARTITIONS: u16 = 0x0011;
    /// One partition, changed: create, change, delete, activate, shut
    /// down, restart.
    pub const PARTITION: u16 = 0x0012;
    /// Partition profiles.
    pub const PROFILES: u16 = 0x0013;
    /// A partition's control panel.
    pub const PANEL: u16 = 0x0014;
    /// A partition's reference code history.
    pub const REFCODES: u16 = 0x0015;
    /// A partition's dumps.
    pub const DUMPS: u16 = 0x0016;
    /// A partition's console.
    pub const CONSOLE: u16 = 0x0017;
    /// Virtual resources: storage, disks, media, tape, switches,
    /// processor pools.
    pub const VRES: u16 = 0x0018;
}

/// Capability bits, as each side's hello says them: a family's version
/// 1 is its bit here; a later version, or operations added to a family
/// behind a bit, has a bit of its own.
pub mod caps {
    /// [`super::family::BASE`] version 1.
    pub const BASE: u64 = 1 << 0;
    /// [`super::family::PAIR`] version 1.
    pub const PAIR: u64 = 1 << 1;
    /// [`super::family::SERVICE`] version 1.
    pub const SERVICE: u64 = 1 << 2;
    /// [`super::family::UPDATE`] version 1.
    pub const UPDATE: u64 = 1 << 3;
    /// The system's own dumps: base's operations `0x0010` to `0x0013`
    /// and its events `0x0010` and `0x0011` ([`crate::base::TakeDump`]).
    pub const SYSTEM_DUMPS: u64 = 1 << 4;
    /// [`super::family::HOST`] version 1.
    pub const HOST: u64 = 1 << 8;
    /// [`super::family::PARTITIONS`] version 1.
    pub const PARTITIONS: u64 = 1 << 9;
    /// [`super::family::PARTITION`] version 1.
    pub const PARTITION: u64 = 1 << 10;
    /// [`super::family::PROFILES`] version 1.
    pub const PROFILES: u64 = 1 << 11;
    /// [`super::family::PANEL`] version 1.
    pub const PANEL: u64 = 1 << 12;
    /// [`super::family::REFCODES`] version 1.
    pub const REFCODES: u64 = 1 << 13;
    /// [`super::family::DUMPS`] version 1.
    pub const DUMPS: u64 = 1 << 14;
    /// [`super::family::CONSOLE`] version 1.
    pub const CONSOLE: u64 = 1 << 15;
    /// [`super::family::VRES`] version 1.
    pub const VRES: u64 = 1 << 16;
    /// Physical I/O: the host family's devices (operation `0x0010`,
    /// events `0x0010` to `0x0014`) and vres's physical tape drives
    /// (operation `0x0034`, event `0x000A`).
    pub const PHYSICAL_IO: u64 = 1 << 17;
    /// [`super::family::PARTITION`] version 2: a running partition's
    /// entitlement (operation `0x0007`, the family's events).
    pub const PARTITION_V2: u64 = 1 << 18;
    /// Switches managed from a console: vres's operations `0x0045` to
    /// `0x004A` (make, change, rename, delete, an uplink set and
    /// cleared).
    pub const SWITCHES: u64 = 1 << 19;
    /// Storage pools managed from a console: vres's operations `0x0002`
    /// to `0x000A` (the disks, make, delete, rename, a unit replaced,
    /// hot spares, the media repository, extend).
    pub const POOLS: u64 = 1 << 20;
    /// [`super::family::PARTITION`] version 3: a partition's settings
    /// (operations `0x0008` to `0x000A`).
    pub const PARTITION_V3: u64 = 1 << 21;
    /// Activation times: the partitions family's operation `0x0004` and
    /// the profiles family's `0x0007`.
    pub const ACTIVATIONS: u64 = 1 << 22;
    /// A host's power-on settings: the host family's operations
    /// `0x0020` and `0x0021` and its event `0x0020`.
    pub const POWER_ON: u64 = 1 << 23;
    /// [`super::family::PANEL`] version 2: the functions a system has
    /// (operation `0x0002`).
    pub const PANEL_FUNCTIONS: u64 = 1 << 24;
    /// [`super::family::SERVICE`] version 2: problem notes and severity
    /// (operations `0x0004` to `0x0006`, event `0x0005`).
    pub const PROBLEM_NOTES: u64 = 1 << 25;
    /// Dump space: base's operation `0x0014` (with
    /// [`SYSTEM_DUMPS`]) and the dumps family's `0x0005`.
    pub const DUMP_SPACE: u64 = 1 << 26;
    /// The cumulative fix level: base's operation `0x0008`.
    pub const FIX_LEVEL: u64 = 1 << 27;
    /// What the system has and uses (processors, main storage, the
    /// system ASP, jobs): base's operation `0x0009`.
    pub const RESOURCES: u64 = 1 << 28;

    /// What every managed system and console has.
    pub const MANAGED: u64 = BASE | PAIR;
    /// What a virtual server host answers, version 1.
    pub const HOST_V1: u64 = MANAGED
        | HOST
        | PARTITIONS
        | PARTITION
        | PROFILES
        | PANEL
        | REFCODES
        | DUMPS
        | CONSOLE
        | VRES;
    /// Problems, code update and the system's own dumps, which a
    /// managed system says beside the rest of what it answers.
    pub const SERVICING: u64 = SERVICE | UPDATE | SYSTEM_DUMPS;
    /// What a standalone system will say: base and pair, its problems
    /// with their notes, its own dumps and their space, its control
    /// panel and its functions, its console, its fix level and its
    /// resources. No code
    /// update (its LIC's slots are changed by PTFs) and no other host
    /// family.
    pub const STANDALONE: u64 = MANAGED
        | SERVICE
        | SYSTEM_DUMPS
        | PANEL
        | CONSOLE
        | PROBLEM_NOTES
        | DUMP_SPACE
        | PANEL_FUNCTIONS
        | FIX_LEVEL
        | RESOURCES;

    /// The capability of a family's version 1, by the family's number;
    /// 0 for a family this crate does not know.
    #[must_use]
    pub fn of(family: u16) -> u64 {
        use super::family as f;
        match family {
            f::BASE => BASE,
            f::PAIR => PAIR,
            f::SERVICE => SERVICE,
            f::UPDATE => UPDATE,
            f::HOST => HOST,
            f::PARTITIONS => PARTITIONS,
            f::PARTITION => PARTITION,
            f::PROFILES => PROFILES,
            f::PANEL => PANEL,
            f::REFCODES => REFCODES,
            f::DUMPS => DUMPS,
            f::CONSOLE => CONSOLE,
            f::VRES => VRES,
            _ => 0,
        }
    }

    /// Every capability a request of `family` and `op` needs: its
    /// family's, and the bit of an operation added behind one of its
    /// own. 0 for a family this crate does not know.
    #[must_use]
    pub fn of_request(family: u16, op: u16) -> u64 {
        use super::family as f;
        use crate::base::op as b;
        use crate::host::{dumps, host, panel, partition, partitions, profiles, vres};
        match family {
            f::BASE if b::is_dump(op) => BASE | SYSTEM_DUMPS,
            f::BASE if b::is_dump_space(op) => BASE | SYSTEM_DUMPS | DUMP_SPACE,
            f::BASE if b::is_fix_level(op) => BASE | FIX_LEVEL,
            f::BASE if b::is_resources(op) => BASE | RESOURCES,
            f::SERVICE if crate::service::op::is_v2(op) => SERVICE | PROBLEM_NOTES,
            f::HOST if host::op::is_physical_io(op) => HOST | PHYSICAL_IO,
            f::HOST if host::op::is_power_on(op) => HOST | POWER_ON,
            f::PARTITIONS if partitions::op::is_activations(op) => PARTITIONS | ACTIVATIONS,
            f::PARTITION if partition::op::is_v2(op) => PARTITION | PARTITION_V2,
            f::PARTITION if partition::op::is_v3(op) => PARTITION | PARTITION_V3,
            f::PROFILES if profiles::op::is_activations(op) => PROFILES | ACTIVATIONS,
            f::PANEL if panel::op::is_v2(op) => PANEL | PANEL_FUNCTIONS,
            f::DUMPS if dumps::op::is_space(op) => DUMPS | DUMP_SPACE,
            f::VRES if vres::op::is_physical_io(op) => VRES | PHYSICAL_IO,
            f::VRES if vres::op::is_switches(op) => VRES | SWITCHES,
            f::VRES if vres::op::is_pools(op) => VRES | POOLS,
            _ => of(family),
        }
    }

    /// Every capability an event of `family` and `op` needs, as
    /// [`of_request`]: a session sends an event only to a peer that
    /// agreed on all of it.
    #[must_use]
    pub fn of_event(family: u16, op: u16) -> u64 {
        use super::family as f;
        use crate::host::{host, partition, vres};
        match family {
            f::BASE if crate::base::event::is_dump(op) => BASE | SYSTEM_DUMPS,
            f::SERVICE if crate::service::event::is_v2(op) => SERVICE | PROBLEM_NOTES,
            f::HOST if host::event::is_physical_io(op) => HOST | PHYSICAL_IO,
            f::HOST if host::event::is_power_on(op) => HOST | POWER_ON,
            f::VRES if vres::event::is_physical_io(op) => VRES | PHYSICAL_IO,
            f::PARTITION if partition::event::is_v2(op) => PARTITION | PARTITION_V2,
            _ => of(family),
        }
    }
}
