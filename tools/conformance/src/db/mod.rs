// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The database conformance kit: the fixed `QSTEST` library built from
//! `conformance/db/`, run in memory against the `db` and `sql` crates,
//! compared with the outputs of a real IBM i, and driven through the
//! service console under QEMU.

pub mod compare;
pub mod kit;
pub mod local;
mod qemu;
pub mod rpg;

pub use compare::{Comparison, compare};
pub use kit::Kit;
pub use local::Local;
