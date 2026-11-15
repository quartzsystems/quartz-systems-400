// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The virtual resources family: a host's storage pools and their units
//! (read: pools are made at the host), virtual disks, the media library
//! (optical images and tape volumes, an upload stream, load and unload
//! into a partition's optical and tape devices), virtual switches with
//! their ports, counters and forwarding tables (read: switches are made
//! at the host), and shared processor pools.
//!
//! Operation numbers go by area: storage pools from `0x0001`, virtual
//! disks from `0x0010`, the media library and a partition's drives from
//! `0x0020`, tape volumes from `0x0030`, switches from `0x0040`, shared
//! processor pools from `0x0050`.

use ipc::wire_struct;

use super::model::{Holder, Note, PartitionId, check_id, check_items, check_name};
use crate::message::Empty;
use crate::{Family, family, messages, open_enum};

// Storage pools.

/// A unit of a storage pool.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PoolUnit {
    /// Its number, from 1; hot spares follow the members.
    pub number: u32,
    /// Its disk's kernel name; empty when it is not there.
    pub disk: String,
    /// Its disk's serial number.
    pub serial: String,
    /// Its disk's location code.
    pub location: String,
    /// Its disk's size, bytes.
    pub bytes: u64,
    /// Its state, in the host's words (`Active`, `Rebuilding`,
    /// `Failed`, `Missing`, `Hot spare`).
    pub state: String,
}
wire_struct!(PoolUnit {
    number,
    disk,
    serial,
    location,
    bytes,
    state
});

/// A storage pool.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoragePool {
    /// Its name.
    pub name: String,
    /// Its identity.
    pub uuid: String,
    /// Its protection, in the host's words (`none`, `mirrored`,
    /// `raid5`, `raid6`).
    pub protection: String,
    /// Its state, in the host's words (`Active`, `Degraded`,
    /// `Rebuilding`, `Resynchronizing`, `Failed`, `Assembling`).
    pub state: String,
    /// What it is doing: empty, `rebuilding`, `resynchronizing`.
    pub work: String,
    /// The unit worked on, from 1; 0 for the whole pool.
    pub work_unit: u32,
    /// How far, percent.
    pub percent: u32,
    /// Its size, bytes: what its file system holds.
    pub bytes: u64,
    /// Bytes allocated in it.
    pub allocated: u64,
    /// Bytes its virtual disks and media are, thin or thick.
    pub provisioned: u64,
    /// Whether it holds the media library.
    pub repository: bool,
    /// Its units.
    pub units: Vec<PoolUnit>,
    /// Why it failed or was not assembled; empty when it did not.
    pub failure: String,
}
wire_struct!(StoragePool {
    name,
    uuid,
    protection,
    state,
    work,
    work_unit,
    percent,
    bytes,
    allocated,
    provisioned,
    repository,
    units,
    failure
});

/// Ask for every storage pool.
pub type Pools = Empty;

// Virtual disks.

/// A virtual disk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vdisk {
    /// Its pool.
    pub pool: String,
    /// Its name.
    pub name: String,
    /// Its size, bytes.
    pub bytes: u64,
    /// Bytes the pool has given it.
    pub allocated: u64,
    /// Thick: allocated whole when made.
    pub thick: bool,
    /// Who holds it.
    pub holder: Holder,
}
wire_struct!(Vdisk {
    pool,
    name,
    bytes,
    allocated,
    thick,
    holder
});

/// Ask for the virtual disks of a pool.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Vdisks {
    /// The pool; empty for every pool.
    pub pool: String,
}
wire_struct!(Vdisks { pool });

/// Make a virtual disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VdiskCreate {
    /// Its pool.
    pub pool: String,
    /// Its name.
    pub name: String,
    /// Its size, MiB.
    pub mib: u64,
    /// Thick, else thin.
    pub thick: bool,
}
wire_struct!(VdiskCreate {
    pool,
    name,
    mib,
    thick
});

