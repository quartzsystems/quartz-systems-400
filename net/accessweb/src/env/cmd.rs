// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Commands: run, prompt, saved commands
//! (handoff steps 18-20). The area agent grows this trait with what its
//! pages read and run; every change runs as the signed-on user.

/// Commands: run, prompt, saved commands.
pub trait Cmd: Send + Sync {}
