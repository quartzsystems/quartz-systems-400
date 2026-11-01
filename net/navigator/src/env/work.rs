// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Work management: active jobs, job queues, subsystems, output queues and spooled files, writers, the job scheduler.

/// What the work management pages need.
pub trait Work: Send + Sync {}