/// Grow a virtual disk no running partition holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VdiskResize {
    /// Its pool.
    pub pool: String,
    /// Its name.
    pub name: String,
    /// Its new size, MiB.
    pub mib: u64,
}
wire_struct!(VdiskResize { pool, name, mib });

/// Delete a virtual disk no partition holds. One a profile names is
/// deleted only when `force`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VdiskDelete {
    /// Its pool.
    pub pool: String,
    /// Its name.
    pub name: String,
    /// Even when a profile names it.
    pub force: bool,
}
wire_struct!(VdiskDelete { pool, name, force });

/// Ask which profiles name a virtual disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VdiskNamed {
    /// Its pool.
    pub pool: String,
    /// Its name.
    pub name: String,
}
wire_struct!(VdiskNamed { pool, name });

/// A profile that names a virtual disk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Naming {
    /// The partition.
    pub partition: PartitionId,
    /// The partition's name.
    pub name: String,
    /// The profile.
    pub profile: String,
    /// The SCSI adapter's slot.
    pub slot: u32,
    /// The logical unit.
    pub lun: u32,
}
wire_struct!(Naming {
    partition,
    name,
    profile,
    slot,
    lun
});

// The media library.

/// An optical image of the media library.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Image {
    /// Its name.
    pub name: String,
    /// Its size, bytes.
    pub bytes: u64,
    /// Its volume identifier.
    pub label: String,
    /// Who has it loaded (one of them: an image may be in several).
    pub holder: Holder,
}
wire_struct!(Image {
    name,
    bytes,
    label,
    holder
});

/// Ask for the optical images.
pub type Images = Empty;

/// Rename an image no partition has loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageRename {
    /// Its name.
    pub name: String,
    /// Its new name.
    pub to: String,
}
wire_struct!(ImageRename { name, to });

/// Delete an image no partition has loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageDelete {
    /// Its name.
    pub name: String,
}
wire_struct!(ImageDelete { name });

/// Upload an optical image: the reply opens a stream, console to host,
/// that carries exactly `bytes` bytes. When they are checked and named,
/// [`UploadEnded`] says how it went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageUpload {
    /// Its name.
    pub name: String,
    /// Its size, bytes.
    pub bytes: u64,
}
wire_struct!(ImageUpload { name, bytes });

/// An upload's stream is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uploading {
    /// The image's name.
    pub name: String,
    /// The bytes the stream is to carry.
    pub bytes: u64,
}
wire_struct!(Uploading { name, bytes });

open_enum! {
    /// What a partition's media device is.
    DriveKind {
        /// An optical device.
        Optical = 1,
        /// A tape drive.
        Tape = 2,
    }
}

/// Ask for a running partition's optical and tape devices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drives {
    /// The partition.
    pub id: PartitionId,
}
wire_struct!(Drives { id });

/// An optical or tape device of a running partition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drive {
    /// Its slot.
    pub slot: u32,
    /// What it is.
    pub kind: DriveKind,
    /// The medium in it: an image or a volume; empty for none.
    pub name: String,
    /// Whether the partition prevents its removal.
    pub prevented: bool,
}
wire_struct!(Drive {
    slot,
    kind,
    name,
    prevented
});

/// Load an image into a running partition's optical device, or mount a
/// tape volume in its tape drive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Load {
    /// The partition.
    pub id: PartitionId,
    /// The device's slot.
    pub slot: u32,
    /// The image or volume.
    pub name: String,
}
wire_struct!(Load { id, slot, name });

/// Unload a device's medium (an image, or a tape volume: unmount). When
/// the partition prevents its removal, only `forced`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Unload {
    /// The partition.
    pub id: PartitionId,
    /// The device's slot.
    pub slot: u32,
    /// Even when the partition prevents it.
    pub forced: bool,
}
wire_struct!(Unload { id, slot, forced });

/// The medium unloaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unloaded {
    /// Its name.
    pub name: String,
}
wire_struct!(Unloaded { name });

// Tape volumes.

