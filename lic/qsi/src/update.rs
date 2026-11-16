// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The update family: code update of a managed system whose own code is
//! in two slots (a virtual server host's is). One image is staged at a
//! time: uploaded on a stream and verified by the managed side, then
//! installed to the slot that is not running and marked pending, then
//! activated by a restart into it, which either confirms or rolls back.
//! Nothing running is disturbed before the activation.

use ipc::wire_struct;

use crate::base::MAX_DELAY;
use crate::host::model::{Note, check_name};
use crate::message::Empty;
use crate::{Family, family, messages, open_enum};

open_enum! {
    /// How a slot's last activation ended.
    Attempt {
        /// It was never activated from an update.
        Never = 1,
        /// It is pending: the next restart tries it.
        Pending = 2,
        /// It started and confirmed.
        Confirmed = 3,
        /// It did not confirm and the system went back to the other.
        RolledBack = 4,
    }
}

open_enum! {
    /// Where the staged image is.
    Verification {
        /// Its bytes are still coming.
        Receiving = 1,
        /// Whole, being verified.
        Verifying = 2,
        /// Verified: it can be installed.
        Verified = 3,
        /// Refused: the reason is in its `error`.
        Refused = 4,
    }
}

open_enum! {
    /// What an activation does with running partitions.
    Partitions {
        /// Refuse while any runs (the default).
        Refuse = 1,
        /// Shut each down through its operating system first.
        ShutDown = 2,
    }
}

/// What an update image is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    /// The product it is for, in the managed side's form.
    pub product: String,
    /// Its release, in the managed side's form.
    pub release: String,
    /// Its build.
    pub build: String,
    /// The fixes it names.
    pub fixes: Vec<String>,
    /// Its size, bytes.
    pub bytes: u64,
}
wire_struct!(Image {
    product,
    release,
    build,
    fixes,
    bytes
});

/// One slot of the system's own code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slot {
    /// Its name (`A`, `B`).
    pub name: String,
    /// The release in it; empty when it holds none.
    pub release: String,
    /// Its build.
    pub build: String,
    /// The fixes it names.
    pub fixes: Vec<String>,
    /// The system runs from it now.
    pub running: bool,
    /// The system starts from it by default.
    pub default: bool,
    /// How its last activation ended.
    pub last: Attempt,
    /// Why, in the managed side's words, when it rolled back.
    pub reason: String,
    /// When it last changed, seconds since the epoch; 0 when never.
    pub time: u64,
}
wire_struct!(Slot {
    name,
    release,
    build,
    fixes,
    running,
    default,
    last,
    reason,
    time
});

/// What is staged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedImage {
    /// The name the console gave it.
    pub name: String,
    /// Its size, bytes.
    pub bytes: u64,
    /// Where it is.
    pub verification: Verification,
    /// What it is, once verified.
    pub image: Option<Image>,
    /// Why it was refused, when it was.
    pub error: Option<Note>,
    /// When it was staged, seconds since the epoch.
    pub time: u64,
}
wire_struct!(StagedImage {
    name,
    bytes,
    verification,
    image,
    error,
    time
});

/// An install under way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    /// The slot being written.
    pub slot: String,
    /// How far, in percent.
    pub percent: u8,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(Progress {
    slot,
    percent,
    time
});

/// Ask for the code's status.
pub type GetStatus = Empty;

/// The code's status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// Each slot, in order (`A`, then `B`).
    pub slots: Vec<Slot>,
    /// What is staged, if anything.
    pub staged: Option<StagedImage>,
    /// An install under way, if any.
    pub installing: Option<Progress>,
}
wire_struct!(Status {
    slots,
    staged,
    installing
});

/// Stage an update image: the reply opens a stream, console to managed
/// side, of exactly `bytes` bytes. How it ended is [`Staged`], then
/// [`Verified`]. One image is staged at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stage {
    /// A name for it, the console's.
    pub name: String,
    /// Its size, bytes.
    pub bytes: u64,
    /// Accept a release older than the one running.
    pub allow_older: bool,
}
wire_struct!(Stage {
    name,
    bytes,
    allow_older
});

