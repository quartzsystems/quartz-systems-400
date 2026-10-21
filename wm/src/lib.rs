// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management: what a job is, how it is started, controlled and
//! ended. Every job is one Linux process running `qsjob`; [`ctl`] is
//! the channel its parent holds to it. `docs/WORKMGMT.md` describes
//! the model.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod cmd;
pub mod ctl;
pub mod job;
pub mod table;
