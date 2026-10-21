// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management roles of a job process: starting other job
//! processes, the storage pools' cgroups, the subsystem monitor and the
//! routed (batch) job.

pub mod batch;
pub mod cgroup;
pub mod inter;
pub mod monitor;
pub mod scheduler;
pub mod spawn;
pub mod writer;