/// A stage's stream is open: send this many bytes on it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staging {
    /// The name.
    pub name: String,
    /// The bytes to send.
    pub bytes: u64,
}
wire_struct!(Staging { name, bytes });

/// Throw away what is staged.
pub type Discard = Empty;

/// Write the staged, verified image to the slot that is not running and
/// mark it pending. Progress follows as events.
pub type Install = Empty;

/// The slot an install writes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    /// The slot.
    pub slot: String,
}
wire_struct!(Target { slot });

/// Restart into the pending slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Activate {
    /// What to do with running partitions.
    pub partitions: Partitions,
    /// How long each partition's shut down may take, seconds, at most
    /// 99999; with [`Partitions::ShutDown`].
    pub timeout: u32,
}
wire_struct!(Activate {
    partitions,
    timeout
});

/// Clear a pending slot that has not been activated.
pub type Cancel = Empty;

/// A stage's upload ended: whole, or why not.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Staged {
    /// The name.
    pub name: String,
    /// The bytes received.
    pub bytes: u64,
    /// Why it is not staged; `None` when it is whole and being verified.
    pub error: Option<Note>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(Staged {
    name,
    bytes,
    error,
    time
});

/// The staged image was verified, or refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verified {
    /// The name.
    pub name: String,
    /// What it is; `None` when refused.
    pub image: Option<Image>,
    /// Why it was refused ([`msgid::BAD_SIGNATURE`] to
    /// [`msgid::NO_STATE`]); `None` when verified.
    pub error: Option<Note>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(Verified {
    name,
    image,
    error,
    time
});

/// An install ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installed {
    /// The slot written.
    pub slot: String,
    /// What it holds now and is pending; `None` when the install failed.
    pub image: Option<Image>,
    /// Why it failed; `None` when it did not.
    pub error: Option<Note>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(Installed {
    slot,
    image,
    error,
    time
});

/// The system is restarting into the pending slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Activating {
    /// The slot.
    pub slot: String,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(Activating { slot, time });

/// After the restart, the slot started and confirmed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirmed {
    /// The slot, now running and the default.
    pub slot: String,
    /// Its release.
    pub release: String,
    /// Its build.
    pub build: String,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(Confirmed {
    slot,
    release,
    build,
    time
});

/// After the restart, the slot did not confirm and the system went back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RolledBack {
    /// The slot that did not confirm.
    pub slot: String,
    /// Why, in the managed side's words.
    pub reason: String,
    /// The reference code that says why, when there is one.
    pub code: String,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(RolledBack {
    slot,
    reason,
    code,
    time
});

/// Operation numbers.
pub mod op {
    /// [`super::GetStatus`].
    pub const STATUS: u16 = 0x0001;
    /// [`super::Stage`].
    pub const STAGE: u16 = 0x0002;
    /// [`super::Discard`].
    pub const DISCARD: u16 = 0x0003;
    /// [`super::Install`].
    pub const INSTALL: u16 = 0x0004;
    /// [`super::Activate`].
    pub const ACTIVATE: u16 = 0x0005;
    /// [`super::Cancel`].
    pub const CANCEL: u16 = 0x0006;
}

/// Event numbers.
pub mod event {
    /// [`super::Staged`].
    pub const STAGED: u16 = 0x0001;
    /// [`super::Verified`].
    pub const VERIFIED: u16 = 0x0002;
    /// [`super::Progress`].
    pub const INSTALLING: u16 = 0x0003;
    /// [`super::Installed`].
    pub const INSTALLED: u16 = 0x0004;
    /// [`super::Activating`].
    pub const ACTIVATING: u16 = 0x0005;
    /// [`super::Confirmed`].
    pub const CONFIRMED: u16 = 0x0006;
    /// [`super::RolledBack`].
    pub const ROLLED_BACK: u16 = 0x0007;
}

