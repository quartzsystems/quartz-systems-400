// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database: tables, Run SQL, saved requests, data transfer
//! (handoff steps 7-12). The area agent grows this trait with what its
//! pages read and run; every change runs as the signed-on user.

/// The database: tables, Run SQL, saved requests, data transfer.
pub trait Db: Send + Sync {}
