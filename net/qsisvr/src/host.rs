// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the families need of the system ([`Host`]), and the far ends of
//! the streams a request opens ([`Opener`], [`Pipe`]). The job answers
//! them through the storage service; [`crate::mem`] answers them in
//! memory.

use qsi::base::{Identity, Level, Power, RefcodeList, Space, State, SystemDump, Time};
use qsi::host::console::Capture;
use qsi::service;
use qsisess::Note;

/// Why a request was refused: every message that applies, the first
/// first.
pub type Refusal = Vec<Note>;

/// A refusal of one message.
#[must_use]
pub fn refusal(msgid: &str, text: &str) -> Refusal {
    vec![Note::new(msgid, text)]
}

/// The system, as the families ask it. `user` is the name the console
/// gave for the request: it is for the log and grants nothing.
pub trait Host {
    /// What the system says of itself.
    ///
    /// # Errors
    /// The system could not be asked.
    fn identity(&mut self) -> Result<Identity, Refusal>;
    /// Its power state, current code, release, slot and keylock.
    ///
    /// # Errors
    /// The system could not be asked.
    fn state(&mut self) -> Result<State, Refusal>;
    /// Power down: off or restart, controlled within the delay or at
    /// once.
    ///
    /// # Errors
    /// A power down under way, or the system's refusal.
    fn power(&mut self, power: &Power, user: &str) -> Result<(), Refusal>;
    /// The reference codes before `before` (from the newest without
    /// one), newest first, at most `limit`.
    ///
    /// # Errors
    /// The history could not be read.
    fn refcodes(&mut self, before: Option<u64>, limit: u32) -> Result<RefcodeList, Refusal>;
    /// The clock and the zone.
    ///
    /// # Errors
    /// The system could not be asked.
    fn time(&mut self) -> Result<Time, Refusal>;
    /// The cumulative fix level.
    ///
    /// # Errors
    /// The system could not be asked.
    fn fix_level(&mut self) -> Result<Level, Refusal>;
    /// The system's dumps, oldest first.
    ///
    /// # Errors
    /// The system could not be asked.
    fn dumps(&mut self) -> Result<Vec<SystemDump>, Refusal>;
    /// Take a system dump; its name.
    ///
    /// # Errors
    /// One is being taken, or it could not be.
    fn take_dump(&mut self, why: &str, user: &str) -> Result<String, Refusal>;
    /// Delete dump `name`.
    ///
    /// # Errors
    /// No such dump.
    fn delete_dump(&mut self, name: &str, user: &str) -> Result<(), Refusal>;
    /// What the dumps take.
    ///
    /// # Errors
    /// The system could not be asked.
    fn dump_space(&mut self) -> Result<Space, Refusal>;
    /// A request of the service family, answered from the problem log.
    ///
    /// # Errors
    /// No such problem, closed already, or the log could not be read.
    fn problems(
        &mut self,
        request: &service::Request,
        user: &str,
    ) -> Result<service::Reply, Refusal>;
    /// Dedicated service tools on the system's console (function 21).
    ///
    /// # Errors
    /// The system's refusal: already showing, not in manual.
    fn service_tools(&mut self, user: &str) -> Result<(), Refusal>;
    /// What the console shows now.
    ///
    /// # Errors
    /// The console cannot be reached.
    fn screen(&mut self) -> Result<Capture, Refusal>;
}

/// What a pipe has for the console now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Flow {
    /// Bytes to send.
    Bytes(Vec<u8>),
    /// Nothing yet.
    Idle,
    /// No more: the stream closes, or is reset with a message.
    End(Option<Note>),
}

/// A stream's far end: the console's viewer, a dump being read.
pub trait Pipe {
    /// What there is for the console, without waiting.
    fn poll(&mut self) -> Flow;
    /// Bytes the console sent.
    ///
    /// # Errors
    /// The far end takes no more: the stream is reset with the message.
    fn put(&mut self, bytes: &[u8]) -> Result<(), Note>;
    /// The console will send no more.
    fn closed(&mut self) {}
    /// The stream is over, either way: let go of the far end.
    fn reset(&mut self) {}
}

/// What opens a stream's far end.
pub trait Opener {
    /// Attach to the system's console: its pipe.
    ///
    /// # Errors
    /// Another viewer has it (`QSI0601`), or it cannot be reached.
    fn console(&mut self, take_over: bool) -> Result<Box<dyn Pipe>, Refusal>;
    /// Read dump `name`: its size and its pipe.
    ///
    /// # Errors
    /// No such dump.
    fn dump(&mut self, name: &str) -> Result<(u64, Box<dyn Pipe>), Refusal>;
}
