// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host family: a virtual server host's processors and memory
//! (installed, configurable, available), its shared processor pools,
//! the host reserve and its settings, and its hardware by location code
//! with what owns each item. Read only: the host's own settings are
//! changed at the host in version 1.
//!
//! Behind [`crate::caps::PHYSICAL_IO`]: the host's devices as physical
//! I/O sees them (released, assignable or why not, the isolation group,
//! who holds each), and their events. A device is released at the host;
//! a profile's `physical_io` gives it to a partition at activation.

use ipc::wire_struct;

use super::model::{Note, PartitionId};
use crate::message::Empty;
use crate::{Family, family, messages, open_enum};

/// Installed, configurable and available, of one resource.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Amount {
    /// What the machine has.
    pub installed: u64,
    /// What partitions can be given: installed less the host reserve.
    pub configurable: u64,
    /// What no running partition holds.
    pub available: u64,
}
wire_struct!(Amount {
    installed,
    configurable,
    available
});

/// A shared processor pool, as it is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcPool {
    /// Its name; the default pool is `DEFAULT`.
    pub name: String,
    /// Its cores, by number.
    pub cores: Vec<u32>,
    /// The most units its partitions use together, hundredths of a core.
    pub maximum: u32,
    /// The units its running partitions are entitled to.
    pub entitled: u32,
    /// What an activation can still be given of it.
    pub available: u32,
    /// Its running partitions.
    pub partitions: u32,
}
wire_struct!(ProcPool {
    name,
    cores,
    maximum,
    entitled,
    available,
    partitions
});

/// Ask for the host's resources.
pub type GetHost = Empty;

/// The host's resources, as they are now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Host {
    /// Sockets.
    pub sockets: u32,
    /// Cores.
    pub cores: u32,
    /// Threads of a core.
    pub threads_per_core: u32,
    /// Processors, hundredths of a core.
    pub processors: Amount,
    /// Memory, MiB.
    pub memory: Amount,
    /// The memory block size, MiB: a profile's memory is in multiples
    /// of it.
    pub memory_block_mib: u64,
    /// The host reserve's cores, by number.
    pub reserved_cores: Vec<u32>,
    /// The host reserve's memory, MiB: installed less configurable.
    pub reserved_memory_mib: u64,
    /// The shared processor pools, the default one first.
    pub pools: Vec<ProcPool>,
    /// Partitions defined.
    pub partitions: u32,
    /// The most partitions the host may have.
    pub max_partitions: u32,
}
wire_struct!(Host {
    sockets,
    cores,
    threads_per_core,
    processors,
    memory,
    memory_block_mib,
    reserved_cores,
    reserved_memory_mib,
    pools,
    partitions,
    max_partitions
});

/// Ask for the host's settings for its partitions.
pub type GetSettings = Empty;

/// The host's settings for its partitions, as it keeps them. The
/// reserve applies at the host's next IPL; [`Host`] has what is in
/// force.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Settings {
    /// The most partitions the host may have defined.
    pub max_partitions: u32,
    /// The memory block size, MiB.
    pub memory_block_mib: u64,
    /// Cores kept back for the host.
    pub reserved_cores: u32,
    /// Memory kept back for the host, MiB; the host's default when none.
    pub reserved_memory_mib: Option<u64>,
    /// The first three bytes of the MAC addresses the host makes,
    /// `xx:xx:xx`.
    pub mac_prefix: Option<String>,
    /// Seconds after a delayed shut down's delay before it is forced.
    pub delayed_grace_secs: u32,
    /// Seconds an operating-system shut down is given before it is
    /// forced.
    pub system_timeout_secs: u32,
    /// Seconds each partition is given to shut down through its system
    /// at the host's own shut down.
    pub host_timeout_secs: u32,
    /// Seconds after the kernel is entered for a system to open its
    /// service channel.
    pub channel_wait_secs: u32,
    /// The most dumps kept of one partition.
    pub max_dumps: u32,
    /// The most MiB one partition's dumps take together.
    pub max_dump_mib: u64,
}
wire_struct!(Settings {
    max_partitions,
    memory_block_mib,
    reserved_cores,
    reserved_memory_mib,
    mac_prefix,
    delayed_grace_secs,
    system_timeout_secs,
    host_timeout_secs,
    channel_wait_secs,
    max_dumps,
    max_dump_mib
});

open_enum! {
    /// What a hardware item is.
    ItemKind {
        /// A whole disk.
        Disk = 1,
        /// A network interface.
        Interface = 2,
        /// A PCI function.
        Pci = 3,
    }
}

open_enum! {
    /// What owns a hardware item.
    Owner {
        /// Nothing: free to be used.
        None = 1,
        /// The host itself: its load source, its management interface.
        Host = 2,
        /// A storage pool, a unit or hot spare of it ([`Item::owner_name`]).
        Pool = 3,
        /// A virtual switch, its uplink or a bond member of it.
        Switch = 4,
        /// A partition, given it as physical I/O.
        Partition = 5,
        /// Not this host's: another's label, a file system, a volume
        /// manager's label.
        Foreign = 6,
    }
}

