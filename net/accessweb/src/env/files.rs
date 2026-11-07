// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The integrated file system: browse, properties, edit, shares
//! (handoff steps 13-14). The area agent grows this trait with what its
//! pages read and run; every change runs as the signed-on user.

/// The integrated file system: browse, properties, edit, shares.
pub trait Files: Send + Sync {}
