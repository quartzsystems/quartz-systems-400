// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The base family: what any managed system answers, a virtual server
//! host or a standalone system. Who it is ([`Identify`]), its state
//! ([`GetState`]), power ([`Power`]), its reference code history
//! ([`Refcodes`]), events ([`Subscribe`]) and its clock ([`GetTime`]).
//! Behind a capability bit of their own ([`crate::caps::SYSTEM_DUMPS`]),
//! the system's own dumps: list, take, delete and download
//! ([`TakeDump`]).
//!
//! Every identifier a console shows (a state's words, a reference code,
//! its text) is the managed side's, passed through as it gives it.

use ipc::wire_struct;

use crate::hello::PeerKind;
use crate::host::model::{Note, check_name};
use crate::message::Empty;
use crate::{Family, family, messages, open_enum};

open_enum! {
    /// A managed system's power state.
    PowerState {
        /// Starting: the IPL is under way.
        Starting = 1,
        /// Running: the IPL is complete.
        Running = 2,
        /// Ending: a power down is under way.
        Ending = 3,
        /// Off, or about to be.
        Off = 4,
        /// Halted after a terminal reference code, its memory kept.
        Halted = 5,
    }
}

open_enum! {
    /// The keylock: as a system read it at its IPL, or a partition's
    /// position on its host.
    Keylock {
        /// Normal.
        Normal = 1,
        /// Manual.
        Manual = 2,
        /// Auto: unattended, remote and timed IPLs allowed (a host's
        /// partitions have it).
        Auto = 3,
        /// Secure: unattended, the panel locked (a host's partitions
        /// have it).
        Secure = 4,
    }
}

open_enum! {
    /// What a power request asks.
    PowerAction {
        /// Power off.
        Off = 1,
        /// Power off and IPL again.
        Restart = 2,
    }
}

/// Ask who the system is.
pub type Identify = Empty;

/// Who the system is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// What it is.
    pub kind: PeerKind,
    /// Its name.
    pub name: String,
    /// Its release, in its own form.
    pub release: String,
    /// Its serial number.
    pub serial: String,
    /// Its machine type and model, in its own form; empty when unknown.
    pub model: String,
    /// The build it runs, in its own form.
    pub build: String,
}
wire_struct!(Identity {
    kind,
    name,
    release,
    serial,
    model,
    build
});

/// A reference code as posted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refcode {
    /// Its place in the history: counts up by one with each code, never
    /// reused. 0 when the code is not in the history.
    pub seq: u64,
    /// The code, eight characters.
    pub code: String,
    /// Words 2 to 9 in hexadecimal where the code has them.
    pub words: Vec<String>,
    /// Its text, in the system's words.
    pub text: String,
    /// When it was posted, Unix milliseconds.
    pub time_ms: u64,
}
wire_struct!(Refcode {
    seq,
    code,
    words,
    text,
    time_ms
});

/// Ask for the system's state.
pub type GetState = Empty;

/// The system's state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    /// The power state.
    pub power: PowerState,
    /// The code current now, when there is one.
    pub refcode: Option<Refcode>,
    /// The release running.
    pub release: String,
    /// The side the IPL was from (`A` or `B`); empty when the system has none.
    pub slot: String,
    /// The keylock, where there is one.
    pub keylock: Option<Keylock>,
}
wire_struct!(State {
    power,
    refcode,
    release,
    slot,
    keylock
});

/// Power the system off or restart it. The answer is [`Empty`]: the
/// power down was started; its codes and states follow as events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Power {
    /// Off or restart.
    pub action: PowerAction,
    /// At once, rather than controlled.
    pub immediate: bool,
    /// How long a controlled end may take, seconds.
    pub delay: u32,
}
wire_struct!(Power {
    action,
    immediate,
    delay
});

/// Read the reference code history, newest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refcodes {
    /// Codes before this place in the history; `None` from the newest.
    pub before: Option<u64>,
    /// At most this many; the managed side may give fewer.
    pub limit: u32,
}
wire_struct!(Refcodes { before, limit });

/// A page of the history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefcodeList {
    /// The codes, newest first.
    pub codes: Vec<Refcode>,
    /// Where the next page starts (`before`), when there is one.
    pub more: Option<u64>,
}
wire_struct!(RefcodeList { codes, more });

/// Ask for events: those after `since`, then each as it happens.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subscribe {
    /// The last event's number this console saw, on a reconnect.
    pub since: Option<u64>,
}
wire_struct!(Subscribe { since });

open_enum! {
    /// What a subscription found of what was missed.
    Replay {
        /// Nothing to replay: a first subscription.
        Fresh = 1,
        /// What was missed follows, then each event as it happens.
        Resumed = 2,
        /// The log no longer reaches back that far: read everything
        /// again, then events follow.
        ReadAgain = 3,
    }
}