/// A tape volume of the media library.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Volume {
    /// Its name.
    pub name: String,
    /// Its volume identifier.
    pub identifier: String,
    /// Its capacity, MiB.
    pub capacity_mib: u64,
    /// Bytes of its image in use.
    pub used: u64,
    /// Whether it is write-protected.
    pub protected: bool,
    /// Who has it mounted.
    pub holder: Holder,
}
wire_struct!(Volume {
    name,
    identifier,
    capacity_mib,
    used,
    protected,
    holder
});

/// Ask for the tape volumes.
pub type Volumes = Empty;

/// Make a tape volume.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeCreate {
    /// Its name.
    pub name: String,
    /// Its volume identifier.
    pub identifier: String,
    /// Its capacity, MiB.
    pub mib: u64,
}
wire_struct!(VolumeCreate {
    name,
    identifier,
    mib
});

/// Delete a tape volume no partition has mounted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeDelete {
    /// Its name.
    pub name: String,
}
wire_struct!(VolumeDelete { name });

/// Set or clear a tape volume's write protection, while no partition
/// has it mounted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeProtect {
    /// Its name.
    pub name: String,
    /// Protected.
    pub on: bool,
}
wire_struct!(VolumeProtect { name, on });

// Virtual switches.

open_enum! {
    /// How an uplink's interfaces are bonded.
    Bond {
        /// One carries the traffic; another takes over when it fails.
        ActiveBackup = 1,
        /// LACP with the physical switch.
        Lacp = 2,
    }
}

/// A switch's uplink.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Uplink {
    /// Its interfaces, by location code.
    pub interfaces: Vec<String>,
    /// How several are bonded; one has none.
    pub bond: Option<Bond>,
    /// The VLAN it carries untagged.
    pub native_vlan: Option<u16>,
}
wire_struct!(Uplink {
    interfaces,
    bond,
    native_vlan
});

/// One interface of an uplink, as it is now.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Member {
    /// Its location code.
    pub location: String,
    /// Its kernel name; empty when it is not on the host.
    pub name: String,
    /// Whether it has carrier.
    pub carrier: bool,
    /// Its speed, Mb/s, when it has a link and its driver says.
    pub speed_mbps: u32,
}
wire_struct!(Member {
    location,
    name,
    carrier,
    speed_mbps
});

/// A virtual switch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Switch {
    /// Its name.
    pub name: String,
    /// Its number, given when made and never changed.
    pub id: u32,
    /// The VLANs it carries.
    pub vlans: Vec<u16>,
    /// Its MTU.
    pub mtu: u32,
    /// Its uplink; an internal switch has none.
    pub uplink: Option<Uplink>,
    /// The uplink's state, in the host's words (`None`, `Up`,
    /// `Degraded`, `Down`, `Missing`).
    pub uplink_state: String,
    /// The uplink's interfaces.
    pub members: Vec<Member>,
    /// The partitions' ports on it.
    pub ports: u32,
    /// Why it was not built; empty when it was.
    pub failure: String,
}
wire_struct!(Switch {
    name,
    id,
    vlans,
    mtu,
    uplink,
    uplink_state,
    members,
    ports,
    failure
});

/// Ask for every switch.
pub type Switches = Empty;

/// A port's counters, from the partition's side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Bytes the partition sent.
    pub sent_bytes: u64,
    /// Frames the partition sent.
    pub sent_packets: u64,
    /// Frames the partition sent that were dropped.
    pub sent_dropped: u64,
    /// Bytes sent to the partition.
    pub received_bytes: u64,
    /// Frames sent to the partition.
    pub received_packets: u64,
    /// Frames for the partition that were dropped.
    pub received_dropped: u64,
}
wire_struct!(Counters {
    sent_bytes,
    sent_packets,
    sent_dropped,
    received_bytes,
    received_packets,
    received_dropped
});

/// A partition's port on a switch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Port {
    /// The partition.
    pub partition: PartitionId,
    /// The adapter's slot.
    pub slot: u32,
    /// The switch.
    pub switch: String,
    /// The port VLAN.
    pub vlan: u16,
    /// The tagged VLANs.
    pub tagged: Vec<u16>,
    /// The adapter's MAC address.
    pub mac: String,
    /// Whether it has link: a VMM has it open.
    pub link: bool,
    /// Its counters.
    pub counters: Counters,
}
wire_struct!(Port {
    partition,
    slot,
    switch,
    vlan,
    tagged,
    mac,
    link,
    counters
});

