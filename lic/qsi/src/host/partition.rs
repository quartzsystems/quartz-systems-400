// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The partition family: one partition, changed. Create, change and
//! delete it; activate it with a profile and overrides; shut it down
//! (delayed, operating system or immediate, each with or without a
//! restart); restart it. What follows an answer (the state, the codes,
//! the resources) comes as the partitions family's events.
//!
//! Version 2 ([`crate::caps::PARTITION_V2`]) changes a running shared
//! partition's entitlement: its units, capped or uncapped, its weight.
//!
//! Version 3 ([`crate::caps::PARTITION_V3`]) adds a partition's
//! settings beside its definition ([`Settings`]: a description, panel
//! functions from a console, how it is shut down with the managed
//! system), and a create that names its ID and settings. A definition
//! is in the middle of other bodies and cannot gain fields, so the
//! settings are a body of their own.

use ipc::wire_struct;

use super::model::{
    Current, Definition, IplSource, PartitionId, Sharing, check_id, check_name, check_text,
};
use crate::base::Keylock;
use crate::message::Empty;
use crate::{Family, family, messages, open_enum};

/// The longest delay of a delayed shut down, seconds.
pub const MAX_DELAY: u32 = 99_999;

/// Create a partition. Its ID and serial number are the host's to
/// assign; everything else has the host's defaults.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Create {
    /// Its name.
    pub name: String,
}
wire_struct!(Create { name });

/// Change a definition (its name, keylock, default profile, auto
/// start) while the partition is not activated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// The definition, its ID saying which.
    pub definition: Definition,
}
wire_struct!(Change { definition });

/// Delete a partition and its profiles while it is not activated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delete {
    /// The partition.
    pub id: PartitionId,
    /// Its virtual disks are deleted too, every one or none; kept
    /// otherwise.
    pub disks: bool,
}
wire_struct!(Delete { id, disks });

/// Activate a partition. The answer is what it was given; it is then
/// Starting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activate {
    /// The partition.
    pub id: PartitionId,
    /// The profile; empty for the default one.
    pub profile: String,
    /// An IPL source for this activation, in place of the profile's.
    pub source: Option<IplSource>,
    /// A keylock position for this activation, in place of the
    /// definition's.
    pub keylock: Option<Keylock>,
}
wire_struct!(Activate {
    id,
    profile,
    source,
    keylock
});

open_enum! {
    /// How a partition is shut down.
    ShutdownKind {
        /// The system's controlled power down with a delay; forced when
        /// the delay and the host's grace have passed.
        Delayed = 1,
        /// The system's immediate power down; forced after the host's
        /// timeout.
        System = 2,
        /// The VMM is stopped.
        Immediate = 3,
    }
}

/// Shut a partition down. A delayed or operating-system one is answered
/// when the system has accepted it; an immediate one when the VMM has
/// been asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShutDown {
    /// The partition.
    pub id: PartitionId,
    /// How.
    pub kind: ShutdownKind,
    /// A delayed one's delay, seconds, 0 to [`MAX_DELAY`].
    pub delay: u32,
    /// Activate it again afterwards.
    pub restart: bool,
}
wire_struct!(ShutDown {
    id,
    kind,
    delay,
    restart
});

/// Restart a partition at once: its VMM stopped and the partition
/// activated again with the profile it had.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Restart {
    /// The partition.
    pub id: PartitionId,
}
wire_struct!(Restart { id });

/// The highest uncapped weight.
pub const MAX_WEIGHT: u32 = 255;

/// Change a running shared-processor partition's entitlement
/// ([`crate::caps::PARTITION_V2`]): any of its processing units,
/// capped or uncapped, and uncapped weight; what is none stays.
/// Virtual processors, dedicated processors and memory change only at
/// activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entitlement {
    /// The partition.
    pub id: PartitionId,
    /// Its processing units, hundredths of a core.
    pub units: Option<u32>,
    /// Capped or uncapped.
    pub sharing: Option<Sharing>,
    /// Its uncapped weight, 0 to [`MAX_WEIGHT`].
    pub weight: Option<u32>,
}
wire_struct!(Entitlement {
    id,
    units,
    sharing,
    weight
});

/// A running partition's entitlement was changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntitlementChanged {
    /// The partition.
    pub id: PartitionId,
    /// Its name.
    pub name: String,
    /// What it held before.
    pub before: Current,
    /// What it holds now.
    pub current: Current,
    /// The console's user who asked.
    pub by: String,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(EntitlementChanged {
    id,
    name,
    before,
    current,
    by,
    time
});

