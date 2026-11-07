// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Manage` (version 7, draft): subsystems, system values, PTFs, disk
//! units, hardware resources, the IPL and power. Each operation does
//! what its command does, with that command's authority checks and
//! audit entries (docs/QAP.md, "Service 8 `Manage`").

use ipc::{wire_enum, wire_struct};

use crate::types::{JobId, ObjectName};

/// A subsystem's state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SubsystemStatus {
    /// Started, taking work.
    #[default]
    Active,
    /// Not started.
    Inactive,
    /// Ending.
    Ending,
    /// Starting.
    Starting,
}

wire_enum!(SubsystemStatus {
    0 => Active,
    1 => Inactive,
    2 => Ending,
    3 => Starting,
});

impl SubsystemStatus {
    /// Every status, in value order.
    pub const ALL: [SubsystemStatus; 4] = [
        SubsystemStatus::Active,
        SubsystemStatus::Inactive,
        SubsystemStatus::Ending,
        SubsystemStatus::Starting,
    ];
}

/// A subsystem description and its state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subsystem {
    /// The subsystem description.
    pub sbsd: ObjectName,
    /// Its state.
    pub status: SubsystemStatus,
    /// Jobs active in it.
    pub jobs: u32,
    /// Its monitor job, while it runs.
    pub monitor: Option<JobId>,
    /// The description's text.
    pub text: String,
}

wire_struct!(Subsystem {
    sbsd,
    status,
    jobs,
    monitor,
    text
});

/// `ListSubsystems`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListSubsystems {
    /// `*ALL`, `*LIBL` or a library.
    pub library: String,
    /// The first entry, 0-based.
    pub start: u32,
    /// Entries at most; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListSubsystems {
    library,
    start,
    max
});

/// `ListSubsystems`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subsystems {
    /// Descriptions matching.
    pub total: u32,
    /// Those returned, active ones first, then by name.
    pub subsystems: Vec<Subsystem>,
}

wire_struct!(Subsystems { total, subsystems });

/// A subsystem by its description: `StartSubsystem`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SubsystemRef {
    /// The description; the library may be `*LIBL`.
    pub sbsd: ObjectName,
}

wire_struct!(SubsystemRef { sbsd });

/// How work ends.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EndHow {
    /// `*CNTRLD`: jobs are told and given the delay.
    #[default]
    Controlled,
    /// `*IMMED`.
    Immediate,
}

wire_enum!(EndHow {
    0 => Controlled,
    1 => Immediate,
});

/// `EndSubsystem`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EndSubsystem {
    /// The subsystem; `None` for `*ALL`.
    pub sbsd: Option<ObjectName>,
    /// How.
    pub how: EndHow,
    /// Seconds a controlled end waits; `None` for `*NOLIMIT`.
    pub delay: Option<u32>,
}

wire_struct!(EndSubsystem { sbsd, how, delay });

/// A system value's type.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ValueKind {
    /// Character.
    #[default]
    Character,
    /// Decimal.
    Decimal,
    /// Binary integer.
    Integer,
    /// A list of character values.
    List,
}

wire_enum!(ValueKind {
    0 => Character,
    1 => Decimal,
    2 => Integer,
    3 => List,
});

impl ValueKind {
    /// Every type, in value order.
    pub const ALL: [ValueKind; 4] = [
        ValueKind::Character,
        ValueKind::Decimal,
        ValueKind::Integer,
        ValueKind::List,
    ];
}

/// A system value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemValue {
    /// Its name (`QMAXSIGN`).
    pub name: String,
    /// Its type.
    pub kind: ValueKind,
    /// Its value as `CHGSYSVAL VALUE()` takes it, without apostrophes;
    /// a list's entries separated by one blank.
    pub value: String,
    /// The value's description.
    pub text: String,
    /// Its catalog group (`SEC`, `DATTIM`, ...).
    pub category: String,
}

wire_struct!(SystemValue {
    name,
    kind,
    value,
    text,
    category
});

/// `GetSystemValues`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GetSystemValues {
    /// The values, by name; empty for every one.
    pub names: Vec<String>,
}

wire_struct!(GetSystemValues { names });

/// `GetSystemValues`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemValues {
    /// The values, in the order asked (by name for all).
    pub values: Vec<SystemValue>,
}

wire_struct!(SystemValues { values });

/// `SetSystemValue`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetSystemValue {
    /// The value's name.
    pub name: String,
    /// The new value, as [`SystemValue::value`].
    pub value: String,
}

wire_struct!(SetSystemValue { name, value });

