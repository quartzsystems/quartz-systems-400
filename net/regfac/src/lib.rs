// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The registration facility: exit points and their exit programs.
//! The repository is the `*EXITRG` object `QUSRSYS/QUSEXRGOBJ`
//! ([`store`], [`payload`]) holding a [`Registry`]; the storage service
//! answers every job's [`Query`] over it, for the APIs (`QUSRTVEI`,
//! `QUSADDEP` ...; receivers in [`exti`]), the commands (`ADDEXITPGM`,
//! `RMVEXITPGM`, `WRKREGINF`) and the servers. A server calls a point's
//! programs with [`call_exit`], in the point's format ([`formats`]),
//! through a [`ProgramCaller`] over its own job. `docs/HOSTSVR.md` (§
//! Exit points) has the layouts.

#![deny(missing_docs)]
#![allow(clippy::many_single_char_names)]

pub mod call;
pub mod error;
pub mod exti;
pub mod formats;
pub mod mem;
pub mod model;
pub mod payload;
pub mod query;
pub mod registry;
pub mod shipped;
pub mod spec;
pub mod store;
pub mod text;

pub use call::{Called, ExitAnswer, ExitRegistry, ProgramCaller, call_exit};
pub use error::{RegFail, RegResult};
pub use formats::ExitRequest;
pub use mem::MemRegistry;
pub use model::{Desc, ExitPoint, ExitProgram, MtAction, Prep, ThreadSafe};
pub use query::{Answer, Outcome, Query, answer};
pub use registry::{AddOpts, HIGH, LOW, Registry, Which};
