// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The partitions family: a host's partitions, read. The list, one
//! partition whole (its definition, state, reference code, current
//! resources, what its system reported and its service channel), and
//! the virtual Ethernet adapters of their profiles. The events keep a
//! console current without polling: a state, the resources, a
//! definition, a channel.
//!
//! Behind [`crate::caps::ACTIVATIONS`]: when each partition's current
//! activation began, and with which profile ([`Activations`]).

use ipc::wire_struct;

use super::model::{Channel, Current, Definition, Note, PartitionId, PartitionState, check_id};
use crate::message::Empty;
use crate::{Family, family, messages};

/// A virtual Ethernet adapter of a partition, as a list shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Link {
    /// Its slot.
    pub slot: u32,
    /// Its switch.
    pub switch: String,
    /// Its MAC address.
    pub mac: String,
    /// Whether it has link: its port is open on a running partition.
    pub link: bool,
}
wire_struct!(Link {
    slot,
    switch,
    mac,
    link
});

/// One partition, as a list shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// Its ID.
    pub id: PartitionId,
    /// Its name.
    pub name: String,
    /// Its state.
    pub state: PartitionState,
    /// Its current reference code; empty when it has posted none.
    pub code: String,
    /// The code's text.
    pub text: String,
    /// What it holds; `None` when it is not activated.
    pub current: Option<Current>,
    /// The profile it was last activated with.
    pub profile: String,
    /// How its VMM last ended (`exit 0`, `signal 9`); empty when it
    /// has not.
    pub last_end: String,
    /// Why it is not activated or in error, when no request said.
    pub message: Option<Note>,
    /// Its virtual Ethernet adapters: the running profile's, else the
    /// default profile's.
    pub ethernet: Vec<Link>,
    /// Its service channel and what the system reported.
    pub channel: Channel,
}
wire_struct!(Status {
    id,
    name,
    state,
    code,
    text,
    current,
    profile,
    last_end,
    message,
    ethernet,
    channel
});

/// Ask for every partition.
pub type List = Empty;

/// Ask for one partition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Get {
    /// The partition.
    pub id: PartitionId,
}
wire_struct!(Get { id });

/// One partition whole.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Partition {
    /// Its definition.
    pub definition: Definition,
    /// Its state, code, resources and channel.
    pub status: Status,
}
wire_struct!(Partition { definition, status });

/// Ask for the virtual Ethernet adapters of a partition's profiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adapters {
    /// The partition; 0 for every partition.
    pub id: PartitionId,
}
wire_struct!(Adapters { id });

/// A virtual Ethernet adapter of a profile.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Adapter {
    /// The partition.
    pub partition: PartitionId,
    /// Its name.
    pub name: String,
    /// The profile.
    pub profile: String,
    /// The slot.
    pub slot: u32,
    /// The switch.
    pub switch: String,
    /// The port VLAN.
    pub vlan: u16,
    /// The tagged VLANs.
    pub tagged: Vec<u16>,
    /// The MAC address.
    pub mac: String,
    /// Whether the partition is activated only with its port.
    pub required: bool,
}
wire_struct!(Adapter {
    partition,
    name,
    profile,
    slot,
    switch,
    vlan,
    tagged,
    mac,
    required
});

/// A partition's state changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateChanged {
    /// The partition.
    pub id: PartitionId,
    /// Its name.
    pub name: String,
    /// The state it is in now.
    pub state: PartitionState,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(StateChanged {
    id,
    name,
    state,
    time
});

/// What a partition holds changed: given at an activation, given back
/// at an end, or its entitlement changed while it runs (the partition
/// family's version 2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourcesChanged {
    /// The partition.
    pub id: PartitionId,
    /// Its name.
    pub name: String,
    /// What it holds now; `None` when it has ended.
    pub current: Option<Current>,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(ResourcesChanged {
    id,
    name,
    current,
    time
});

/// A partition was created, changed or deleted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Defined {
    /// The partition.
    pub id: PartitionId,
    /// Its name.
    pub name: String,
    /// Whether its definition still exists.
    pub exists: bool,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(Defined {
    id,
    name,
    exists,
    time
});

