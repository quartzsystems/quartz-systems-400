// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QS/400's QSI server (`docs/QSID.md` section 12): a standalone system
//! answering a management console over QSI (`docs/QSI.md`), on the
//! three neutral crates `qsi`, `qsisess` and `qsitrust`.
//!
//! - [`gate`]: the limits before authentication, as QS/VS's server has
//!   them;
//! - [`serve`]: the listener, the TLS handshake and each session's
//!   loop;
//! - [`conn`]: one connection after TLS, its session, pairing, streams
//!   and events;
//! - [`families`]: what a standalone system answers, over a [`Host`];
//! - [`trust`]: the pins as the handshakes judge them, and the
//!   [`trust::Keeper`] of the password and the pins;
//! - [`pump`]: the events, from what changed;
//! - [`history`]: the reference codes `slic` posts, as it writes them;
//! - [`store`]: the files the server keeps (frozen formats);
//! - [`mem`]: a system in memory, for the tests and the fuzzer.
//!
//! The job (`work/qsjob/src/work/qsi/`) runs it over the storage service.

#![forbid(unsafe_code)]

pub mod client;
pub mod conn;
pub mod families;
pub mod fuzzing;
pub mod gate;
pub mod history;
pub mod host;
pub mod mem;
pub mod pump;
pub mod serve;
pub mod store;
pub mod trust;

pub use host::{Flow, Host, Opener, Pipe, Refusal, refusal};
