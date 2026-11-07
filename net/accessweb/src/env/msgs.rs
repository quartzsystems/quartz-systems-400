// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Messages: the user's queue, other queues, send, reply
//! (handoff step 16). The area agent grows this trait with what its
//! pages read and run; every change runs as the signed-on user.

/// Messages: the user's queue, other queues, send, reply.
pub trait Msgs: Send + Sync {}
