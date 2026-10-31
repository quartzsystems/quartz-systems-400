// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The save and restore engine: objects of the store into a save stream
//! (`savfmt`) and back, with IBM's rules for what a restore does to an
//! object that exists, its members, owner, authorities and journaling.
//! The commands (`SAVLIB`, `RSTOBJ`, ...) are thin layers over
//! [`save::save_library`] and [`restore::restore`].

#![deny(missing_docs)]

mod error;
pub mod hooks;
mod ids;
pub mod image;
pub mod msg;
pub mod order;
pub mod restore;
pub mod save;
pub mod secdta;
pub mod select;

pub use error::{Error, Result};
pub use hooks::{AllowAll, Hooks};
pub use restore::{AllowDiff, MbrOpt, RestoreOptions, RestoreReport, RstOption, restore};
pub use save::{ObjResult, SaveOptions, SaveReport, SaveTarget, save_library};
pub use select::ObjSel;