/// The family's own refusals (`QSI1101` to `QSI1199`). A managed side
/// with identifiers of its own gives them after these.
pub mod msgid {
    /// The image's signature is not one this system trusts.
    pub const BAD_SIGNATURE: &str = "QSI1101";
    /// The image is damaged: it does not read, or a hash is wrong.
    pub const DAMAGED: &str = "QSI1102";
    /// The image is for another product.
    pub const WRONG_PRODUCT: &str = "QSI1103";
    /// The image is older than what runs, and the stage did not allow it.
    pub const OLDER: &str = "QSI1104";
    /// This system's own state (its slots, its record) cannot be read.
    pub const NO_STATE: &str = "QSI1105";
    /// Nothing is staged.
    pub const NOTHING_STAGED: &str = "QSI1106";
    /// An image is staged already: discard it first.
    pub const STAGED: &str = "QSI1107";
    /// The staged image is not verified.
    pub const NOT_VERIFIED: &str = "QSI1108";
    /// More bytes than the stage named: the stream is reset.
    pub const UPLOAD_LONG: &str = "QSI1109";
    /// Fewer bytes than the stage named: nothing is kept.
    pub const UPLOAD_CUT: &str = "QSI1110";
    /// An install or an activation is under way.
    pub const BUSY: &str = "QSI1111";
    /// No slot is pending.
    pub const NOTHING_PENDING: &str = "QSI1112";
    /// Partitions run, and the activation said to refuse.
    pub const PARTITIONS_RUN: &str = "QSI1113";
    /// A slot is pending already: activate or cancel it first.
    pub const PENDING: &str = "QSI1114";
}

messages! {
    /// What a console asks.
    pub enum Request: family::UPDATE => {
        /// The code's status.
        0x0001 => Status(GetStatus),
        /// Stage an image.
        0x0002 => Stage(Stage),
        /// Throw away what is staged.
        0x0003 => Discard(Discard),
        /// Install what is staged.
        0x0004 => Install(Install),
        /// Restart into the pending slot.
        0x0005 => Activate(Activate),
        /// Clear the pending slot.
        0x0006 => Cancel(Cancel),
    }
}

messages! {
    /// What the managed side answers.
    pub enum Reply: family::UPDATE => {
        /// The code's status.
        0x0001 => Status(Status),
        /// The stream is open.
        0x0002 => Stage(Staging),
        /// Discarded.
        0x0003 => Discard(Empty),
        /// Begun, to this slot.
        0x0004 => Install(Target),
        /// The restart was begun.
        0x0005 => Activate(Empty),
        /// Cancelled.
        0x0006 => Cancel(Empty),
    }
}

messages! {
    /// What the managed side pushes.
    pub enum Event: family::UPDATE => {
        /// An upload ended.
        0x0001 => Staged(Staged),
        /// Verified or refused.
        0x0002 => Verified(Verified),
        /// An install's progress.
        0x0003 => Installing(Progress),
        /// An install ended.
        0x0004 => Installed(Installed),
        /// Restarting into the pending slot.
        0x0005 => Activating(Activating),
        /// The slot confirmed after the restart.
        0x0006 => Confirmed(Confirmed),
        /// The slot rolled back after the restart.
        0x0007 => RolledBack(RolledBack),
    }
}

impl Request {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::Stage(s) if s.bytes == 0 => Err("an image of no bytes"),
            Self::Stage(s) => check_name(&s.name),
            Self::Activate(a) if a.timeout > MAX_DELAY => Err("a timeout over 99999 seconds"),
            _ => Ok(()),
        }
    }
}

/// The update family.
#[derive(Debug)]
pub struct Update;

impl Family for Update {
    const ID: u16 = family::UPDATE;
    const VERSION: u16 = 1;
    const CAP: u64 = crate::caps::UPDATE;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
