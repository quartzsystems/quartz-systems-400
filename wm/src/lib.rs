// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management: what a job is, how it is started, controlled and
//! ended. Every job is one Linux process running `qsjob`; [`ctl`] is
//! the channel its parent holds to it. The objects that describe work
//! (`*JOBD`, `*CLS`, ...) are in [`obj`]. `docs/WORKMGMT.md` describes
//! the model and the payload layouts.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod cmd;
pub mod ctl;
mod error;
pub mod job;
pub mod joblog;
pub mod obj;
pub mod rec;
pub mod spec;
pub mod table;

pub use error::{Error, Result};