open_enum! {
    /// How a partition is shut down when the managed system powers off
    /// (version 3).
    WithHost {
        /// The system's controlled power down with the settings' delay.
        Delayed = 1,
        /// At once.
        Immediate = 2,
    }
}

/// A partition's settings beside its definition (version 3). They may
/// be changed whether it runs or not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// What it is for, in a person's words; 256 bytes at most.
    pub description: String,
    /// Control panel functions are allowed from a console. A host
    /// refuses them with its own message when not; QSI does not
    /// interpret it.
    pub panel: bool,
    /// How it is shut down with the managed system.
    pub with_host: WithHost,
    /// A delayed shut down's delay, seconds, at most [`MAX_DELAY`].
    pub delay: u32,
}
wire_struct!(Settings {
    description,
    panel,
    with_host,
    delay
});

impl Settings {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        check_text(&self.description)?;
        if self.delay > MAX_DELAY {
            return Err("a delay over 99999 seconds");
        }
        Ok(())
    }
}

/// Create a partition with its settings and, when given, its ID
/// (version 3). Its serial number is the host's to assign.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateWith {
    /// Its ID; `None` for the host to give one.
    pub id: Option<PartitionId>,
    /// Its name.
    pub name: String,
    /// Its settings.
    pub settings: Settings,
}
wire_struct!(CreateWith { id, name, settings });

/// Ask for a partition's settings (version 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GetSettings {
    /// The partition.
    pub id: PartitionId,
}
wire_struct!(GetSettings { id });

/// Change a partition's settings, running or not (version 3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetSettings {
    /// The partition.
    pub id: PartitionId,
    /// Its settings.
    pub settings: Settings,
}
wire_struct!(SetSettings { id, settings });

/// Operation numbers.
pub mod op {
    /// [`super::Create`].
    pub const CREATE: u16 = 0x0001;
    /// [`super::Change`].
    pub const CHANGE: u16 = 0x0002;
    /// [`super::Delete`].
    pub const DELETE: u16 = 0x0003;
    /// [`super::Activate`].
    pub const ACTIVATE: u16 = 0x0004;
    /// [`super::ShutDown`].
    pub const SHUT_DOWN: u16 = 0x0005;
    /// [`super::Restart`].
    pub const RESTART: u16 = 0x0006;
    /// [`super::Entitlement`] ([`crate::caps::PARTITION_V2`]).
    pub const ENTITLEMENT: u16 = 0x0007;
    /// [`super::CreateWith`] ([`crate::caps::PARTITION_V3`]).
    pub const CREATE_WITH: u16 = 0x0008;
    /// [`super::GetSettings`] ([`crate::caps::PARTITION_V3`]).
    pub const GET_SETTINGS: u16 = 0x0009;
    /// [`super::SetSettings`] ([`crate::caps::PARTITION_V3`]).
    pub const SET_SETTINGS: u16 = 0x000A;

    /// Whether `op` is version 2's, which needs
    /// [`crate::caps::PARTITION_V2`].
    #[must_use]
    pub fn is_v2(op: u16) -> bool {
        op == ENTITLEMENT
    }

    /// Whether `op` is version 3's, which needs
    /// [`crate::caps::PARTITION_V3`] (and not version 2's bit).
    #[must_use]
    pub fn is_v3(op: u16) -> bool {
        (CREATE_WITH..=SET_SETTINGS).contains(&op)
    }
}

/// Event numbers: version 2's ([`crate::caps::PARTITION_V2`]).
pub mod event {
    /// [`super::EntitlementChanged`].
    pub const ENTITLEMENT: u16 = 0x0001;

    /// Whether `op` is version 2's: every event of the family is.
    #[must_use]
    pub fn is_v2(op: u16) -> bool {
        op == ENTITLEMENT
    }
}

/// The family's own refusals (`QSI0360` to `QSI0399`).
pub mod msgid {
    /// A value of an enumeration this host does not have (a shut down
    /// kind, an IPL source, a keylock position of a later version).
    pub const NO_SUCH_VALUE: &str = "QSI0361";
    /// A create with an ID another partition has (version 3).
    pub const ID_TAKEN: &str = "QSI0362";
    /// A create with an ID within 1 to 999 but beyond the host's most
    /// partitions (version 3).
    pub const ID_OUT_OF_RANGE: &str = "QSI0363";
}

