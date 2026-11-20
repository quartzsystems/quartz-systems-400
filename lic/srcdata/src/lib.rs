// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The system reference codes as data: every code the system posts, its
//! family and text, and its meaning, cause and recovery where written.
//!
//! Families: `C1` LIC init, `C6` OS init and install, `A6` attention
//! (non-fatal), `B1` fatal, `D1` power down.

mod gaps;
mod model;
mod table;

pub use gaps::{Gaps, gaps, unexplained};
pub use model::{Family, Phase, Src};
pub use table::*;