/// Ask for the partitions' ports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ports {
    /// The switch; empty for every one.
    pub switch: String,
}
wire_struct!(Ports { switch });

/// Ask for a switch's forwarding table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fdb {
    /// The switch.
    pub switch: String,
}
wire_struct!(Fdb { switch });

/// An entry of a forwarding table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FdbRow {
    /// The address.
    pub mac: String,
    /// Its VLAN; 0 for none.
    pub vlan: u16,
    /// Where it is: a partition's port, `uplink`, or the switch.
    pub port: String,
    /// The partition, for a partition's port.
    pub partition: PartitionId,
    /// The slot, for a partition's port.
    pub slot: u32,
    /// How it is known: `static`, `learned`, `local`.
    pub origin: String,
}
wire_struct!(FdbRow {
    mac,
    vlan,
    port,
    partition,
    slot,
    origin
});

/// Ask who is on which VLAN of every switch.
pub type Vlans = Empty;

/// A partition's port on a VLAN.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VlanPort {
    /// The partition.
    pub partition: PartitionId,
    /// The adapter's slot.
    pub slot: u32,
    /// Whether the VLAN is tagged to it.
    pub tagged: bool,
}
wire_struct!(VlanPort {
    partition,
    slot,
    tagged
});

/// Who is on one VLAN of a switch.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VlanUse {
    /// The switch.
    pub switch: String,
    /// The VLAN.
    pub vlan: u16,
    /// Whether the uplink carries it untagged.
    pub native: bool,
    /// The ports on it.
    pub ports: Vec<VlanPort>,
}
wire_struct!(VlanUse {
    switch,
    vlan,
    native,
    ports
});

/// Ask for the switches' limits.
pub type GetLimits = Empty;

/// The switches' limits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Limits {
    /// Switches on the host.
    pub switches: u32,
    /// VLANs of one switch.
    pub vlans_per_switch: u32,
    /// Ethernet adapters of one partition.
    pub adapters_per_partition: u32,
    /// Partitions' ports on one switch.
    pub ports_per_switch: u32,
}
wire_struct!(Limits {
    switches,
    vlans_per_switch,
    adapters_per_partition,
    ports_per_switch
});

// Shared processor pools.

/// A shared processor pool, as made or changed. The default pool is
/// not one: it has every core no other has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcPoolSpec {
    /// Its name.
    pub name: String,
    /// Its cores, by number.
    pub cores: Vec<u32>,
    /// The most units its partitions use together, hundredths of a
    /// core; its cores' worth when none.
    pub maximum_units: Option<u32>,
}
wire_struct!(ProcPoolSpec {
    name,
    cores,
    maximum_units
});

/// Delete a shared processor pool no partition runs in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcPoolDelete {
    /// Its name.
    pub name: String,
}
wire_struct!(ProcPoolDelete { name });

// Events.

/// A storage pool changed: its state, its work, its allocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoolChanged {
    /// Its name.
    pub name: String,
    /// It now; `None` when it is gone.
    pub pool: Option<StoragePool>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(PoolChanged { name, pool, time });

/// A virtual disk was made, grown, deleted, held or given back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VdiskChanged {
    /// Its pool.
    pub pool: String,
    /// Its name.
    pub name: String,
    /// It now; `None` when it is gone.
    pub vdisk: Option<Vdisk>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(VdiskChanged {
    pool,
    name,
    vdisk,
    time
});

/// An image was added, renamed away, deleted, loaded or unloaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageChanged {
    /// Its name.
    pub name: String,
    /// It now; `None` when it is gone.
    pub image: Option<Image>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(ImageChanged { name, image, time });

