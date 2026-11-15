// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The pairing family: the one thing a peer that is not pinned may do.
//! The bodies carry `qsitrust::pair`'s messages field for field; the
//! exchange and its checks are there.
//!
//! 1. The console sends [`Start`]; the managed side answers [`Answer`].
//! 2. The console checks the answer and sends [`Confirm`]; the managed
//!    side answers [`Done`] and pins the console. The console pins the
//!    managed side on `Done`.
//!
//! A failure at either step is a refusal: `QSI0201` when no password
//! is out, `QSI0202` when it did not confirm.

use ipc::wire_struct;

use crate::message::Empty;
use crate::{Family, family, messages};

/// The console's first message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Start {
    /// SPAKE2's message A.
    pub spake: Vec<u8>,
    /// The console's name.
    pub name: String,
}
wire_struct!(Start { spake, name });

/// The managed side's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    /// SPAKE2's message B.
    pub spake: Vec<u8>,
    /// The managed side's confirmation.
    pub confirm: Vec<u8>,
    /// The managed side's name.
    pub name: String,
}
wire_struct!(Answer {
    spake,
    confirm,
    name
});

/// The console's confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    /// The console's confirmation.
    pub confirm: Vec<u8>,
}
wire_struct!(Confirm { confirm });

/// Pairing is done: the managed side has pinned the console.
pub type Done = Empty;

/// Operation numbers.
pub mod op {
    /// [`super::Start`] / [`super::Answer`].
    pub const START: u16 = 0x0001;
    /// [`super::Confirm`] / [`super::Done`].
    pub const CONFIRM: u16 = 0x0002;
}

messages! {
    /// What a console asks.
    pub enum Request: family::PAIR => {
        /// Begin.
        0x0001 => Start(Start),
        /// Confirm.
        0x0002 => Confirm(Confirm),
    }
}

messages! {
    /// What the managed side answers.
    pub enum Reply: family::PAIR => {
        /// The answer to a start.
        0x0001 => Answer(Answer),
        /// The answer to a confirmation.
        0x0002 => Done(Done),
    }
}

messages! {
    /// The family has no events.
    pub enum Event: family::PAIR => {}
}

/// The pairing family.
#[derive(Debug)]
pub struct Pair;

impl Family for Pair {
    const ID: u16 = family::PAIR;
    const VERSION: u16 = 1;
    const CAP: u64 = crate::caps::PAIR;
    type Request = Request;
    type Reply = Reply;
    type Event = Event;
}
