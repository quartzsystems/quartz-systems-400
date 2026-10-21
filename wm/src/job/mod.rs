// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The job: identity, type and status.

mod attrs;
mod id;
mod kind;
mod status;

pub use attrs::{JobAttrs, LibList, SysvalFn};
pub use id::{InternalId, JobId, MAX_NUMBER, next_number};
pub use kind::{JobType, SubType, wrkactjob_type};
pub use status::{JobState, Status};