/// An upload or an import ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadEnded {
    /// The image's name.
    pub name: String,
    /// The image, when it is in the library.
    pub image: Option<Image>,
    /// Why it is not; `None` when it is.
    pub error: Option<Note>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(UploadEnded {
    name,
    image,
    error,
    time
});

/// A tape volume was made, deleted, protected, mounted or unmounted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeChanged {
    /// Its name.
    pub name: String,
    /// It now; `None` when it is gone.
    pub volume: Option<Volume>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(VolumeChanged { name, volume, time });

/// What a partition's optical or tape device holds changed: loaded or
/// unloaded from a console, or by the system itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriveChanged {
    /// The partition.
    pub id: PartitionId,
    /// The device.
    pub drive: Drive,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(DriveChanged { id, drive, time });

/// A switch was made, changed or deleted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SwitchChanged {
    /// Its name.
    pub name: String,
    /// It now; `None` when it is gone.
    pub switch: Option<Switch>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(SwitchChanged { name, switch, time });

/// An uplink's interface lost or regained carrier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UplinkChanged {
    /// The switch.
    pub switch: String,
    /// The interface's location code.
    pub location: String,
    /// Whether it has carrier now.
    pub carrier: bool,
    /// The uplink's state now, in the host's words.
    pub state: String,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(UplinkChanged {
    switch,
    location,
    carrier,
    state,
    time
});

/// A partition's port was opened or closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortChanged {
    /// The partition.
    pub partition: PartitionId,
    /// The adapter's slot.
    pub slot: u32,
    /// The switch.
    pub switch: String,
    /// Whether it is open now.
    pub open: bool,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(PortChanged {
    partition,
    slot,
    switch,
    open,
    time
});

/// Operation numbers.
pub mod op {
    /// [`super::Pools`].
    pub const POOLS: u16 = 0x0001;
    /// [`super::Vdisks`].
    pub const VDISKS: u16 = 0x0010;
    /// [`super::VdiskCreate`].
    pub const VDISK_CREATE: u16 = 0x0011;
    /// [`super::VdiskResize`].
    pub const VDISK_RESIZE: u16 = 0x0012;
    /// [`super::VdiskDelete`].
    pub const VDISK_DELETE: u16 = 0x0013;
    /// [`super::VdiskNamed`].
    pub const VDISK_NAMED: u16 = 0x0014;
    /// [`super::Images`].
    pub const IMAGES: u16 = 0x0020;
    /// [`super::ImageRename`].
    pub const IMAGE_RENAME: u16 = 0x0021;
    /// [`super::ImageDelete`].
    pub const IMAGE_DELETE: u16 = 0x0022;
    /// [`super::ImageUpload`].
    pub const IMAGE_UPLOAD: u16 = 0x0023;
    /// [`super::Drives`].
    pub const DRIVES: u16 = 0x0024;
    /// [`super::Load`].
    pub const LOAD: u16 = 0x0025;
    /// [`super::Unload`].
    pub const UNLOAD: u16 = 0x0026;
    /// [`super::Volumes`].
    pub const VOLUMES: u16 = 0x0030;
    /// [`super::VolumeCreate`].
    pub const VOLUME_CREATE: u16 = 0x0031;
    /// [`super::VolumeDelete`].
    pub const VOLUME_DELETE: u16 = 0x0032;
    /// [`super::VolumeProtect`].
    pub const VOLUME_PROTECT: u16 = 0x0033;
    /// [`super::Switches`].
    pub const SWITCHES: u16 = 0x0040;
    /// [`super::Ports`].
    pub const PORTS: u16 = 0x0041;
    /// [`super::Fdb`].
    pub const FDB: u16 = 0x0042;
    /// [`super::Vlans`].
    pub const VLANS: u16 = 0x0043;
    /// [`super::GetLimits`].
    pub const LIMITS: u16 = 0x0044;
    /// [`super::ProcPoolSpec`], made.
    pub const PROC_POOL_CREATE: u16 = 0x0050;
    /// [`super::ProcPoolSpec`], changed.
    pub const PROC_POOL_CHANGE: u16 = 0x0051;
    /// [`super::ProcPoolDelete`].
    pub const PROC_POOL_DELETE: u16 = 0x0052;
}

