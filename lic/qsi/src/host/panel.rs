// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The panel family: a partition's control panel functions, addressed
//! by partition ID. Each answers with the text it displays. The host
//! carries out every function but 21, which the partition's system
//! does over its service channel; which functions there are, and which
//! need the keylock in manual, is the host's to say (a function it does
//! not offer is refused with its own message).

use ipc::wire_struct;

use super::model::{IplSource, IplType, PartitionId, check_id};
use crate::base::Keylock;
use crate::{Family, family, messages};

/// The functions a host has, by number.
pub mod function {
    /// Display the IPL source, the IPL type and the keylock.
    pub const DISPLAY_IPL: u8 = 1;
    /// Select the IPL source, IPL type and keylock for the next
    /// activation ([`super::PanelArgs`]).
    pub const SELECT_IPL: u8 = 2;
    /// Restart the partition's IPL now.
    pub const RESTART: u8 = 3;
    /// Power the partition off now.
    pub const POWER_OFF: u8 = 8;
    /// Display word 1 of the current reference code; 12 to 19 are its
    /// words 2 to 9.
    pub const WORD_1: u8 = 11;
    /// Display word 9.
    pub const WORD_9: u8 = 19;
    /// Display the partition's type, serial number and releases.
    pub const IDENTITY: u8 = 20;
    /// Dedicated service tools on the partition's console (the system's).
    pub const DST: u8 = 21;
    /// Dump the partition's memory, then restart its IPL.
    pub const DUMP: u8 = 22;
    /// Take the dump again from the memory still held.
    pub const DUMP_AGAIN: u8 = 34;
}

/// The highest function number a panel has: two digits.
pub const MAX_FUNCTION: u8 = 99;

/// Function 02's selection for the next activation: what is left out is
/// not changed. Empty for every other function.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PanelArgs {
    /// The IPL source.
    pub source: Option<IplSource>,
    /// The IPL type.
    pub ipl_type: Option<IplType>,
    /// The keylock position.
    pub keylock: Option<Keylock>,
}
wire_struct!(PanelArgs {
    source,
    ipl_type,
    keylock
});

/// Carry out a function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Function {
    /// The partition.
    pub id: PartitionId,
    /// The function's number ([`function`]).
    pub function: u8,
    /// Its arguments.
    pub args: PanelArgs,
}
wire_struct!(Function { id, function, args });

/// What a function displays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    /// Its text, in the host's words; empty when it displays nothing.
    pub text: String,
}
wire_struct!(Shown { text });

/// Operation numbers.
pub mod op {
    /// [`super::Function`].
    pub const FUNCTION: u16 = 0x0001;
}

/// The family's own refusals (`QSI0450` to `QSI0499`): none in
/// version 1; a host refuses with its own.
pub mod msgid {}

messages! {
    /// What a console asks.
    pub enum Request: family::PANEL => {
        /// Carry out a function.
        0x0001 => Function(Function),
    }
}

messages! {
    /// What the host answers.
    pub enum Reply: family::PANEL => {
        /// What it displays.
        0x0001 => Function(Shown),
    }
}

messages! {
    /// The family has no events: what a function does comes as the
    /// partitions and reference code families' events.
    pub enum Event: family::PANEL => {}
}

impl Request {
    /// The protocol's bounds.
    pub fn check(&self) -> Result<(), &'static str> {
        let Self::Function(f) = self;
        check_id(f.id)?;
        if f.function == 0 || f.function > MAX_FUNCTION {
            return Err("a function outside 1 to 99");
        }
        Ok(())
    }
}

/// The panel family.
#[derive(Debug)]
pub struct Panel;

impl Family for Panel {
    const ID: u16 = family::PANEL;
    const VERSION: u16 = 1;
    const CAP: u64 = crate::caps::PANEL;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
