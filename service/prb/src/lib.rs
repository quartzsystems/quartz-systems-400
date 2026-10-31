// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The problem log (`docs/SERVICE.md`): a problem entry for each failure
//! the system detects (a severe `QSYSOPR` message, a hardware entry in
//! the product activity log, a dump) or a user reports through
//! `ANZPRB`; its status from opened to closed; the notes `WRKPRB`
//! option 12 adds; and the report a "report to the service provider"
//! sends. The integrator keeps the encoded log.

#![deny(missing_docs)]

pub mod analyze;
mod log;
mod problem;

pub use log::{Filter, ProblemLog};
pub use problem::{Origin, Problem, PrbStatus, PrbType};