/// Event numbers.
pub mod event {
    /// [`super::PoolChanged`].
    pub const POOL: u16 = 0x0001;
    /// [`super::VdiskChanged`].
    pub const VDISK: u16 = 0x0002;
    /// [`super::ImageChanged`].
    pub const IMAGE: u16 = 0x0003;
    /// [`super::UploadEnded`].
    pub const UPLOAD: u16 = 0x0004;
    /// [`super::VolumeChanged`].
    pub const VOLUME: u16 = 0x0005;
    /// [`super::DriveChanged`].
    pub const DRIVE: u16 = 0x0006;
    /// [`super::SwitchChanged`].
    pub const SWITCH: u16 = 0x0007;
    /// [`super::UplinkChanged`].
    pub const UPLINK: u16 = 0x0008;
    /// [`super::PortChanged`].
    pub const PORT: u16 = 0x0009;
}

/// The family's own refusals (`QSI0701` to `QSI0999`).
pub mod msgid {
    /// An upload's stream carried more bytes than its request said.
    pub const UPLOAD_LONG: &str = "QSI0701";
    /// An upload's stream ended before the bytes its request said.
    pub const UPLOAD_CUT: &str = "QSI0702";
}

messages! {
    /// What a console asks.
    pub enum Request: family::VRES => {
        /// Every storage pool.
        0x0001 => Pools(Pools),
        /// A pool's virtual disks.
        0x0010 => Vdisks(Vdisks),
        /// Make a virtual disk.
        0x0011 => VdiskCreate(VdiskCreate),
        /// Grow one.
        0x0012 => VdiskResize(VdiskResize),
        /// Delete one.
        0x0013 => VdiskDelete(VdiskDelete),
        /// The profiles that name one.
        0x0014 => VdiskNamed(VdiskNamed),
        /// The optical images.
        0x0020 => Images(Images),
        /// Rename an image.
        0x0021 => ImageRename(ImageRename),
        /// Delete an image.
        0x0022 => ImageDelete(ImageDelete),
        /// Upload an image: the reply opens the stream.
        0x0023 => ImageUpload(ImageUpload),
        /// A partition's optical and tape devices.
        0x0024 => Drives(Drives),
        /// Load or mount a medium.
        0x0025 => Load(Load),
        /// Unload or unmount one.
        0x0026 => Unload(Unload),
        /// The tape volumes.
        0x0030 => Volumes(Volumes),
        /// Make a tape volume.
        0x0031 => VolumeCreate(VolumeCreate),
        /// Delete one.
        0x0032 => VolumeDelete(VolumeDelete),
        /// Set or clear its write protection.
        0x0033 => VolumeProtect(VolumeProtect),
        /// Every switch.
        0x0040 => Switches(Switches),
        /// The partitions' ports.
        0x0041 => Ports(Ports),
        /// A switch's forwarding table.
        0x0042 => Fdb(Fdb),
        /// Who is on which VLAN.
        0x0043 => Vlans(Vlans),
        /// The switches' limits.
        0x0044 => Limits(GetLimits),
        /// Make a shared processor pool.
        0x0050 => ProcPoolCreate(ProcPoolSpec),
        /// Change one.
        0x0051 => ProcPoolChange(ProcPoolSpec),
        /// Delete one.
        0x0052 => ProcPoolDelete(ProcPoolDelete),
    }
}

