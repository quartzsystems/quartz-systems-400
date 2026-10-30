// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Qshell in a QS/400 job.
//!
//! A job's runtime (its link to the storage service, the command
//! analyzer, the ILE job) lives on the job's main thread and is not
//! `Send`; the shell runs on worker threads and reaches the job through
//! `qsh::sys::Sys`. The two meet at a [`broker`]: [`JobSys`] (the `Sys`)
//! sends each operation as a task, and the job's thread serves tasks
//! against its [`Side`], the trait the job implements (`work/qsjob`) and
//! tests implement over an in-memory IFS.
//!
//! - [`side`]: what the job answers: the IFS client, CL, SQL, programs,
//!   child jobs, the objects Qshell's utilities read.
//! - [`broker`]: tasks from worker threads to the job's thread.
//! - [`sys`]: [`JobSys`], files as IFS descriptors, utilities and the
//!   shell itself on threads, `*PGM` objects and CL in the job, other
//!   programs as child jobs.
//! - [`run`]: the shell on a worker thread while the job serves:
//!   `QSH CMD()` ([`run::batch`]), `qsh` arguments ([`run::invoke`]) and
//!   the line-fed terminal session ([`run::Session`]).
//!
//! `docs/QSH.md` ("QSH in a job") is the reference.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod broker;
pub mod run;
pub mod side;
pub mod sys;

pub use broker::{Broker, Desk};
pub use side::{ChildKind, Ids, JobSpawn, ProgramRun, Side};
pub use sys::{JobSys, Setup};