/// The answer to [`Subscribe`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subscribed {
    /// What was found.
    pub replay: Replay,
    /// How many events follow at once, replayed.
    pub replayed: u32,
    /// The number the next new event will have.
    pub next: u64,
}
wire_struct!(Subscribed {
    replay,
    replayed,
    next
});

/// Stop the events.
pub type Unsubscribe = Empty;

/// Ask for the system's clock.
pub type GetTime = Empty;

/// The system's clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Time {
    /// Now, Unix nanoseconds.
    pub unix_ns: i64,
    /// The offset from UTC in force, seconds.
    pub offset_s: i32,
    /// The time zone's name, in the system's form; empty when none.
    pub zone: String,
}
wire_struct!(Time {
    unix_ns,
    offset_s,
    zone
});

open_enum! {
    /// What a system's own dump holds.
    DumpKind {
        /// The system's service data: its logs, state and history.
        ServiceData = 1,
        /// A kind the managed side names in `kind_name`.
        System = 2,
    }
}

/// Ask for the system's own dumps.
pub type ListDumps = Empty;

/// One of the system's own dumps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemDump {
    /// Its name.
    pub name: String,
    /// What it holds.
    pub kind: DumpKind,
    /// The managed side's name for its kind; may be empty for service
    /// data.
    pub kind_name: String,
    /// Its size, bytes.
    pub bytes: u64,
    /// When it was taken, seconds since the epoch.
    pub time: u64,
    /// What asked for it, in the managed side's words (a console's
    /// user, a reference code, the operator).
    pub by: String,
}
wire_struct!(SystemDump {
    name,
    kind,
    kind_name,
    bytes,
    time,
    by
});

/// Take a dump now. The answer is its name; it is written in the
/// background and [`DumpTaken`] says when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TakeDump {
    /// What it is to hold.
    pub kind: DumpKind,
    /// The managed side's name for a [`DumpKind::System`] kind; empty
    /// otherwise.
    pub kind_name: String,
}
wire_struct!(TakeDump { kind, kind_name });

/// The name a dump being taken has.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DumpBegun {
    /// The dump's name.
    pub name: String,
}
wire_struct!(DumpBegun { name });

/// Delete one of the system's dumps.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeleteDump {
    /// The dump.
    pub name: String,
}
wire_struct!(DeleteDump { name });

/// Download one of the system's dumps: the reply opens a stream of its
/// bytes, managed side to console, which ends after the last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadDump {
    /// The dump.
    pub name: String,
}
wire_struct!(DownloadDump { name });

/// A download's stream is open: this many bytes follow on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DumpDownloading {
    /// The dump.
    pub name: String,
    /// Its size, bytes.
    pub bytes: u64,
}
wire_struct!(DumpDownloading { name, bytes });

/// A dump was written, or could not be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DumpTaken {
    /// The dump's name.
    pub name: String,
    /// The dump; `None` when it was not written.
    pub dump: Option<SystemDump>,
    /// Why it was not written; `None` when it was.
    pub error: Option<Note>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(DumpTaken {
    name,
    dump,
    error,
    time
});

/// A dump was removed: deleted, or put out for the managed side's
/// limits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DumpRemoved {
    /// The dump's name.
    pub name: String,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(DumpRemoved { name, time });

/// Operation numbers.
pub mod op {
    /// [`super::Identify`].
    pub const IDENTIFY: u16 = 0x0001;
    /// [`super::GetState`].
    pub const STATE: u16 = 0x0002;
    /// [`super::Power`].
    pub const POWER: u16 = 0x0003;
    /// [`super::Refcodes`].
    pub const REFCODES: u16 = 0x0004;
    /// [`super::Subscribe`].
    pub const SUBSCRIBE: u16 = 0x0005;
    /// [`super::Unsubscribe`].
    pub const UNSUBSCRIBE: u16 = 0x0006;
    /// [`super::GetTime`].
    pub const TIME: u16 = 0x0007;
    /// [`super::ListDumps`] ([`crate::caps::SYSTEM_DUMPS`]).
    pub const DUMPS: u16 = 0x0010;
    /// [`super::TakeDump`] ([`crate::caps::SYSTEM_DUMPS`]).
    pub const TAKE_DUMP: u16 = 0x0011;
    /// [`super::DeleteDump`] ([`crate::caps::SYSTEM_DUMPS`]).
    pub const DELETE_DUMP: u16 = 0x0012;
    /// [`super::DownloadDump`] ([`crate::caps::SYSTEM_DUMPS`]).
    pub const DOWNLOAD_DUMP: u16 = 0x0013;