/// One item of the host's hardware.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// Its location code.
    pub location: String,
    /// What it is.
    pub kind: ItemKind,
    /// Its kernel name (`nvme0n1`, `eth1`) or PCI address
    /// (`0000:00:1f.2`).
    pub name: String,
    /// Its model, driver or vendor and device, in the host's words.
    pub description: String,
    /// Its serial number; empty when it has none.
    pub serial: String,
    /// A disk's size, bytes; 0 otherwise.
    pub bytes: u64,
    /// An interface's MAC address; empty otherwise.
    pub mac: String,
    /// An interface's speed, Mb/s, when it has a link; 0 otherwise.
    pub speed_mbps: u32,
    /// An interface's carrier.
    pub carrier: bool,
    /// What owns it.
    pub owner: Owner,
    /// The owner's name: the pool's, the switch's, the partition's.
    pub owner_name: String,
    /// The owning partition's ID; 0 when it is not a partition's.
    pub partition: PartitionId,
    /// What it is, in the host's words (`POOL1 unit 2, Active`,
    /// `Uplink of PRODNET`, `Management`).
    pub what: String,
}
wire_struct!(Item {
    location,
    kind,
    name,
    description,
    serial,
    bytes,
    mac,
    speed_mbps,
    carrier,
    owner,
    owner_name,
    partition,
    what
});

/// Ask for the hardware inventory.
pub type GetHardware = Empty;

/// The host's hardware.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hardware {
    /// The manufacturer.
    pub manufacturer: String,
    /// The product name.
    pub product: String,
    /// The processor's model name.
    pub processor: String,
    /// The processors' virtualization (`VMX`, `SVM`).
    pub virtualization: String,
    /// Every item, by location code.
    pub items: Vec<Item>,
}
wire_struct!(Hardware {
    manufacturer,
    product,
    processor,
    virtualization,
    items
});

// Physical I/O ([`crate::caps::PHYSICAL_IO`]).

open_enum! {
    /// Why a device cannot be given to a partition now.
    Blocker {
        /// The host uses it: its load source, a pool's disk, a switch's
        /// uplink, its management interface.
        HostUses = 1,
        /// It is not released for partitions.
        NotReleased = 2,
        /// It shares its isolation group with a device not released.
        GroupNotReleased = 3,
        /// It cannot be reset, so it cannot be handed on clean.
        NoReset = 4,
        /// The host has no interrupt remapping.
        NoRemapping = 5,
    }
}

/// Why a device cannot be given now, with the host's message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blocked {
    /// The reason.
    pub reason: Blocker,
    /// The host's message for it.
    pub note: Note,
}
wire_struct!(Blocked { reason, note });

/// The partition that holds a device, and the profile it was activated
/// with.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceHolder {
    /// The partition.
    pub partition: PartitionId,
    /// Its name.
    pub name: String,
    /// The profile.
    pub profile: String,
}
wire_struct!(DeviceHolder {
    partition,
    name,
    profile
});

/// A PCI function of the host, as physical I/O sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    /// Its location code.
    pub location: String,
    /// Its PCI address (`0000:41:00.0`).
    pub address: String,
    /// Its class code: base class, subclass and interface, 24 bits.
    pub class: u32,
    /// Its vendor identifier.
    pub vendor: u16,
    /// Its device identifier.
    pub device: u16,
    /// What it is, in the host's words.
    pub description: String,
    /// The host has released it for partitions.
    pub released: bool,
    /// It can be given to a partition now.
    pub assignable: bool,
    /// Why not, when it cannot.
    pub blocked: Option<Blocked>,
    /// Its isolation group's number on the host.
    pub group: u32,
    /// The other devices of its group, by location code: they go with
    /// it.
    pub with: Vec<String>,
    /// The partition that holds it; none when free.
    pub holder: Option<DeviceHolder>,
}
wire_struct!(Device {
    location,
    address,
    class,
    vendor,
    device,
    description,
    released,
    assignable,
    blocked,
    group,
    with,
    holder
});

/// Ask for the host's devices as physical I/O sees them.
pub type GetDevices = Empty;

/// A device was released or taken back at the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceChanged {
    /// Its location code.
    pub location: String,
    /// It now; `None` when it is gone.
    pub device: Option<Device>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(DeviceChanged {
    location,
    device,
    time
});

/// A device was given to a partition at its activation, or given back
/// at its end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceHeld {
    /// Its location code.
    pub location: String,
    /// The partition and its profile.
    pub holder: DeviceHolder,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(DeviceHeld {
    location,
    holder,
    time
});

/// A device did not reset when it was given or given back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResetFailed {
    /// Its location code.
    pub location: String,
    /// The partition it was going to or coming from; 0 for none.
    pub partition: PartitionId,
    /// The host's message.
    pub note: Note,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(ResetFailed {
    location,
    partition,
    note,
    time
});