/// A PTF's state, as `DSPPTF` shows it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PtfStatus {
    /// Loaded, not applied.
    #[default]
    NotApplied,
    /// Temporarily applied.
    TemporarilyApplied,
    /// Permanently applied.
    PermanentlyApplied,
    /// Removed temporarily; still loaded.
    TemporarilyRemoved,
    /// Removed for good.
    PermanentlyRemoved,
    /// A later PTF supersedes it.
    Superseded,
    /// Damaged.
    Damaged,
    /// Only its save file or cover letter is on the system.
    SaveFileOnly,
}

wire_enum!(PtfStatus {
    0 => NotApplied,
    1 => TemporarilyApplied,
    2 => PermanentlyApplied,
    3 => TemporarilyRemoved,
    4 => PermanentlyRemoved,
    5 => Superseded,
    6 => Damaged,
    7 => SaveFileOnly,
});

impl PtfStatus {
    /// Every status, in value order.
    pub const ALL: [PtfStatus; 8] = [
        PtfStatus::NotApplied,
        PtfStatus::TemporarilyApplied,
        PtfStatus::PermanentlyApplied,
        PtfStatus::TemporarilyRemoved,
        PtfStatus::PermanentlyRemoved,
        PtfStatus::Superseded,
        PtfStatus::Damaged,
        PtfStatus::SaveFileOnly,
    ];
}

/// What a PTF waits for at the next IPL.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IplAction {
    /// Nothing.
    #[default]
    None,
    /// Applied at the next IPL.
    Apply,
    /// Removed at the next IPL.
    Remove,
}

wire_enum!(IplAction {
    0 => None,
    1 => Apply,
    2 => Remove,
});

/// A PTF.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ptf {
    /// The product (`QS00SS1`).
    pub product: String,
    /// The PTF identifier (`QS00001`).
    pub id: String,
    /// The product's release (`V1R1M0`).
    pub release: String,
    /// Its state.
    pub status: PtfStatus,
    /// What the next IPL does with it.
    pub ipl_action: IplAction,
    /// Whether it needs an IPL to apply (delayed).
    pub delayed: bool,
    /// Its text.
    pub text: String,
}

wire_struct!(Ptf {
    product,
    id,
    release,
    status,
    ipl_action,
    delayed,
    text
});

/// `ListPtfs`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListPtfs {
    /// `*ALL` or a product.
    pub product: String,
    /// The first entry, 0-based.
    pub start: u32,
    /// Entries at most; 0 for no limit.
    pub max: u32,
}

wire_struct!(ListPtfs {
    product,
    start,
    max
});

/// `ListPtfs`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ptfs {
    /// PTFs matching.
    pub total: u32,
    /// Those returned, by product then identifier.
    pub ptfs: Vec<Ptf>,
}

wire_struct!(Ptfs { total, ptfs });

/// A disk unit's state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UnitStatus {
    /// In use.
    #[default]
    Active,
    /// Mirrored, its mirror suspended.
    Suspended,
    /// Its mirror being brought up to date.
    Resuming,
    /// Failed.
    Failed,
    /// Present, not in an ASP.
    NonConfigured,
}

wire_enum!(UnitStatus {
    0 => Active,
    1 => Suspended,
    2 => Resuming,
    3 => Failed,
    4 => NonConfigured,
});

/// A unit's protection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Protection {
    /// None.
    #[default]
    Unprotected,
    /// Mirrored.
    Mirrored,
    /// Device parity.
    Parity,
}

wire_enum!(Protection {
    0 => Unprotected,
    1 => Mirrored,
    2 => Parity,
});

/// A disk unit, as `WRKDSKSTS` and DST's configuration show it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskUnit {
    /// The unit number; 0 for one not configured.
    pub unit: u16,
    /// Its ASP; 0 for none.
    pub asp: u16,
    /// Its resource name (`DD001`).
    pub resource: String,
    /// Type and model, as the hardware list shows them.
    pub type_model: String,
    /// The serial number.
    pub serial: String,
    /// Bytes.
    pub capacity: u64,
    /// Bytes in use.
    pub used: u64,
    /// Its state.
    pub status: UnitStatus,
    /// Its protection.
    pub protection: Protection,
}

wire_struct!(DiskUnit {
    unit,
    asp,
    resource,
    type_model,
    serial,
    capacity,
    used,
    status,
    protection
});

/// An auxiliary storage pool.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Asp {
    /// Its number.
    pub asp: u16,
    /// Bytes.
    pub capacity: u64,
    /// Bytes in use.
    pub used: u64,
    /// The storage threshold, percent.
    pub threshold: u8,
}

wire_struct!(Asp {
    asp,
    capacity,
    used,
    threshold
});

/// `ListDiskUnits`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskUnits {
    /// The units, configured ones by unit number, then the others.
    pub units: Vec<DiskUnit>,
    /// The ASPs, by number.
    pub asps: Vec<Asp>,
}

wire_struct!(DiskUnits { units, asps });