/// Version 2's refusals of an entitlement (`QSI1401` to `QSI1449`). A
/// host with identifiers of its own gives them after these.
pub mod entitlement {
    /// The partition is not running.
    pub const NOT_RUNNING: &str = "QSI1401";
    /// It has dedicated processors.
    pub const DEDICATED: &str = "QSI1402";
    /// The units are outside its profile's minimum and maximum.
    pub const OUTSIDE_PROFILE: &str = "QSI1403";
    /// The units are below what its virtual processors need (0.05
    /// each), or above their worth (1.00 each).
    pub const VIRTUALS: &str = "QSI1404";
    /// Its pool has not that much left.
    pub const POOL_SHORT: &str = "QSI1405";
}

messages! {
    /// What a console asks.
    pub enum Request: family::PARTITION => {
        /// Create a partition.
        0x0001 => Create(Create),
        /// Change its definition.
        0x0002 => Change(Change),
        /// Delete it.
        0x0003 => Delete(Delete),
        /// Activate it.
        0x0004 => Activate(Activate),
        /// Shut it down.
        0x0005 => ShutDown(ShutDown),
        /// Restart it.
        0x0006 => Restart(Restart),
        /// Change a running partition's entitlement.
        0x0007 => Entitlement(Entitlement),
        /// Create a partition with its ID and settings.
        0x0008 => CreateWith(CreateWith),
        /// Its settings.
        0x0009 => GetSettings(GetSettings),
        /// Change them.
        0x000A => SetSettings(SetSettings),
    }
}

messages! {
    /// What the host answers.
    pub enum Reply: family::PARTITION => {
        /// The definition, with its ID and serial number.
        0x0001 => Create(Definition),
        /// Changed.
        0x0002 => Change(Empty),
        /// Deleted.
        0x0003 => Delete(Empty),
        /// What it was given.
        0x0004 => Activate(Current),
        /// Accepted.
        0x0005 => ShutDown(Empty),
        /// Asked.
        0x0006 => Restart(Empty),
        /// What it holds now.
        0x0007 => Entitlement(Current),
        /// The definition, with its ID and serial number.
        0x0008 => CreateWith(Definition),
        /// The settings.
        0x0009 => GetSettings(Settings),
        /// Changed.
        0x000A => SetSettings(Empty),
    }
}

messages! {
    /// Version 1 has no events: the partitions family's say what
    /// follows. Version 2 says an entitlement changed (the partitions
    /// family's `ResourcesChanged` follows too).
    pub enum Event: family::PARTITION => {
        /// A running partition's entitlement was changed.
        0x0001 => Entitlement(EntitlementChanged),
    }
}

impl Request {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::Create(c) => check_name(&c.name),
            Self::Change(c) => c.definition.check(),
            Self::Delete(Delete { id, .. })
            | Self::Restart(Restart { id })
            | Self::GetSettings(GetSettings { id }) => check_id(*id),
            Self::CreateWith(c) => {
                if let Some(id) = c.id {
                    check_id(id)?;
                }
                check_name(&c.name)?;
                c.settings.check()
            }
            Self::SetSettings(s) => {
                check_id(s.id)?;
                s.settings.check()
            }
            Self::Activate(a) => {
                check_id(a.id)?;
                check_name(&a.profile)
            }
            Self::ShutDown(s) => {
                check_id(s.id)?;
                if s.delay > MAX_DELAY {
                    return Err("a delay over 99999 seconds");
                }
                Ok(())
            }
            Self::Entitlement(e) => {
                check_id(e.id)?;
                if e.units.is_none() && e.sharing.is_none() && e.weight.is_none() {
                    return Err("an entitlement that changes nothing");
                }
                if e.weight.is_some_and(|w| w > MAX_WEIGHT) {
                    return Err("a weight over 255");
                }
                Ok(())
            }
        }
    }
}

/// The partition family. Version 2 ([`crate::caps::PARTITION_V2`]) adds
/// a running partition's entitlement and its event, version 3
/// ([`crate::caps::PARTITION_V3`]) a partition's settings; version 1 is
/// [`crate::caps::PARTITION`]. Each later version's bit alone gates its
/// operations.
#[derive(Debug)]
pub struct Partition;

impl Family for Partition {
    const ID: u16 = family::PARTITION;
    const VERSION: u16 = 3;
    const CAP: u64 = crate::caps::PARTITION;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