messages! {
    /// What the host answers.
    pub enum Reply: family::VRES => {
        /// The pools.
        0x0001 => Pools(Vec<StoragePool>),
        /// The virtual disks.
        0x0010 => Vdisks(Vec<Vdisk>),
        /// The disk made.
        0x0011 => VdiskCreate(Vdisk),
        /// The disk grown.
        0x0012 => VdiskResize(Vdisk),
        /// Deleted.
        0x0013 => VdiskDelete(Empty),
        /// The profiles that name it.
        0x0014 => VdiskNamed(Vec<Naming>),
        /// The images.
        0x0020 => Images(Vec<Image>),
        /// Renamed.
        0x0021 => ImageRename(Empty),
        /// Deleted.
        0x0022 => ImageDelete(Empty),
        /// The stream is open.
        0x0023 => ImageUpload(Uploading),
        /// The devices.
        0x0024 => Drives(Vec<Drive>),
        /// Loaded.
        0x0025 => Load(Empty),
        /// The medium unloaded.
        0x0026 => Unload(Unloaded),
        /// The volumes.
        0x0030 => Volumes(Vec<Volume>),
        /// The volume made.
        0x0031 => VolumeCreate(Volume),
        /// Deleted.
        0x0032 => VolumeDelete(Empty),
        /// Set.
        0x0033 => VolumeProtect(Empty),
        /// The switches.
        0x0040 => Switches(Vec<Switch>),
        /// The ports.
        0x0041 => Ports(Vec<Port>),
        /// The table.
        0x0042 => Fdb(Vec<FdbRow>),
        /// The VLANs.
        0x0043 => Vlans(Vec<VlanUse>),
        /// The limits.
        0x0044 => Limits(Limits),
        /// Made.
        0x0050 => ProcPoolCreate(Empty),
        /// Changed.
        0x0051 => ProcPoolChange(Empty),
        /// Deleted.
        0x0052 => ProcPoolDelete(Empty),
    }
}

messages! {
    /// What the host pushes.
    pub enum Event: family::VRES => {
        /// A storage pool changed.
        0x0001 => Pool(PoolChanged),
        /// A virtual disk changed.
        0x0002 => Vdisk(VdiskChanged),
        /// An image changed.
        0x0003 => Image(ImageChanged),
        /// An upload ended.
        0x0004 => Upload(UploadEnded),
        /// A tape volume changed.
        0x0005 => Volume(VolumeChanged),
        /// A partition's device changed.
        0x0006 => Drive(DriveChanged),
        /// A switch changed.
        0x0007 => Switch(SwitchChanged),
        /// An uplink's carrier changed.
        0x0008 => Uplink(UplinkChanged),
        /// A port opened or closed.
        0x0009 => Port(PortChanged),
    }
}

impl Request {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::Pools(_)
            | Self::Images(_)
            | Self::Volumes(_)
            | Self::Switches(_)
            | Self::Vlans(_)
            | Self::Limits(_) => Ok(()),
            Self::Vdisks(Vdisks { pool: n })
            | Self::ImageDelete(ImageDelete { name: n })
            | Self::VolumeDelete(VolumeDelete { name: n })
            | Self::VolumeProtect(VolumeProtect { name: n, .. })
            | Self::Ports(Ports { switch: n })
            | Self::Fdb(Fdb { switch: n })
            | Self::ProcPoolDelete(ProcPoolDelete { name: n })
            | Self::ImageUpload(ImageUpload { name: n, .. }) => check_name(n),
            Self::VdiskCreate(VdiskCreate { pool, name, .. })
            | Self::VdiskResize(VdiskResize { pool, name, .. })
            | Self::VdiskDelete(VdiskDelete { pool, name, .. })
            | Self::VdiskNamed(VdiskNamed { pool, name }) => {
                check_name(pool)?;
                check_name(name)
            }
            Self::ImageRename(ImageRename { name, to }) => {
                check_name(name)?;
                check_name(to)
            }
            Self::Drives(Drives { id }) | Self::Unload(Unload { id, .. }) => check_id(*id),
            Self::Load(l) => {
                check_id(l.id)?;
                check_name(&l.name)
            }
            Self::VolumeCreate(v) => {
                check_name(&v.name)?;
                check_name(&v.identifier)
            }
            Self::ProcPoolCreate(p) | Self::ProcPoolChange(p) => {
                check_name(&p.name)?;
                check_items(&p.cores)
            }
        }
    }
}

/// The virtual resources family.
#[derive(Debug)]
pub struct Vres;

impl Family for Vres {
    const ID: u16 = family::VRES;
    const VERSION: u16 = 1;
    const CAP: u64 = crate::caps::VRES;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
