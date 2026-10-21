// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The machine interface runtime: the job stand-in with its pending
//! exception, fat pointers and the spaces they address, and the library
//! of functions generated code calls through the runtime table for
//! decimal, string, conversion, date and pointer work. `docs/ABI.md`
//! names the functions and `docs/ILE.md` the model.

#![deny(missing_docs)]

mod error;

pub mod act;
pub mod cee;
pub mod cond;
pub mod dump;
pub mod except;
pub mod exec;
pub mod heap;
pub mod io;
pub mod job;
pub mod msg;
pub mod rtlib;
pub mod space;
pub mod storage;
pub mod usrspc;

pub use act::{ActivationGroup, AgId, AgKind, LoadedProgram, MemSource, ObjectSource};
pub use cond::{Cond, Exception};
pub use error::{MiError, Result};
pub use exec::Program;
pub use heap::{Heap, HeapId};
pub use job::Job;
pub use msg::{ClearMode, Message, MsgQueue, QueueRef};
pub use space::{FatPtr, PtrKind, Spaces};
pub use usrspc::{MemUserSpaces, UserSpaceSource};
