// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the host families share: a partition's ID, its definition and
//! profiles, the resources it holds, its service channel, and the
//! bounds a receiver checks. The fields and their units are QS/VS's
//! (`part/model`, `part/resource`, `part/partd`); a value a host's own
//! rules refuse (a name not of its form, memory not a multiple of its
//! block) is the host's to refuse with its own messages. The bounds
//! here are the protocol's: what no receiver should have to hold.

use ipc::wire_struct;

use crate::base::Keylock;
use crate::open_enum;

/// A partition's ID on its host, 1 to [`MAX_ID`].
pub type PartitionId = u32;

/// The highest partition ID.
pub const MAX_ID: u32 = 999;
/// The longest name a console sends: a partition's, a profile's, a
/// pool's, a disk's, an image's, a switch's.
pub const MAX_NAME: usize = 64;
/// The longest text a console sends: a backing, a MAC address, a
/// location code.
pub const MAX_TEXT: usize = 256;
/// The most items of a list a console sends.
pub const MAX_ITEMS: usize = 256;

/// A partition ID in range.
pub(crate) fn check_id(id: PartitionId) -> Result<(), &'static str> {
    if id == 0 || id > MAX_ID {
        return Err("a partition ID outside 1 to 999");
    }
    Ok(())
}

/// A name no longer than [`MAX_NAME`].
pub(crate) fn check_name(name: &str) -> Result<(), &'static str> {
    if name.len() > MAX_NAME {
        return Err("a name longer than 64 bytes");
    }
    Ok(())
}

/// A text no longer than [`MAX_TEXT`].
pub(crate) fn check_text(text: &str) -> Result<(), &'static str> {
    if text.len() > MAX_TEXT {
        return Err("a text longer than 256 bytes");
    }
    Ok(())
}

/// A list no longer than [`MAX_ITEMS`].
pub(crate) fn check_items<T>(items: &[T]) -> Result<(), &'static str> {
    if items.len() > MAX_ITEMS {
        return Err("a list of more than 256 items");
    }
    Ok(())
}

open_enum! {
    /// A partition's state, by the names a console shows.
    PartitionState {
        /// No VMM; it holds nothing.
        NotActivated = 1,
        /// Its VMM runs and its system has not reported its IPL complete.
        Starting = 2,
        /// Its system reported its IPL complete, or it has no service
        /// channel and its kernel was entered.
        Running = 3,
        /// A shut down was asked for or the system is ending, and its
        /// VMM has not ended yet.
        ShuttingDown = 4,
        /// Held with its memory after a terminal code or a guest fault,
        /// or its VMM failed or was killed.
        Error = 5,
    }
}

open_enum! {
    /// What runs in a partition.
    Environment {
        /// The operating system a standalone system runs.
        Standalone = 1,
    }
}

open_enum! {
    /// Where an IPL comes from.
    IplSource {
        /// Side A.
        A = 1,
        /// Side B.
        B = 2,
        /// The side the partition itself asked for.
        Recorded = 3,
        /// Installation media in the first optical device.
        D = 4,
    }
}

open_enum! {
    /// Whether an IPL stops for an operator.
    IplType {
        /// It runs through.
        Unattended = 1,
        /// It stops at the IPL menu.
        Attended = 2,
    }
}

open_enum! {
    /// Where a partition's console is.
    ConsoleMode {
        /// The serial port.
        Serial = 1,
        /// The display, which a partition does not have: given the
        /// console port.
        Local = 2,
        /// The console port of the service channel.
        Partition = 3,
    }
}

open_enum! {
    /// How a partition gets its processors.
    ProcessorMode {
        /// Whole cores no other partition uses.
        Dedicated = 1,
        /// Virtual processors over a shared pool's cores.
        Shared = 2,
    }
}

open_enum! {
    /// Whether a shared partition may use more than its units.
    Sharing {
        /// Never more than its units.
        Capped = 1,
        /// Up to its virtual processors when the pool has spare time.
        Uncapped = 2,
    }
}

/// A message the managed side issued, where an event or a list carries
/// one (a refusal is the session's): its identifier and texts as it
/// gave them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Note {
    /// The message identifier (`QVS2108`).
    pub msgid: String,
    /// First-level text.
    pub text: String,
    /// Second-level text; empty when it has none.
    pub help: String,
}
wire_struct!(Note { msgid, text, help });

