// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Jobs: the user's jobs and the server jobs
//! (handoff step 17). The area agent grows this trait with what its
//! pages read and run; every change runs as the signed-on user.

/// Jobs: the user's jobs and the server jobs.
pub trait Jobs: Send + Sync {}