/// A `WRKHDWRSC TYPE`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResourceType {
    /// `*AHW`: all.
    #[default]
    All,
    /// `*CMN`.
    Communication,
    /// `*LWS`.
    LocalWorkStation,
    /// `*PRC`.
    Processor,
    /// `*STG`.
    Storage,
    /// `*CSA`.
    Coupled,
    /// `*CRP`.
    Cryptographic,
}

wire_enum!(ResourceType {
    0 => All,
    1 => Communication,
    2 => LocalWorkStation,
    3 => Processor,
    4 => Storage,
    5 => Coupled,
    6 => Cryptographic,
});

impl ResourceType {
    /// Every type, in value order.
    pub const ALL: [ResourceType; 7] = [
        ResourceType::All,
        ResourceType::Communication,
        ResourceType::LocalWorkStation,
        ResourceType::Processor,
        ResourceType::Storage,
        ResourceType::Coupled,
        ResourceType::Cryptographic,
    ];

    /// The command's special value.
    #[must_use]
    pub const fn special(self) -> &'static str {
        match self {
            ResourceType::All => "*AHW",
            ResourceType::Communication => "*CMN",
            ResourceType::LocalWorkStation => "*LWS",
            ResourceType::Processor => "*PRC",
            ResourceType::Storage => "*STG",
            ResourceType::Coupled => "*CSA",
            ResourceType::Cryptographic => "*CRP",
        }
    }
}

/// A hardware resource's state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResourceStatus {
    /// Present.
    #[default]
    Operational,
    /// Was present; not found now.
    NotDetected,
}

wire_enum!(ResourceStatus {
    0 => Operational,
    1 => NotDetected,
});

/// A hardware resource, as `WRKHDWRSC` lists it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HardwareResource {
    /// Its name (`CMN01`).
    pub name: String,
    /// Levels below the top of the list (the packaging tree).
    pub depth: u8,
    /// Type and model (`290A-001`), or the type alone.
    pub type_model: String,
    /// Its state.
    pub status: ResourceStatus,
    /// The list's text (`Ethernet Port`).
    pub text: String,
    /// The location (a PCI address, a device).
    pub location: String,
    /// The serial number; empty when none.
    pub serial: String,
    /// The resource it sits in; empty at the top.
    pub parent: String,
}

wire_struct!(HardwareResource {
    name,
    depth,
    type_model,
    status,
    text,
    location,
    serial,
    parent
});

/// `ListHardwareResources`'s request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListHardwareResources {
    /// The type.
    pub kind: ResourceType,
}

wire_struct!(ListHardwareResources { kind });

/// `ListHardwareResources`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HardwareResources {
    /// The resources, in tree order.
    pub resources: Vec<HardwareResource>,
}

wire_struct!(HardwareResources { resources });

/// The keylock's mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IplMode {
    /// Normal.
    #[default]
    Normal,
    /// Manual.
    Manual,
}

wire_enum!(IplMode {
    0 => Normal,
    1 => Manual,
});

/// How the last power down ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PreviousEnd {
    /// Normally.
    #[default]
    Normal,
    /// Abnormally.
    Abnormal,
}

wire_enum!(PreviousEnd {
    0 => Normal,
    1 => Abnormal,
});

/// `IplStatus`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IplStatus {
    /// When the last IPL ended, milliseconds since 1970-01-01 UTC.
    pub ipl_time: i64,
    /// The IPL type (`A`, `B`, `D`).
    pub ipl_type: String,
    /// The mode.
    pub mode: IplMode,
    /// The LIC slot booted (`A`, `B`).
    pub slot: String,
    /// The slot the next IPL boots once, if one is set.
    pub next_slot: Option<String>,
    /// The last system reference code, eight characters.
    pub last_src: String,
    /// Its text.
    pub src_text: String,
    /// How the power down before this IPL ended.
    pub previous_end: PreviousEnd,
    /// Whether the system is in restricted state.
    pub restricted: bool,
}

wire_struct!(IplStatus {
    ipl_time,
    ipl_type,
    mode,
    slot,
    next_slot,
    last_src,
    src_text,
    previous_end,
    restricted
});

/// Which LIC slot the next IPL boots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum IplSource {
    /// `*PANEL`: as set.
    #[default]
    Panel,
    /// Slot A.
    A,
    /// Slot B.
    B,
}

wire_enum!(IplSource {
    0 => Panel,
    1 => A,
    2 => B,
});

/// `PowerDown`'s request: `PWRDWNSYS`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PowerDown {
    /// `OPTION`.
    pub how: EndHow,
    /// `DELAY` seconds for a controlled end; `None` for `*NOLIMIT`.
    pub delay: Option<u32>,
    /// `RESTART(*YES)`.
    pub restart: bool,
    /// `IPLSRC`.
    pub ipl_source: IplSource,
}

wire_struct!(PowerDown {
    how,
    delay,
    restart,
    ipl_source
});
