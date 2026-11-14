// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Access control (docs/DIRSRV.md §12): owners and `aclEntry` values
//! ([`model`]) and their evaluation ([`eval`]).

pub mod eval;
pub mod model;

pub use eval::{Effective, Principal};
pub use model::{A, AclEntry, AclError, C, D, Kind, Pseudo, R, S, Subject, Target, W};

/// The ACL a suffix has when no entry sets one (the primary's).
pub const DEFAULT_ACL: &str = "group:CN=ANYBODY:normal:rsc:system:rsc:restricted:rsc";
