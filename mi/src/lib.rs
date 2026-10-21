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

pub mod cond;
pub mod exec;
pub mod job;
pub mod rtlib;
pub mod space;

pub use cond::{Cond, Exception};
pub use error::{MiError, Result};
pub use exec::Program;
pub use job::Job;
pub use space::{FatPtr, PtrKind, Spaces};
