// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The external descriptions the RPG compiler reads: a database file
//! (PF, LF, SQL table or view) from its stored description, display
//! and printer files from their compiled definitions. The storage
//! service's compile commands and the conformance kit build them here
//! the same way.

#![deny(missing_docs)]

mod database;
mod device;

pub use database::{database_desc, field_type, format_of};
pub use device::{display_desc, printer_desc};