/// Whether a partition is activated when the host has started, and when
/// among the others.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoStart {
    /// Whether it is.
    pub enabled: bool,
    /// Its place: lower first; partitions with one place go by ID.
    pub order: u32,
    /// Seconds to wait before it, after the one before.
    pub delay_secs: u32,
    /// Whether the next waits until this one's IPL is complete or
    /// `wait_secs` have passed.
    pub wait: bool,
    /// The longest that wait, seconds.
    pub wait_secs: u32,
}
wire_struct!(AutoStart {
    enabled,
    order,
    delay_secs,
    wait,
    wait_secs
});

/// A partition's definition: what it is, whatever profile it is
/// activated with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Definition {
    /// Its ID, the host's to assign.
    pub id: PartitionId,
    /// Its name.
    pub name: String,
    /// What runs in it.
    pub environment: Environment,
    /// Its system serial number, the host's to make; never changed.
    pub serial: String,
    /// The keylock position.
    pub keylock: Keylock,
    /// The profile an activation uses when it names none; empty while
    /// it has none.
    pub default_profile: String,
    /// Auto start.
    pub auto_start: AutoStart,
    /// When it was created, seconds since the epoch.
    pub created: u64,
}
wire_struct!(Definition {
    id,
    name,
    environment,
    serial,
    keylock,
    default_profile,
    auto_start,
    created
});

impl Definition {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        check_id(self.id)?;
        check_name(&self.name)?;
        check_name(&self.serial)?;
        check_name(&self.default_profile)
    }
}

/// A profile's processors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Processors {
    /// Dedicated or shared.
    pub mode: ProcessorMode,
    /// The fewest processors (dedicated) or virtual processors (shared).
    pub minimum: u32,
    /// The number given when they are available.
    pub desired: u32,
    /// The most ever given.
    pub maximum: u32,
    /// Shared: the fewest processing units, hundredths of a core.
    pub minimum_units: Option<u32>,
    /// Shared: the units given when available, hundredths of a core.
    pub desired_units: Option<u32>,
    /// Shared: the most units, hundredths of a core.
    pub maximum_units: Option<u32>,
    /// Shared: capped or uncapped; capped when none.
    pub sharing: Option<Sharing>,
    /// Shared and uncapped: the weight, 0 to 255.
    pub weight: Option<u32>,
    /// Shared: the pool; the default pool when none.
    pub pool: Option<String>,
}
wire_struct!(Processors {
    mode,
    minimum,
    desired,
    maximum,
    minimum_units,
    desired_units,
    maximum_units,
    sharing,
    weight,
    pool
});

/// A profile's memory, MiB.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Memory {
    /// The least it is activated with.
    pub minimum_mib: u64,
    /// What it is given when available.
    pub desired_mib: u64,
    /// The most ever given.
    pub maximum_mib: u64,
}
wire_struct!(Memory {
    minimum_mib,
    desired_mib,
    maximum_mib
});

/// One unit of a virtual SCSI adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lun {
    /// The logical unit number.
    pub lun: u32,
    /// What it is kept in: a virtual disk, `POOL/NAME`.
    pub backing: String,
    /// The serial number the system sees; the host makes one when none.
    pub serial: Option<String>,
}
wire_struct!(Lun {
    lun,
    backing,
    serial
});

/// A virtual SCSI adapter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScsiAdapter {
    /// The virtual slot.
    pub slot: u32,
    /// Its units.
    pub luns: Vec<Lun>,
}
wire_struct!(ScsiAdapter { slot, luns });

/// How a partition IPLs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Boot {
    /// The IPL source.
    pub source: IplSource,
    /// The IPL type.
    pub ipl_type: IplType,
    /// The console mode.
    pub console: ConsoleMode,
}
wire_struct!(Boot {
    source,
    ipl_type,
    console
});

/// A virtual Ethernet adapter: a port of a virtual switch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EthernetAdapter {
    /// The virtual slot.
    pub slot: u32,
    /// The switch it is on.
    pub switch: Option<String>,
    /// The port VLAN, untagged to the system.
    pub vlan: u32,
    /// VLANs passed with their tags.
    pub tagged: Vec<u32>,
    /// The MAC address, `xx:xx:xx:xx:xx:xx`; the host makes one when none.
    pub mac: Option<String>,
    /// Whether the partition is activated only with its port.
    pub required: bool,
}
wire_struct!(EthernetAdapter {
    slot,
    switch,
    vlan,
    tagged,
    mac,
    required
});

