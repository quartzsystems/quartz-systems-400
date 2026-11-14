// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Passwords (docs/DIRSRV.md §11): storage schemes ([`scheme`], with
//! `crypt(3)` in [`crypt`]) and password policy ([`policy`]).

pub mod crypt;
pub mod policy;
pub mod scheme;

pub use policy::{Partial, Policy, State};
pub use scheme::{Scheme, verify};
