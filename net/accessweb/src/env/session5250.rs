// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! 5250 sessions: saved session files, the telnet client, the screen
//! (handoff steps 5-6b). The area agent grows this trait with what its
//! pages read and run; every change runs as the signed-on user.

/// 5250 sessions: saved session files, the telnet client, the screen.
pub trait Session5250: Send + Sync {}