    /// Whether `op` is one of the system's dumps, which need
    /// [`crate::caps::SYSTEM_DUMPS`].
    #[must_use]
    pub fn is_dump(op: u16) -> bool {
        (DUMPS..=DOWNLOAD_DUMP).contains(&op)
    }
}

/// Event numbers.
pub mod event {
    /// A reference code was posted.
    pub const REFCODE: u16 = 0x0001;
    /// The state changed.
    pub const STATE: u16 = 0x0002;
    /// What [`super::Identity`] says changed.
    pub const IDENTITY: u16 = 0x0003;
    /// [`super::DumpTaken`] ([`crate::caps::SYSTEM_DUMPS`]).
    pub const DUMP_TAKEN: u16 = 0x0010;
    /// [`super::DumpRemoved`] ([`crate::caps::SYSTEM_DUMPS`]).
    pub const DUMP_REMOVED: u16 = 0x0011;

    /// Whether `op` is an event of the system's dumps, which needs
    /// [`crate::caps::SYSTEM_DUMPS`].
    #[must_use]
    pub fn is_dump(op: u16) -> bool {
        (DUMP_TAKEN..=DUMP_REMOVED).contains(&op)
    }
}

/// The refusals of the system's dumps (`QSI1201` to `QSI1249`).
pub mod msgid {
    /// No dump of that name, from a managed side with no identifier of
    /// its own.
    pub const NO_DUMP: &str = "QSI1201";
    /// A dump is being taken already: one at a time.
    pub const TAKING: &str = "QSI1202";
    /// A kind of dump this system does not take.
    pub const NO_KIND: &str = "QSI1203";
}

messages! {
    /// What a console asks.
    pub enum Request: family::BASE => {
        /// Who the system is.
        0x0001 => Identify(Identify),
        /// Its state.
        0x0002 => State(GetState),
        /// Power off or restart.
        0x0003 => Power(Power),
        /// The code history.
        0x0004 => Refcodes(Refcodes),
        /// Events.
        0x0005 => Subscribe(Subscribe),
        /// No more events.
        0x0006 => Unsubscribe(Unsubscribe),
        /// The clock.
        0x0007 => Time(GetTime),
        /// The system's own dumps.
        0x0010 => Dumps(ListDumps),
        /// Take one now.
        0x0011 => TakeDump(TakeDump),
        /// Delete one.
        0x0012 => DeleteDump(DeleteDump),
        /// Download one.
        0x0013 => DownloadDump(DownloadDump),
    }
}

messages! {
    /// What the managed side answers.
    pub enum Reply: family::BASE => {
        /// Who the system is.
        0x0001 => Identify(Identity),
        /// Its state.
        0x0002 => State(State),
        /// The power down was started.
        0x0003 => Power(Empty),
        /// A page of the history.
        0x0004 => Refcodes(RefcodeList),
        /// Subscribed.
        0x0005 => Subscribe(Subscribed),
        /// Unsubscribed.
        0x0006 => Unsubscribe(Empty),
        /// The clock.
        0x0007 => Time(Time),
        /// The dumps, oldest first.
        0x0010 => Dumps(Vec<SystemDump>),
        /// Begun.
        0x0011 => TakeDump(DumpBegun),
        /// Deleted.
        0x0012 => DeleteDump(Empty),
        /// The stream is open.
        0x0013 => DownloadDump(DumpDownloading),
    }
}

messages! {
    /// What the managed side pushes.
    pub enum Event: family::BASE => {
        /// A reference code was posted.
        0x0001 => Refcode(Refcode),
        /// The state changed.
        0x0002 => State(State),
        /// What it says of itself changed.
        0x0003 => Identity(Identity),
        /// A dump was written, or could not be.
        0x0010 => DumpTaken(DumpTaken),
        /// A dump was removed.
        0x0011 => DumpRemoved(DumpRemoved),
    }
}

/// The longest delay of a controlled power down, seconds.
pub const MAX_DELAY: u32 = 99_999;

impl Request {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::Power(p) if p.delay > MAX_DELAY => Err("a delay over 99999 seconds"),
            Self::TakeDump(t) => check_name(&t.kind_name),
            Self::DeleteDump(DeleteDump { name }) | Self::DownloadDump(DownloadDump { name }) => {
                check_name(name)
            }
            _ => Ok(()),
        }
    }
}

/// The base family.
#[derive(Debug)]
pub struct Base;

impl Family for Base {
    const ID: u16 = family::BASE;
    const VERSION: u16 = 1;
    const CAP: u64 = crate::caps::BASE;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
