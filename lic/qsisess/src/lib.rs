// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSI's session over any byte stream (`docs/QSI.md`): the hello each
//! way, requests and their replies or refusals, events with sequence
//! numbers and a replay log, streams with credit, keep-alive and a
//! clean close.
//!
//! A [`Session`] does no I/O. Its caller feeds it the bytes the
//! connection gave ([`Session::feed`]) and the clock ([`Session::tick`]),
//! sends what it produces ([`Session::take_output`]) and acts on what
//! it hands up ([`Session::poll`]). The same type serves both ends: the
//! console ([`Role::Console`]) asks, the managed side ([`Role::Managed`])
//! answers and pushes events. Frames are `qsipart`'s ([`wire`] has the
//! session's kinds); the peer is treated as hostile, and what it does
//! wrong becomes a refusal or a close with a named message identifier
//! ([`qsi::msgid`]), never a panic.

#![forbid(unsafe_code)]

mod error;
pub mod fuzzing;
mod limits;
mod log;
pub mod loopback;
mod message;
mod session;
pub mod wire;

pub use error::Error;
pub use limits::Limits;
pub use log::EventLog;
pub use message::{Event, Incoming, MAX_ALSO, MAX_HELP, MAX_MSGID, Note, Outcome, Reply, Request};
pub use session::{Role, Session, StreamId};