/// A virtual optical or tape device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediaDevice {
    /// The virtual slot.
    pub slot: u32,
    /// The medium loaded at activation, by its name in the library.
    pub media: Option<String>,
}
wire_struct!(MediaDevice { slot, media });

/// A device of the host given to the partition (physical I/O, a later
/// version's to manage; carried so a profile reads whole).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhysicalIo {
    /// Its location code.
    pub location: String,
    /// Whether the partition cannot be activated without it.
    pub required: bool,
}
wire_struct!(PhysicalIo { location, required });

/// A profile: what an activation gives the partition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    /// Its name, one of the partition's.
    pub name: String,
    /// Processors.
    pub processors: Processors,
    /// Memory.
    pub memory: Memory,
    /// The IPL.
    pub boot: Boot,
    /// Virtual SCSI adapters.
    pub scsi: Vec<ScsiAdapter>,
    /// Virtual Ethernet adapters.
    pub ethernet: Vec<EthernetAdapter>,
    /// Virtual optical devices.
    pub optical: Vec<MediaDevice>,
    /// Virtual tape devices.
    pub tape: Vec<MediaDevice>,
    /// Physical I/O.
    pub physical_io: Vec<PhysicalIo>,
}
wire_struct!(Profile {
    name,
    processors,
    memory,
    boot,
    scsi,
    ethernet,
    optical,
    tape,
    physical_io
});

impl Profile {
    /// The protocol's bounds: names, texts and lists.
    pub fn check(&self) -> Result<(), &'static str> {
        check_name(&self.name)?;
        if let Some(p) = &self.processors.pool {
            check_name(p)?;
        }
        check_items(&self.scsi)?;
        for a in &self.scsi {
            check_items(&a.luns)?;
            for l in &a.luns {
                check_text(&l.backing)?;
                if let Some(s) = &l.serial {
                    check_name(s)?;
                }
            }
        }
        check_items(&self.ethernet)?;
        for e in &self.ethernet {
            check_items(&e.tagged)?;
            for t in [&e.switch, &e.mac].into_iter().flatten() {
                check_name(t)?;
            }
        }
        check_items(&self.optical)?;
        check_items(&self.tape)?;
        for m in self.optical.iter().chain(&self.tape) {
            if let Some(n) = &m.media {
                check_name(n)?;
            }
        }
        check_items(&self.physical_io)?;
        for p in &self.physical_io {
            check_text(&p.location)?;
        }
        Ok(())
    }
}

/// What an activated partition holds now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Current {
    /// Dedicated or shared.
    pub mode: ProcessorMode,
    /// Dedicated: its cores, by number.
    pub cores: Vec<u32>,
    /// Shared: its pool.
    pub pool: String,
    /// Its entitlement, hundredths of a core (a dedicated partition's
    /// is its cores').
    pub units: u32,
    /// The processors it sees: its cores, or its virtual processors.
    pub virtuals: u32,
    /// Shared: whether it is kept to its units.
    pub capped: bool,
    /// Shared: its uncapped weight; 0 when capped.
    pub weight: u32,
    /// Its memory, MiB.
    pub memory_mib: u64,
}
wire_struct!(Current {
    mode,
    cores,
    pool,
    units,
    virtuals,
    capped,
    weight,
    memory_mib
});

/// A partition's service channel, and what its system reported of
/// itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // four facts apart
pub struct Channel {
    /// The channel is up: the system said hello on it.
    pub up: bool,
    /// The system has none: its kernel was entered and it did not open
    /// the service port in time.
    pub none: bool,
    /// It answers pings.
    pub responding: bool,
    /// It is held with its memory.
    pub kept: bool,
    /// The system's name.
    pub system: String,
    /// Its release (`V1R2M0`); empty until known.
    pub release: String,
    /// Its fix level; empty until known.
    pub fix_level: String,
    /// Its addresses, each `address/prefix`.
    pub addresses: Vec<String>,
    /// The partition scope's version both sides speak; 0 with no channel.
    pub version: u16,
    /// The partition scope's capabilities both have.
    pub caps: u64,
}
wire_struct!(Channel {
    up,
    none,
    responding,
    kept,
    system,
    release,
    fix_level,
    addresses,
    version,
    caps
});

/// Who holds a virtual disk or a medium.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Holder {
    /// The partition; 0 for none.
    pub partition: PartitionId,
    /// The slot it is in.
    pub slot: u32,
}
wire_struct!(Holder { partition, slot });