/// Operation numbers.
pub mod op {
    /// [`super::GetHost`].
    pub const HOST: u16 = 0x0001;
    /// [`super::GetSettings`].
    pub const SETTINGS: u16 = 0x0002;
    /// [`super::GetHardware`].
    pub const HARDWARE: u16 = 0x0003;
    /// [`super::GetDevices`] ([`crate::caps::PHYSICAL_IO`]).
    pub const DEVICES: u16 = 0x0010;

    /// Whether `op` is physical I/O's, which needs
    /// [`crate::caps::PHYSICAL_IO`].
    #[must_use]
    pub fn is_physical_io(op: u16) -> bool {
        op == DEVICES
    }
}

/// Event numbers.
pub mod event {
    /// The host's resources changed: a partition was activated or
    /// ended, a pool made, changed or deleted.
    pub const RESOURCES: u16 = 0x0001;
    /// A hardware item changed: its owner, its carrier, or it came or
    /// went.
    pub const ITEM: u16 = 0x0002;
    /// A device was released at the host ([`crate::caps::PHYSICAL_IO`]).
    pub const RELEASED: u16 = 0x0010;
    /// A device was taken back at the host ([`crate::caps::PHYSICAL_IO`]).
    pub const TAKEN_BACK: u16 = 0x0011;
    /// A device was given to a partition ([`crate::caps::PHYSICAL_IO`]).
    pub const ASSIGNED: u16 = 0x0012;
    /// A device was given back ([`crate::caps::PHYSICAL_IO`]).
    pub const RETURNED: u16 = 0x0013;
    /// A device did not reset ([`crate::caps::PHYSICAL_IO`]).
    pub const RESET_FAILED: u16 = 0x0014;

    /// Whether `op` is an event of physical I/O, which needs
    /// [`crate::caps::PHYSICAL_IO`].
    #[must_use]
    pub fn is_physical_io(op: u16) -> bool {
        (RELEASED..=RESET_FAILED).contains(&op)
    }
}

/// A hardware item changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemChanged {
    /// Its location code.
    pub location: String,
    /// It now; `None` when it is gone.
    pub item: Option<Item>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(ItemChanged {
    location,
    item,
    time
});

/// The family's own refusals (`QSI0301` to `QSI0329`).
pub mod msgid {
    /// The host's service that answers a request did not: its socket
    /// is not there or it did not answer in time.
    pub const SERVICE_DOWN: &str = "QSI0301";
}

/// Physical I/O's refusals (`QSI1301` to `QSI1349`), of a profile put,
/// an activation, or a load into a physical drive's slot. A host with
/// identifiers of its own gives them after these.
pub mod physio {
    /// No device of the host has that location code.
    pub const UNKNOWN_LOCATION: &str = "QSI1301";
    /// The device is not released for partitions.
    pub const NOT_RELEASED: &str = "QSI1302";
    /// Another partition holds it.
    pub const HELD: &str = "QSI1303";
    /// Its isolation group is not wholly given to this partition.
    pub const GROUP_SPLIT: &str = "QSI1304";
    /// It cannot be given now for another reason: the host uses it, it
    /// cannot be reset, the host has no interrupt remapping. The
    /// device's own message follows.
    pub const NOT_ASSIGNABLE: &str = "QSI1305";
    /// A tape device names a location that is not a tape drive.
    pub const NOT_A_DRIVE: &str = "QSI1306";
    /// A load or unload of a slot that is a physical drive: its
    /// cartridge is changed by hand.
    pub const NOT_VIRTUAL: &str = "QSI1307";
}

messages! {
    /// What a console asks.
    pub enum Request: family::HOST => {
        /// The host's resources.
        0x0001 => Host(GetHost),
        /// Its settings.
        0x0002 => Settings(GetSettings),
        /// Its hardware.
        0x0003 => Hardware(GetHardware),
        /// Its devices for physical I/O.
        0x0010 => Devices(GetDevices),
    }
}

messages! {
    /// What the host answers.
    pub enum Reply: family::HOST => {
        /// The host's resources.
        0x0001 => Host(Host),
        /// Its settings.
        0x0002 => Settings(Settings),
        /// Its hardware.
        0x0003 => Hardware(Hardware),
        /// Its devices, by location code.
        0x0010 => Devices(Vec<Device>),
    }
}

messages! {
    /// What the host pushes.
    pub enum Event: family::HOST => {
        /// The resources changed.
        0x0001 => Resources(Host),
        /// A hardware item changed.
        0x0002 => Item(ItemChanged),
        /// A device was released for partitions.
        0x0010 => Released(DeviceChanged),
        /// A device was taken back from partitions.
        0x0011 => TakenBack(DeviceChanged),
        /// A device was given to a partition at its activation.
        0x0012 => Assigned(DeviceHeld),
        /// A device was given back at a partition's end.
        0x0013 => Returned(DeviceHeld),
        /// A device did not reset.
        0x0014 => ResetFailed(ResetFailed),
    }
}

impl Request {
    /// The protocol's bounds: these requests have no fields.
    pub fn check(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

/// The host family.
#[derive(Debug)]
pub struct HostFamily;

impl Family for HostFamily {
    const ID: u16 = family::HOST;
    const VERSION: u16 = 1;
    const CAP: u64 = crate::caps::HOST;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