/// A partition's service channel came up or went, stopped or started
/// answering, or its system reported a change of itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelChanged {
    /// The partition.
    pub id: PartitionId,
    /// Its name.
    pub name: String,
    /// The channel as it is now.
    pub channel: Channel,
    /// When, seconds since the epoch.
    pub time: u64,
}
wire_struct!(ChannelChanged {
    id,
    name,
    channel,
    time
});

/// Ask when each partition was activated
/// ([`crate::caps::ACTIVATIONS`]).
pub type Activations = Empty;

/// A partition's current activation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Activation {
    /// The partition.
    pub id: PartitionId,
    /// When its current activation began, seconds since the epoch;
    /// `None` when it is not activated.
    pub since: Option<u64>,
    /// The profile it was activated with; empty when never.
    pub profile: String,
}
wire_struct!(Activation { id, since, profile });

/// Operation numbers.
pub mod op {
    /// [`super::List`].
    pub const LIST: u16 = 0x0001;
    /// [`super::Get`].
    pub const GET: u16 = 0x0002;
    /// [`super::Adapters`].
    pub const ADAPTERS: u16 = 0x0003;
    /// [`super::Activations`] ([`crate::caps::ACTIVATIONS`]).
    pub const ACTIVATIONS: u16 = 0x0004;

    /// Whether `op` is the activation times', which needs
    /// [`crate::caps::ACTIVATIONS`].
    #[must_use]
    pub fn is_activations(op: u16) -> bool {
        op == ACTIVATIONS
    }
}

/// Event numbers.
pub mod event {
    /// [`super::StateChanged`].
    pub const STATE: u16 = 0x0001;
    /// [`super::ResourcesChanged`].
    pub const RESOURCES: u16 = 0x0002;
    /// [`super::Defined`].
    pub const DEFINED: u16 = 0x0003;
    /// [`super::ChannelChanged`].
    pub const CHANNEL: u16 = 0x0004;
}

/// The family's own refusals (`QSI0330` to `QSI0359`).
pub mod msgid {
    /// No partition of that ID, from a managed side that has no
    /// identifier of its own for it (a host answers with its own).
    pub const NO_PARTITION: &str = "QSI0331";
}

messages! {
    /// What a console asks.
    pub enum Request: family::PARTITIONS => {
        /// Every partition, in ID order.
        0x0001 => List(List),
        /// One partition whole.
        0x0002 => Get(Get),
        /// The Ethernet adapters of its profiles.
        0x0003 => Adapters(Adapters),
        /// When each was activated.
        0x0004 => Activations(Activations),
    }
}

messages! {
    /// What the host answers.
    pub enum Reply: family::PARTITIONS => {
        /// Every partition, in ID order.
        0x0001 => List(Vec<Status>),
        /// One partition whole.
        0x0002 => Get(Partition),
        /// The adapters.
        0x0003 => Adapters(Vec<Adapter>),
        /// Each partition's activation, in ID order.
        0x0004 => Activations(Vec<Activation>),
    }
}

messages! {
    /// What the host pushes.
    pub enum Event: family::PARTITIONS => {
        /// A state changed.
        0x0001 => State(StateChanged),
        /// What a partition holds changed.
        0x0002 => Resources(ResourcesChanged),
        /// A partition was created, changed or deleted.
        0x0003 => Defined(Defined),
        /// A service channel changed.
        0x0004 => Channel(ChannelChanged),
    }
}

impl Request {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        match self {
            Self::List(_) | Self::Activations(_) => Ok(()),
            Self::Get(g) => check_id(g.id),
            Self::Adapters(a) if a.id == 0 => Ok(()),
            Self::Adapters(a) => check_id(a.id),
        }
    }
}

/// The partitions family.
#[derive(Debug)]
pub struct Partitions;

impl Family for Partitions {
    const ID: u16 = family::PARTITIONS;
    const VERSION: u16 = 1;
    const CAP: u64 = crate::caps::PARTITIONS;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
